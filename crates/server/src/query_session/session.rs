//! Session on a target server: opens the connection, then runs the client's
//! requests (queries, database changes) until the session stops.

use std::sync::Arc;
use std::time::Instant;

use futures_util::future::join_all;
use prisme_connectors::mysql::{self, Connection, MysqlTarget};
use prisme_dialects::Dialect;
use prisme_dialects::classify::{StatementKind, classify};
use prisme_dialects::limit::{LimitOutcome, apply_default_limit, written_limit};
use prisme_splitter::{Statement, split, statement_at};
use tokio::sync::{Semaphore, mpsc, watch};

use super::cancellation::Cancellation;
use super::outbox::{Outbox, SessionStopped};
use super::protocol::{BatchQuery, LimitReport, QueryRequest, ServerMessage, SessionRequest};
use crate::api::AppState;
use crate::settings::{self, check_row_limit};

/// Number of rows per `rows` message.
const ROW_BATCH_SIZE: usize = 500;

/// Maximum number of queries run at the same time (one connection each).
const MAX_PARALLEL: usize = 8;

/// Error message of the queries stopped by the user.
const CANCELLED: &str = "query interrupted by the user";

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
    };
    let Some(mut connection) = session.open().await else {
        return;
    };
    if let Ok(dialect) = session.ready(&mut connection).await {
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
                SessionRequest::Run(request) => {
                    cancellation.reset();
                    session.handle(&mut connection, dialect, request).await
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
struct Session<'a> {
    /// Queue of the messages to the client.
    outbox: &'a Outbox,
    /// Connection settings, for the extra connections and the interruptions.
    target: &'a MysqlTarget,
    /// State of the application, to read the settings.
    state: &'a AppState,
    /// User of the session.
    user_id: i64,
    /// Stopping of the running queries, requested by the client.
    cancellation: &'a Cancellation,
}

/// Summary of a result read to the end.
struct Summary {
    row_count: usize,
    truncated: bool,
    affected_rows: u64,
    elapsed_ms: u128,
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
    /// database. Returns the server's dialect.
    async fn ready(&self, connection: &mut Connection) -> Result<Dialect, SessionStopped> {
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
    /// under the cursor, or all the queries in parallel.
    async fn handle(
        &self,
        connection: &mut Connection,
        dialect: Dialect,
        request: QueryRequest,
    ) -> Result<(), SessionStopped> {
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

        let queries = chosen
            .iter()
            .enumerate()
            .map(|(index, statement)| BatchQuery {
                query: index as u32,
                tab: 0,
                sql: statement.sql.clone(),
                start: statement.start,
                end: statement.end,
            })
            .collect();
        self.outbox.send(ServerMessage::Batch { queries }).await?;

        if request.cursor.is_some() {
            self.run_on_session(connection, dialect, &chosen[0].sql, max_rows)
                .await?;
        } else {
            let database = match mysql::current_database(connection).await {
                Ok(database) => database,
                Err(error) => return self.outbox.error(None, error.to_string()).await,
            };
            self.run_in_parallel(&chosen, dialect, max_rows, &database)
                .await?;
        }
        self.outbox.send(ServerMessage::BatchDone).await
    }

    /// Runs a query on the session's connection.
    async fn run_on_session(
        &self,
        connection: &mut Connection,
        dialect: Dialect,
        sql: &str,
        max_rows: usize,
    ) -> Result<(), SessionStopped> {
        self.outbox
            .send(ServerMessage::Started { query: 0 })
            .await?;
        let Some(summary) = self
            .run_query(connection, dialect, 0, sql, max_rows)
            .await?
        else {
            return Ok(());
        };
        let database = match mysql::current_database(connection).await {
            Ok(database) => database,
            Err(error) => return self.outbox.error(Some(0), error.to_string()).await,
        };
        self.done(0, summary, database).await
    }

    /// Runs all the queries in parallel, `MAX_PARALLEL` at most at a time, each
    /// on its own connection opened on the database `database`.
    async fn run_in_parallel(
        &self,
        statements: &[&Statement],
        dialect: Dialect,
        max_rows: usize,
        database: &Option<String>,
    ) -> Result<(), SessionStopped> {
        // All the queries start, but wait for a permit before opening their connection.
        let permits = Semaphore::new(MAX_PARALLEL);
        let mut runs = Vec::with_capacity(statements.len());
        for (index, statement) in statements.iter().enumerate() {
            let query = index as u32;
            runs.push(self.run_with_permit(
                &permits,
                dialect,
                query,
                &statement.sql,
                max_rows,
                database,
            ));
        }
        let results = join_all(runs).await;
        results.into_iter().collect()
    }

    /// Waits for a permit among `permits`, then runs the query on a new connection.
    async fn run_with_permit(
        &self,
        permits: &Semaphore,
        dialect: Dialect,
        query: u32,
        sql: &str,
        max_rows: usize,
        database: &Option<String>,
    ) -> Result<(), SessionStopped> {
        let _permit = permits.acquire().await.map_err(|_| SessionStopped)?;
        if self.cancellation.is_cancelled(query) {
            return self.outbox.error(Some(query), CANCELLED.to_string()).await;
        }
        self.outbox.send(ServerMessage::Started { query }).await?;
        self.run_on_new_connection(dialect, query, sql, max_rows, database)
            .await
    }

    /// Runs a query on a new connection, placed on the database `database`.
    async fn run_on_new_connection(
        &self,
        dialect: Dialect,
        query: u32,
        sql: &str,
        max_rows: usize,
        database: &Option<String>,
    ) -> Result<(), SessionStopped> {
        let mut connection = match mysql::connect(self.target).await {
            Ok(connection) => connection,
            Err(error) => return self.outbox.error(Some(query), error.to_string()).await,
        };
        let result = self
            .run_on_connection(&mut connection, dialect, query, sql, max_rows, database)
            .await;
        let _ = connection.disconnect().await;
        result
    }

    /// Places the connection on the database `database`, then runs the query on it.
    async fn run_on_connection(
        &self,
        connection: &mut Connection,
        dialect: Dialect,
        query: u32,
        sql: &str,
        max_rows: usize,
        database: &Option<String>,
    ) -> Result<(), SessionStopped> {
        if let Some(name) = database
            && let Err(error) = mysql::use_database(connection, name).await
        {
            return self.outbox.error(Some(query), error.to_string()).await;
        }
        match self
            .run_query(connection, dialect, query, sql, max_rows)
            .await?
        {
            Some(summary) => self.done(query, summary, database.clone()).await,
            None => Ok(()),
        }
    }

    /// Runs a query and streams its columns and its rows.
    ///
    /// Returns the summary of the result, or `None` if the query failed (the
    /// error has then already gone to the client).
    async fn run_query(
        &self,
        connection: &mut Connection,
        dialect: Dialect,
        query: u32,
        sql: &str,
        max_rows: usize,
    ) -> Result<Option<Summary>, SessionStopped> {
        if let Some(refusal) = write_refusal(sql, dialect) {
            self.outbox.error(Some(query), refusal).await?;
            return Ok(None);
        }
        let limited = match apply_default_limit(sql, dialect, max_rows as u64) {
            Ok(limited) => limited,
            Err(error) => {
                self.outbox.error(Some(query), error.to_string()).await?;
                return Ok(None);
            }
        };
        // Reading only stops by itself if Prisme set the limit.
        let capped = matches!(
            limited.outcome,
            LimitOutcome::Added | LimitOutcome::Unparsed
        );

        let started = Instant::now();
        let connection_id = connection.id();
        let _running = self.cancellation.track(query, connection_id);
        // Stop requested while the connection was opening: the query does not start.
        if self.cancellation.is_cancelled(query) {
            self.outbox
                .error(Some(query), CANCELLED.to_string())
                .await?;
            return Ok(None);
        }
        let executing = Instant::now();
        let mut stream = match mysql::execute(connection, &limited.sql).await {
            Ok(stream) => stream,
            Err(error) => {
                self.outbox
                    .error(Some(query), self.failure(query, error))
                    .await?;
                return Ok(None);
            }
        };
        self.outbox
            .send(ServerMessage::Columns {
                query,
                columns: stream.column_names(),
                right_aligned: stream.right_aligned_columns(),
                types: stream.column_types(),
                sql: limited.sql.clone(),
                limit: report(limited.outcome),
                expected_rows: match limited.outcome {
                    LimitOutcome::Added | LimitOutcome::Unparsed => Some(max_rows as u64),
                    LimitOutcome::UserLimit => written_limit(sql, dialect),
                    LimitOutcome::NotApplicable => None,
                },
                execution_us: u64::try_from(executing.elapsed().as_micros()).unwrap_or(u64::MAX),
            })
            .await?;

        let mut batch = Vec::with_capacity(ROW_BATCH_SIZE);
        let mut row_count = 0;
        let mut truncated = false;
        loop {
            let row = match stream.next_row().await {
                Ok(Some(row)) => row,
                Ok(None) => break,
                Err(error) => {
                    stream.discard().await;
                    self.outbox
                        .error(Some(query), self.failure(query, error))
                        .await?;
                    return Ok(None);
                }
            };
            if self.cancellation.is_cancelled(query) {
                // The server has already received KILL QUERY: reading stops.
                stream.discard().await;
                self.outbox
                    .error(Some(query), CANCELLED.to_string())
                    .await?;
                return Ok(None);
            }
            if capped && row_count == max_rows {
                // Row N + 1: the result is truncated, no need to read further.
                truncated = true;
                if limited.outcome == LimitOutcome::Unparsed {
                    // Without LIMIT, the server would keep sending rows: it is interrupted.
                    if let Err(error) = mysql::kill_query(self.target, connection_id).await {
                        tracing::warn!("cannot interrupt the query: {error}");
                    }
                }
                break;
            }
            batch.push(row);
            row_count += 1;
            if batch.len() == ROW_BATCH_SIZE {
                let rows = std::mem::replace(&mut batch, Vec::with_capacity(ROW_BATCH_SIZE));
                self.outbox
                    .send(ServerMessage::Rows { query, rows })
                    .await?;
            }
        }
        if !batch.is_empty() {
            self.outbox
                .send(ServerMessage::Rows { query, rows: batch })
                .await?;
        }

        let affected_rows = stream.affected_rows();
        stream.discard().await;
        Ok(Some(Summary {
            row_count,
            truncated,
            affected_rows,
            elapsed_ms: started.elapsed().as_millis(),
        }))
    }

    /// Message of a failed query: "interrupted" if the stop was requested (the
    /// server's error is then that of `KILL QUERY`).
    fn failure(&self, query: u32, error: mysql::MysqlError) -> String {
        if self.cancellation.is_cancelled(query) {
            CANCELLED.to_string()
        } else {
            error.to_string()
        }
    }

    /// Announces the end of a result.
    async fn done(
        &self,
        query: u32,
        summary: Summary,
        database: Option<String>,
    ) -> Result<(), SessionStopped> {
        self.outbox
            .send(ServerMessage::Done {
                query,
                row_count: summary.row_count,
                truncated: summary.truncated,
                affected_rows: summary.affected_rows,
                elapsed_ms: summary.elapsed_ms,
                database,
            })
            .await
    }
}

/// Queries to run: the one under the cursor, or all of them without a cursor.
pub(super) fn chosen_statements(
    statements: &[Statement],
    cursor: Option<usize>,
) -> Vec<&Statement> {
    match cursor {
        Some(cursor) => statement_at(statements, cursor)
            .map(|index| vec![&statements[index]])
            .unwrap_or_default(),
        None => statements.iter().collect(),
    }
}

/// Row limit of a request: the requested one, otherwise the user's setting.
async fn row_limit(state: &AppState, user_id: i64, requested: Option<u32>) -> Result<u32, String> {
    if let Some(row_limit) = requested {
        return check_row_limit(row_limit).map_err(|error| error.to_string());
    }
    state
        .with_db(move |db, _key| settings::load(db, user_id))
        .await
        .map(|settings| settings.row_limit)
        .map_err(|_| "unreadable user settings, see the server log".to_string())
}

/// Returns the reason for the refusal if the query is not a read.
///
/// Writes stay refused as long as the write switches (stage 4 of the v1) do
/// not exist. A query that cannot be parsed is refused too, as nothing proves
/// it is a read.
pub(super) fn write_refusal(sql: &str, dialect: Dialect) -> Option<String> {
    match classify(sql, dialect) {
        StatementKind::Read => None,
        StatementKind::Write => {
            Some("write refused: writes are disabled until the write switches exist".to_string())
        }
        StatementKind::Unparsed => Some(
            "query refused: Prisme cannot parse it, so it cannot check that it is a read"
                .to_string(),
        ),
    }
}

/// Translates the handling of the limit for the client.
fn report(outcome: LimitOutcome) -> LimitReport {
    match outcome {
        LimitOutcome::Added => LimitReport::Added,
        LimitOutcome::UserLimit => LimitReport::UserLimit,
        LimitOutcome::NotApplicable => LimitReport::NotApplicable,
        LimitOutcome::Unparsed => LimitReport::Unparsed,
    }
}
