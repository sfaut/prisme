//! Session on a target server: opens the connection, then runs the client's
//! requests (queries, database changes) until the session stops. The queries
//! themselves run in `execution.rs`, and their row limit is set in `limits.rs`.
//!
//! The session remembers its current database: it reads it from the server
//! (`SELECT DATABASE()`) when it opens, and again only after a query that can
//! change it (`USE`...).
//!
//! It also follows the transaction open on its connection: at the end of each
//! request, it tells the client when one opened or ended
//! (`ServerMessage::Transaction`). A connection the server closed (after a
//! `COMMIT RELEASE`, a `COMMIT` with `completion_type = 2`, a `KILL`...)
//! ends the session.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use prisme_connectors::api::{self, Namespace};
use prisme_connectors::drivers;
use prisme_connectors::flavor::Flavor;
use prisme_connectors::outcome::StatementOutcome;
use prisme_dialects::classify::{StatementKind, WriteKind, classify, ends_transaction};
use prisme_dialects::temporary::{TableName, temporary_changes};
use prisme_dialects::verb::StatementVerb;
use prisme_splitter::split;
use tokio::sync::{mpsc, watch};
use tracing::Instrument;

use super::Source;
use super::cancellation::Cancellation;
use super::execution::{chosen_statements, reads_only, selected_pieces};
use super::limits::run_limits;
use super::outbox::{Outbox, SessionStopped};
use super::protocol::{BatchQuery, QueryRequest, ServerMessage, SessionRequest};
use super::temporary_tables::TemporaryTables;
use crate::analyze::dialect_of;
use crate::api::AppState;
use crate::console::{self, Action};
use crate::error_detail::ErrorDetail;

/// Runs a session: connection to the source, then the client's requests
/// until the session stops.
pub(super) async fn serve(
    outbox: Outbox,
    mut requests: mpsc::Receiver<SessionRequest>,
    mut stop: watch::Receiver<bool>,
    state: AppState,
    user_id: i64,
    source: Source,
    cancellation: Arc<Cancellation>,
) {
    let database = Mutex::new(None);
    let temporary = Mutex::new(TemporaryTables::default());
    let transaction = AtomicBool::new(false);
    let session = Session {
        outbox: &outbox,
        source: &source,
        state: &state,
        user_id,
        cancellation: &cancellation,
        database: &database,
        temporary: &temporary,
        transaction: &transaction,
        batch: 0,
    };
    let opening = console::action_span(Action::Session);
    let Some(mut connection) = session.open().instrument(opening.clone()).await else {
        return;
    };
    let ready = session.ready(connection.as_mut()).instrument(opening).await;
    if let Ok(flavor) = ready {
        session.follow_databases(connection.as_mut()).await;
        loop {
            let request = tokio::select! {
                _ = stop.changed() => break,
                request = requests.recv() => request,
            };
            let Some(request) = request else { break };
            if *stop.borrow() {
                break;
            }
            let handled = match request {
                SessionRequest::Run { batch, request } => {
                    let session = Session { batch, ..session };
                    let span = console::action_span(action_of(&request));
                    session
                        .handle(connection.as_mut(), flavor, request)
                        .instrument(span)
                        .await
                }
                SessionRequest::Use(database) => {
                    session
                        .use_database(connection.as_mut(), flavor, &database)
                        .instrument(console::action_span(Action::Use))
                        .await
                }
            };
            if handled.is_err() {
                break;
            }
        }
    }
    connection.close().await;
}

/// Action of a request, for the history of the queries: a refresh, the
/// query under the cursor, the selected text only, or the queries run in
/// parallel.
fn action_of(request: &QueryRequest) -> Action {
    match (request.tab, request.cursor) {
        (Some(_), _) => Action::Refresh,
        (None, Some(_)) => Action::Run,
        (None, None) if request.exact == Some(true) => Action::RunExact,
        (None, None) => Action::RunAll,
    }
}

/// What the queries need while running.
#[derive(Clone, Copy)]
pub(super) struct Session<'a> {
    /// Number of the batch the session runs: its stops are looked up under it.
    pub(super) batch: u64,
    /// Queue of the messages to the client.
    pub(super) outbox: &'a Outbox,
    /// The source, for the connections of the parallel queries.
    pub(super) source: &'a Source,
    /// State of the application, to read the settings.
    pub(super) state: &'a AppState,
    /// User of the session.
    pub(super) user_id: i64,
    /// Stopping of the running queries, requested by the client.
    pub(super) cancellation: &'a Cancellation,
    /// Current database of the session's connection, as last read.
    pub(super) database: &'a Mutex<Option<String>>,
    /// Temporary tables of the session's connection.
    pub(super) temporary: &'a Mutex<TemporaryTables>,
    /// Is a transaction open on the session's connection, as last told to
    /// the client?
    pub(super) transaction: &'a AtomicBool,
}

/// Summary of a result read to the end.
pub(super) struct Summary {
    pub(super) row_count: usize,
    pub(super) truncated: bool,
    /// What the statement does to the data, from its AST.
    pub(super) verb: StatementVerb,
    /// What the statement did, as its engine reports it.
    pub(super) outcome: StatementOutcome,
    pub(super) elapsed_ms: u128,
}

impl Session<'_> {
    /// Current database of the session's connection, as last read.
    pub(super) fn current_database(&self) -> Option<String> {
        self.database
            .lock()
            .map_or(None, |database| database.clone())
    }

    /// Remembers the current database, just read from the server.
    pub(super) fn remember_database(&self, database: &Option<String>) {
        if let Ok(mut current) = self.database.lock() {
            current.clone_from(database);
        }
    }

    /// Is `table`, named in a query run on the current database `database`,
    /// a temporary table of the session's connection?
    pub(super) fn is_temporary(&self, table: &TableName, database: Option<&str>) -> bool {
        self.temporary
            .lock()
            .is_ok_and(|tables| tables.contains(table, database))
    }

    /// Follows the temporary tables that the query `sql`, run on the current
    /// database `database`, created, dropped or renamed; `succeeded` tells
    /// whether it succeeded.
    pub(super) fn follow_temporary(
        &self,
        sql: &str,
        flavor: Flavor,
        database: Option<&str>,
        succeeded: bool,
    ) {
        let changes = temporary_changes(sql, dialect_of(flavor));
        if changes.is_empty() {
            return;
        }
        if let Ok(mut tables) = self.temporary.lock() {
            tables.follow(&changes, database, succeeded);
        }
    }

    /// Opens the session's connection; on failure, the error goes to the client.
    async fn open(&self) -> Option<Box<dyn api::Session>> {
        let target = &self.source.target;
        let opening = drivers::of(target.engine).open(target, self.source.options.clone());
        match opening.await {
            Ok(connection) => Some(connection),
            Err(error) => {
                let detail = ErrorDetail::of_connector(&error, None);
                let _ = self.outbox.error(None, detail).await;
                None
            }
        }
    }

    /// Announces the session to the client: identity of the source and current
    /// database. Returns the source's flavor.
    async fn ready(&self, connection: &mut dyn api::Session) -> Result<Flavor, SessionStopped> {
        let info = connection.info().clone();
        let database = match connection.current_namespace().await {
            Ok(namespace) => database_of(namespace),
            Err(error) => {
                let detail = ErrorDetail::of_connector(&error, Some(info.flavor));
                self.outbox.error(None, detail).await?;
                return Err(SessionStopped);
            }
        };
        self.remember_database(&database);
        let flavor = info.flavor;
        self.outbox
            .send(ServerMessage::Ready {
                server: info,
                database,
                transaction: false,
            })
            .await?;
        Ok(flavor)
    }

    /// Tells the client when a transaction opened or ended on the session's
    /// connection since the last request; `check` makes sure the connection
    /// is still open (after a `COMMIT` or a `ROLLBACK`). A connection lost
    /// (the server closed it) ends the session, with an error for the client.
    async fn follow_transaction(
        &self,
        connection: &mut dyn api::Session,
        check: bool,
    ) -> Result<(), SessionStopped> {
        let Ok(open) = connection.transaction_state(check).await else {
            self.outbox
                .error(None, ErrorDetail::ConnectionClosedByServer)
                .await?;
            return Err(SessionStopped);
        };
        if self.transaction.swap(open, Ordering::Relaxed) == open {
            return Ok(());
        }
        self.outbox.send(ServerMessage::Transaction { open }).await
    }

    /// Changes the session's current database and announces the new database
    /// to the client; a failure (unknown database, privileges) goes as a
    /// session error.
    async fn use_database(
        &self,
        connection: &mut dyn api::Session,
        flavor: Flavor,
        database: &str,
    ) -> Result<(), SessionStopped> {
        let namespace = Namespace::single(database);
        let changed = match connection.switch_namespace(&namespace).await {
            Ok(()) => connection.current_namespace().await,
            Err(error) => Err(error),
        };
        match changed {
            Ok(namespace) => {
                let database = database_of(namespace);
                self.remember_database(&database);
                self.outbox.send(ServerMessage::Database { database }).await
            }
            Err(error) => {
                let detail = ErrorDetail::of_connector(&error, Some(flavor));
                self.outbox.error(None, detail).await
            }
        }
    }

    /// Handles a request of the client: splits the text, then runs the query
    /// under the cursor, or the queries of the selection (or only their
    /// selected text), or all of them: in parallel when they only read and no
    /// transaction is open, otherwise one after the other on the session's
    /// connection, where the transaction lives. A refresh runs its query
    /// alone, like a cursor at the start.
    async fn handle(
        &self,
        connection: &mut dyn api::Session,
        flavor: Flavor,
        request: QueryRequest,
    ) -> Result<(), SessionStopped> {
        let limits = match run_limits(self.state, self.user_id, request.row_limit).await {
            Ok(limits) => limits,
            Err(detail) => return self.outbox.error(None, detail).await,
        };
        let statements = split(&request.sql, dialect_of(flavor).lexical_rules());
        let cursor = request.tab.map_or(request.cursor, |_| Some(0));
        let selection = request.selection.as_deref().unwrap_or_default();
        let exact = request.exact == Some(true) && cursor.is_none();
        let pieces = if exact {
            selected_pieces(&statements, selection)
        } else {
            Vec::new()
        };
        let chosen = if exact {
            pieces.iter().collect()
        } else {
            chosen_statements(&statements, cursor, selection)
        };
        if chosen.is_empty() {
            return self.outbox.error(None, ErrorDetail::NoQuery).await;
        }

        let queries = (0_u32..)
            .zip(chosen.iter())
            .map(|(query, statement)| BatchQuery {
                query,
                tab: request.tab.unwrap_or(0),
                sql: statement.sql.clone(),
                start: statement.start,
                end: statement.end,
                pivot: None,
                grid: None,
            })
            .collect();
        self.outbox.send(ServerMessage::Batch { queries }).await?;

        if cursor.is_some() {
            self.run_on_session(connection, flavor, 0, &chosen[0].sql, &limits)
                .await?;
        } else if self.parallel(connection) && reads_only(&chosen, flavor) {
            let database = self.current_database();
            self.run_in_parallel(&chosen, flavor, &limits, &database)
                .await?;
        } else {
            self.run_in_sequence(connection, &chosen, flavor, &limits)
                .await?;
        }
        let attaches = chosen
            .iter()
            .any(|statement| attaches(&statement.sql, flavor));
        if attaches {
            self.follow_databases(connection).await;
        }
        let ends = chosen
            .iter()
            .any(|statement| ends_transaction(&statement.sql, dialect_of(flavor)));
        self.follow_transaction(connection, ends).await?;
        self.outbox.send(ServerMessage::BatchDone).await
    }
}

impl Session<'_> {
    /// Can the queries of a batch run in parallel, each on its own
    /// connection? Only when the engine opens several connections on the
    /// source, no transaction is open on the session's connection, where it
    /// lives, and that connection holds no database that the others cannot
    /// attach.
    fn parallel(&self, connection: &dyn api::Session) -> bool {
        connection.info().capabilities.parallel_sessions
            && !self.transaction.load(Ordering::Relaxed)
            && !self.source.shared.private()
    }

    /// Lists the databases attached to the session's connection, which its
    /// other connections attach too (`Shared`), and returns whether they
    /// changed. An engine without attachments has none to list.
    async fn follow_databases(&self, connection: &mut dyn api::Session) -> bool {
        if !connection.info().capabilities.attachments {
            return false;
        }
        match connection.databases().await {
            Ok(listed) => self.source.shared.follow(&listed),
            Err(error) => {
                tracing::warn!("cannot list the attached databases: {error}");
                false
            }
        }
    }
}

/// Does the statement `sql` attach or detach a database?
fn attaches(sql: &str, flavor: Flavor) -> bool {
    matches!(
        classify(sql, dialect_of(flavor)),
        StatementKind::Write(kinds) if kinds.contains(&WriteKind::Attach)
    )
}

/// Current database of the session, as the protocol carries it: the
/// innermost level of the namespace, the only level of a MySQL database.
pub(super) fn database_of(namespace: Option<Namespace>) -> Option<String> {
    namespace.map(|namespace| namespace.name().to_string())
}
