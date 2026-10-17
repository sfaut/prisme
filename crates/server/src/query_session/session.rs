//! Session on a target server: opens the connection, then runs the client's
//! requests (queries, database changes) until the session stops. The queries
//! themselves run in `execution.rs`, and their row limit is set in `limits.rs`;
//! those written in another dialect are transpiled (`transpilation.rs`).

use std::sync::Arc;

use prisme_connectors::mysql::{self, Connection, MysqlTarget};
use prisme_dialects::emit::OutputDialect;
use prisme_splitter::split;
use tokio::sync::{mpsc, watch};

use super::cancellation::Cancellation;
use super::execution::chosen_statements;
use super::limits::row_limit;
use super::outbox::{Outbox, SessionStopped};
use super::protocol::{BatchQuery, QueryRequest, ServerMessage, SessionRequest};
use crate::api::AppState;
use crate::transpilation::{DialectChoice, Translation, output_of};

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
    let session = Session {
        outbox: &outbox,
        target: &target,
        state: &state,
        user_id,
        cancellation: &cancellation,
        batch: 0,
        translation: None,
    };
    let Some(mut connection) = session.open().await else {
        return;
    };
    if let Ok(output) = session.ready(&mut connection).await {
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
                    session.handle(&mut connection, output, request).await
                }
                SessionRequest::Use(database) => {
                    session.use_database(&mut connection, &database).await
                }
            };
            if handled.is_err() {
                break;
            }
        }
    }
    let _ = connection.disconnect().await;
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
    /// Dialects of the batch's queries when they are written in a dialect
    /// other than the native SQL of the server; `None` otherwise.
    pub(super) translation: Option<Translation>,
}

/// Summary of a result read to the end.
pub(super) struct Summary {
    pub(super) row_count: usize,
    pub(super) truncated: bool,
    pub(super) affected_rows: u64,
    pub(super) elapsed_ms: u128,
}

impl Session<'_> {
    /// Opens the session's connection; on failure, the error goes to the client.
    async fn open(&self) -> Option<Connection> {
        match mysql::connect(self.target).await {
            Ok(connection) => Some(connection),
            Err(error) => {
                let _ = self.outbox.error(None, error.to_string()).await;
                None
            }
        }
    }

    /// Announces the session to the client: identity of the server and current
    /// database. Returns the server's dialect, with its version and its SQL
    /// mode.
    async fn ready(&self, connection: &mut Connection) -> Result<OutputDialect, SessionStopped> {
        let info = match mysql::server_info(connection).await {
            Ok(info) => info,
            Err(error) => {
                self.outbox.error(None, error.to_string()).await?;
                return Err(SessionStopped);
            }
        };
        let database = match mysql::current_database(connection).await {
            Ok(database) => database,
            Err(error) => {
                self.outbox.error(None, error.to_string()).await?;
                return Err(SessionStopped);
            }
        };
        let output = output_of(&info);
        self.outbox
            .send(ServerMessage::Ready {
                server: info,
                database,
            })
            .await?;
        Ok(output)
    }

    /// Changes the session's current database and announces the new database
    /// to the client; a failure (unknown database, privileges) goes as a
    /// session error.
    async fn use_database(
        &self,
        connection: &mut Connection,
        database: &str,
    ) -> Result<(), SessionStopped> {
        let changed = match mysql::use_database(connection, database).await {
            Ok(()) => mysql::current_database(connection).await,
            Err(error) => Err(error),
        };
        match changed {
            Ok(database) => self.outbox.send(ServerMessage::Database { database }).await,
            Err(error) => self.outbox.error(None, error.to_string()).await,
        }
    }

    /// Handles a request of the client: splits the text, then runs the query
    /// under the cursor, or all the queries in parallel, transpiled from the
    /// dialect of the request to `output`, the server's.
    async fn handle(
        &self,
        connection: &mut Connection,
        output: OutputDialect,
        request: QueryRequest,
    ) -> Result<(), SessionStopped> {
        let translation = match request.dialect.as_deref().map(DialectChoice::parse) {
            None | Some(Ok(DialectChoice::Source)) => None,
            Some(Ok(DialectChoice::Chosen(input))) => Some(Translation { input, output }),
            Some(Err(message)) => return self.outbox.error(None, message).await,
        };
        let session = Session {
            translation,
            ..*self
        };
        let dialect = output.dialect;
        let max_rows = match row_limit(self.state, self.user_id, request.row_limit).await {
            Ok(max_rows) => max_rows as usize,
            Err(message) => return self.outbox.error(None, message).await,
        };
        let statements = split(&request.sql);
        let chosen = chosen_statements(&statements, request.cursor);
        if chosen.is_empty() {
            let message = "no query to run".to_string();
            return self.outbox.error(None, message).await;
        }

        let queries = (0_u32..)
            .zip(chosen.iter())
            .map(|(query, statement)| BatchQuery {
                query,
                tab: 0,
                sql: statement.sql.clone(),
                start: statement.start,
                end: statement.end,
            })
            .collect();
        self.outbox.send(ServerMessage::Batch { queries }).await?;

        if request.cursor.is_some() {
            session
                .run_on_session(connection, dialect, &chosen[0].sql, max_rows)
                .await?;
        } else {
            let database = match mysql::current_database(connection).await {
                Ok(database) => database,
                Err(error) => return self.outbox.error(None, error.to_string()).await,
            };
            session
                .run_in_parallel(&chosen, dialect, max_rows, &database)
                .await?;
        }
        self.outbox.send(ServerMessage::BatchDone).await
    }
}
