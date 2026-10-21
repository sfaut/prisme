//! Session on a target server: opens the connection, then runs the client's
//! requests (queries, database changes) until the session stops. The queries
//! themselves run in `execution.rs`, and their row limit is set in `limits.rs`.
//!
//! The session remembers its current database: it reads it from the server
//! (`SELECT DATABASE()`) when it opens, and again only after a query that can
//! change it (`USE`...).

use std::sync::{Arc, Mutex};

use prisme_connectors::mysql::{self, Connection, MysqlTarget};
use prisme_dialects::Dialect;
use prisme_splitter::split;
use tokio::sync::{mpsc, watch};
use tracing::Instrument;

use super::cancellation::Cancellation;
use super::execution::chosen_statements;
use super::limits::row_limit;
use super::outbox::{Outbox, SessionStopped};
use super::protocol::{BatchQuery, QueryRequest, ServerMessage, SessionRequest};
use crate::analyze::flavor_of;
use crate::api::AppState;
use crate::console::{self, Action};
use crate::error_detail::ErrorDetail;

/// Runs a session: connection to the target server, then the client's
/// requests until the session stops.
pub(super) async fn serve(
    outbox: Outbox,
    mut requests: mpsc::Receiver<SessionRequest>,
    mut stop: watch::Receiver<bool>,
    state: AppState,
    user_id: i64,
    target: MysqlTarget,
    cancellation: Arc<Cancellation>,
) {
    let database = Mutex::new(None);
    let session = Session {
        outbox: &outbox,
        target: &target,
        state: &state,
        user_id,
        cancellation: &cancellation,
        database: &database,
        batch: 0,
    };
    let opening = console::action_span(Action::Session);
    let Some(mut connection) = session.open().instrument(opening.clone()).await else {
        return;
    };
    if let Ok(dialect) = session.ready(&mut connection).instrument(opening).await {
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
                        .handle(&mut connection, dialect, request)
                        .instrument(span)
                        .await
                }
                SessionRequest::Use(database) => {
                    session
                        .use_database(&mut connection, dialect, &database)
                        .instrument(console::action_span(Action::Use))
                        .await
                }
            };
            if handled.is_err() {
                break;
            }
        }
    }
    let _ = connection.disconnect().await;
}

/// Action of a request, for the history of the queries: a refresh, the
/// query under the cursor, or the queries run in parallel.
fn action_of(request: &QueryRequest) -> Action {
    match (request.tab, request.cursor) {
        (Some(_), _) => Action::Refresh,
        (None, Some(_)) => Action::Run,
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
    /// Connection settings, for the extra connections and the interruptions.
    pub(super) target: &'a MysqlTarget,
    /// State of the application, to read the settings.
    pub(super) state: &'a AppState,
    /// User of the session.
    pub(super) user_id: i64,
    /// Stopping of the running queries, requested by the client.
    pub(super) cancellation: &'a Cancellation,
    /// Current database of the session's connection, as last read.
    pub(super) database: &'a Mutex<Option<String>>,
}

/// Summary of a result read to the end.
pub(super) struct Summary {
    pub(super) row_count: usize,
    pub(super) truncated: bool,
    pub(super) affected_rows: u64,
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

    /// Opens the session's connection; on failure, the error goes to the client.
    async fn open(&self) -> Option<Connection> {
        match mysql::connect(self.target).await {
            Ok(connection) => Some(connection),
            Err(error) => {
                let detail = ErrorDetail::of_mysql(&error, None);
                let _ = self.outbox.error(None, detail).await;
                None
            }
        }
    }

    /// Announces the session to the client: identity of the server and current
    /// database. Returns the server's dialect.
    async fn ready(&self, connection: &mut Connection) -> Result<Dialect, SessionStopped> {
        let info = match mysql::server_info(connection).await {
            Ok(info) => info,
            Err(error) => {
                let detail = ErrorDetail::of_mysql(&error, None);
                self.outbox.error(None, detail).await?;
                return Err(SessionStopped);
            }
        };
        let database = match mysql::current_database(connection).await {
            Ok(database) => database,
            Err(error) => {
                let detail = ErrorDetail::of_mysql(&error, Some(info.flavor));
                self.outbox.error(None, detail).await?;
                return Err(SessionStopped);
            }
        };
        self.remember_database(&database);
        let dialect = crate::analyze::dialect_of(info.flavor);
        self.outbox
            .send(ServerMessage::Ready {
                server: info,
                database,
            })
            .await?;
        Ok(dialect)
    }

    /// Changes the session's current database and announces the new database
    /// to the client; a failure (unknown database, privileges) goes as a
    /// session error.
    async fn use_database(
        &self,
        connection: &mut Connection,
        dialect: Dialect,
        database: &str,
    ) -> Result<(), SessionStopped> {
        let changed = match mysql::use_database(connection, database).await {
            Ok(()) => mysql::current_database(connection).await,
            Err(error) => Err(error),
        };
        match changed {
            Ok(database) => {
                self.remember_database(&database);
                self.outbox.send(ServerMessage::Database { database }).await
            }
            Err(error) => {
                let detail = ErrorDetail::of_mysql(&error, Some(flavor_of(dialect)));
                self.outbox.error(None, detail).await
            }
        }
    }

    /// Handles a request of the client: splits the text, then runs the query
    /// under the cursor, or in parallel the queries of the selection, or all
    /// of them. A refresh runs its query alone, like a cursor at the start.
    async fn handle(
        &self,
        connection: &mut Connection,
        dialect: Dialect,
        request: QueryRequest,
    ) -> Result<(), SessionStopped> {
        let max_rows = match row_limit(self.state, self.user_id, request.row_limit).await {
            Ok(max_rows) => max_rows as usize,
            Err(detail) => return self.outbox.error(None, detail).await,
        };
        let statements = split(&request.sql);
        let cursor = request.tab.map_or(request.cursor, |_| Some(0));
        let selection = request.selection.as_deref().unwrap_or_default();
        let chosen = chosen_statements(&statements, cursor, selection);
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
            })
            .collect();
        self.outbox.send(ServerMessage::Batch { queries }).await?;

        if cursor.is_some() {
            self.run_on_session(connection, dialect, &chosen[0].sql, max_rows)
                .await?;
        } else {
            let database = self.current_database();
            self.run_in_parallel(&chosen, dialect, max_rows, &database)
                .await?;
        }
        self.outbox.send(ServerMessage::BatchDone).await
    }
}
