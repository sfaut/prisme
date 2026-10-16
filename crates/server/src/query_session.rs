//! Query session on a saved connection, over a WebSocket.
//!
//! A session keeps a connection open to the target server (transactions will
//! need it). It outlives its WebSocket: after a disconnection (page reloaded,
//! browser closed), it stays open for the delay set on the connection, and a
//! new WebSocket reattaches to it (`session_relay.rs`, `sessions.rs`). The
//! client sends the editor's whole text; the session splits it into queries
//! (`splitter` crate) and runs:
//!
//! - with a cursor (Ctrl+Enter): the query under the cursor, on the session's
//!   connection;
//! - without a cursor (Ctrl+Shift+Enter): all the queries in parallel, each on
//!   its own connection, opened on the session's current database. A query
//!   that changes the state of its connection (`USE`, `SET`) therefore has no
//!   effect on the others, nor on the session.
//!
//! Results arrive streamed, in batches of rows, never loaded whole in memory.
//! Each message of a result carries the number of its query (`query`): the
//! messages of parallel queries interleave.
//!
//! Protocol, as JSON text:
//!
//! ```text
//! server → client  {"type": "ready", "server": {"flavor": "mariadb", "version": "11.3.2-MariaDB", "ansi_quotes": false}, "database": "xlist"}
//! client → server  {"type": "run", "sql": "SELECT 1; SELECT 2", "cursor": 3, "row_limit": 2000}   cursor and row_limit optional
//! client → server  {"type": "cancel"}                                  stops all the running queries
//! client → server  {"type": "cancel_query", "query": 2}                stops a single query of the running batch
//! client → server  {"type": "use", "database": "xlist"}               changes the session's current database
//! server → client  {"type": "database", "database": "xlist"}          answer to use: the current database
//! server → client  {"type": "batch", "queries": [{"query": 0, "sql": "SELECT 1", "start": 0, "end": 9}]}
//! server → client  {"type": "started", "query": 0}                  the query starts (in parallel, it may wait for its turn)
//! server → client  {"type": "columns", "query": 0, "columns": ["1"], "types": [{"family": "integer", "name": "INT"}], "sql": "SELECT 1 LIMIT 1001", "limit": "added", "expected_rows": 1000, "execution_us": 830}
//! server → client  {"type": "rows", "query": 0, "rows": [["1"]]}       as many batches as needed
//! server → client  {"type": "done", "query": 0, "row_count": 1, "truncated": false, "affected_rows": 0, "elapsed_ms": 2, "database": "xlist"}
//! server → client  {"type": "error", "query": 0, "message": "..."}     instead of the rest, on failure
//! server → client  {"type": "batch_done"}                               all the queries have ended
//! server → client  {"type": "resume", "running": true, "queries": [...]}  on reattaching: the last batch, where it stands
//! ```
//!
//! A WebSocket that reattaches to a session receives `ready`, with the
//! current database, then `resume` for the session of an editor that has
//! already run a batch: each query of the last batch with its tab, its state
//! and the rows received so far; the next messages continue this batch.
//!
//! `cancel` interrupts the running queries (`KILL QUERY` on each of their
//! connections) and prevents those waiting for their turn from starting: each
//! one ends with a "query interrupted" error. `cancel_query` does the same for
//! a single query of the batch (the interface sends it when its tab is
//! closed). The session reads the client's messages while running, to receive
//! these stops in time.
//!
//! An error that concerns no query (invalid message, connection to the target
//! server impossible) has `"query": null`. `database` is the session's current
//! database (`SELECT DATABASE()`, `null` if none): that of the saved
//! connection at first, then the one chosen by `USE`.
//!
//! The row limit follows `docs/V1.md`, section 2.3: the request's
//! `row_limit`, otherwise the user's setting. Until the write switches exist,
//! writes and queries that cannot be parsed are refused (`docs/V1.md`,
//! section 2.4).

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use std::time::Duration;

use axum::extract::ws::WebSocket;
use futures_util::future::join_all;
use prisme_connectors::mysql::{self, Cell, ColumnTypeInfo, Connection, MysqlTarget, ServerInfo};
use prisme_dialects::Dialect;
use prisme_dialects::classify::{StatementKind, classify};
use prisme_dialects::limit::{LimitOutcome, apply_default_limit, written_limit};
use prisme_splitter::{Statement, split, statement_at};
use serde::{Deserialize, Serialize};
use tokio::sync::{Semaphore, mpsc, watch};
use ts_rs::TS;

use crate::api::AppState;
use crate::recorder::{Recorder, ResumedQuery};
use crate::session_relay::{Relay, SessionHandle};
use crate::settings::{self, check_row_limit};

/// Number of rows per `rows` message.
const ROW_BATCH_SIZE: usize = 500;

/// Maximum number of queries run at the same time (one connection each).
const MAX_PARALLEL: usize = 8;

/// Number of messages waiting to be sent beyond which reading the results waits.
const OUTBOX_SIZE: usize = 64;

/// Message sent by the client.
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum ClientMessage {
    /// Run queries.
    Run(QueryRequest),
    /// Stop all the running queries.
    Cancel,
    /// Stop a single query of the running batch, designated by its number.
    CancelQuery { query: u32 },
    /// Change the session's current database, like `USE`, without a result to show.
    Use { database: String },
}

/// Request passed to the session by the reading of the client's messages.
pub(crate) enum SessionRequest {
    /// Run queries.
    Run(QueryRequest),
    /// Change the current database.
    Use(String),
}

/// Error message of the queries stopped by the user.
const CANCELLED: &str = "query interrupted by the user";

/// Run request sent by the client.
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub(crate) struct QueryRequest {
    /// Whole text of the editor, one or more queries.
    sql: String,
    /// Position of the cursor in UTF-16 units: only the query under the
    /// cursor runs. Absent: all the queries, in parallel.
    #[ts(optional)]
    cursor: Option<usize>,
    /// Row limit for these queries only, among the allowed choices.
    #[ts(optional)]
    row_limit: Option<u32>,
}

/// Query of a batch, as announced to the client.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub(crate) struct BatchQuery {
    /// Number of the query in the batch, repeated by its messages.
    pub(crate) query: u32,
    /// Saved result tab of the query (session of an editor), 0 if the session
    /// does not save its results.
    pub(crate) tab: i64,
    /// Text of the query, without a separator.
    pub(crate) sql: String,
    /// Start of the query in the editor's text, in UTF-16 units.
    pub(crate) start: usize,
    /// End of the query in the editor's text, in UTF-16 units.
    pub(crate) end: usize,
}

/// Handling of the limit, as announced to the client.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub(crate) enum LimitReport {
    /// Prisme added `LIMIT N + 1`.
    Added,
    /// The limit written by the user applies.
    UserLimit,
    /// Query without rows to limit.
    NotApplicable,
    /// Query not parsed: reading stops by itself after N rows.
    Unparsed,
}

/// Message sent to the client.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum ServerMessage {
    /// The connection to the target server is open.
    Ready {
        server: ServerInfo,
        database: Option<String>,
    },
    /// Queries about to run.
    Batch { queries: Vec<BatchQuery> },
    /// A query of the batch starts. In parallel, the queries beyond
    /// `MAX_PARALLEL` wait for their turn before this message.
    Started { query: u32 },
    /// Start of a result: columns, text actually run, handling of the limit.
    Columns {
        query: u32,
        columns: Vec<String>,
        /// For each column, are its values right-aligned (numbers, dates and
        /// times)?
        right_aligned: Vec<bool>,
        /// Type of each column, announced by the server with the result.
        types: Vec<ColumnTypeInfo>,
        sql: String,
        limit: LimitReport,
        /// Maximum number of rows: Prisme's limit, or the one written by the
        /// user when it is a number; `null` if unknown. Used by the interface's
        /// progress bar.
        expected_rows: Option<u64>,
        /// Execution time of the query by the target server, in microseconds:
        /// from sending it to receiving its columns.
        execution_us: u64,
    },
    /// Batch of rows.
    Rows { query: u32, rows: Vec<Vec<Cell>> },
    /// End of a result.
    Done {
        query: u32,
        row_count: usize,
        truncated: bool,
        affected_rows: u64,
        elapsed_ms: u128,
        /// Current database of the session after the query (it changes with `USE`).
        database: Option<String>,
    },
    /// Failure of a query, or of the session if `query` is absent.
    Error { query: Option<u32>, message: String },
    /// All the queries of the batch have ended.
    BatchDone,
    /// Current database of the session, after a successful `use` request.
    Database { database: Option<String> },
    /// Last batch of the session, sent to a WebSocket that reattaches to it.
    Resume {
        /// Is the batch still running?
        running: bool,
        /// Queries of the batch, by number, where they stand.
        queries: Vec<ResumedQuery>,
    },
}

/// The message relay is gone: the session stops.
struct SessionStopped;

/// Queue of the messages to send to the client, shared by the parallel queries.
///
/// The relay (`session_relay.rs`) drains the queue to the attached WebSocket,
/// if there is one. The queue is bounded: if the client reads slowly, reading
/// the results waits.
#[derive(Clone)]
struct Outbox {
    /// To the relay.
    sender: mpsc::Sender<ServerMessage>,
}

impl Outbox {
    /// Puts a message in the queue.
    async fn send(&self, message: ServerMessage) -> Result<(), SessionStopped> {
        self.sender.send(message).await.map_err(|_| SessionStopped)
    }

    /// Puts an error message in the queue.
    async fn error(&self, query: Option<u32>, message: String) -> Result<(), SessionStopped> {
        self.send(ServerMessage::Error { query, message }).await
    }
}

/// Stopping of the running queries, requested by the client.
///
/// Keeps the list of the running queries with their connection, to be able
/// to interrupt them (`KILL QUERY`), a flag for stopping the whole batch and
/// the list of the queries stopped one by one. Queries check both before
/// starting and while reading their rows.
#[derive(Default)]
pub(crate) struct Cancellation {
    /// Has stopping the whole batch been requested?
    cancelled: AtomicBool,
    /// Numbers of the queries of the batch stopped one by one.
    cancelled_queries: Mutex<Vec<u32>>,
    /// Running queries: number of the query and id (`Conn::id()`) of its
    /// connection.
    running: Mutex<Vec<(u32, u32)>>,
}

impl Cancellation {
    /// Prepares a new batch: no stop requested anymore.
    fn reset(&self) {
        self.cancelled.store(false, Ordering::SeqCst);
        if let Ok(mut queries) = self.cancelled_queries.lock() {
            queries.clear();
        }
    }

    /// Has stopping the query `query` been requested, alone or with the whole batch?
    fn is_cancelled(&self, query: u32) -> bool {
        self.cancelled.load(Ordering::SeqCst)
            || self
                .cancelled_queries
                .lock()
                .is_ok_and(|queries| queries.contains(&query))
    }

    /// Requests stopping the whole batch and interrupts all the running queries.
    pub(crate) async fn cancel(&self, target: &MysqlTarget) {
        self.cancelled.store(true, Ordering::SeqCst);
        self.kill(target, |_| true).await;
    }

    /// Requests stopping a single query and interrupts it if it is running.
    pub(crate) async fn cancel_query(&self, target: &MysqlTarget, query: u32) {
        if let Ok(mut queries) = self.cancelled_queries.lock() {
            queries.push(query);
        }
        self.kill(target, |running| running == query).await;
    }

    /// Interrupts (`KILL QUERY`) the running queries that `selected` keeps.
    async fn kill(&self, target: &MysqlTarget, selected: impl Fn(u32) -> bool) {
        let connections: Vec<u32> = self
            .running
            .lock()
            .map(|running| {
                running
                    .iter()
                    .filter(|(query, _)| selected(*query))
                    .map(|(_, connection_id)| *connection_id)
                    .collect()
            })
            .unwrap_or_default();
        for connection_id in connections {
            if let Err(error) = mysql::kill_query(target, connection_id).await {
                tracing::warn!("cannot interrupt the query: {error}");
            }
        }
    }

    /// Records that the query `query` runs on a connection, until the returned
    /// guard is dropped.
    fn track(&self, query: u32, connection_id: u32) -> RunningGuard<'_> {
        if let Ok(mut running) = self.running.lock() {
            running.push((query, connection_id));
        }
        RunningGuard {
            cancellation: self,
            connection_id,
        }
    }
}

/// Guard of a running query: when it is dropped, the connection leaves the list.
struct RunningGuard<'a> {
    cancellation: &'a Cancellation,
    connection_id: u32,
}

impl Drop for RunningGuard<'_> {
    /// Removes the connection from the list of the running queries.
    fn drop(&mut self) {
        if let Ok(mut running) = self.cancellation.running.lock() {
            running.retain(|&(_, id)| id != self.connection_id);
        }
    }
}

/// Opens a session on `target`, attached to the WebSocket `socket`, and
/// returns what is needed to reattach another WebSocket to it or to stop it.
///
/// Two tasks share the session: one runs the client's requests (`serve`), the
/// other relays the messages to the current WebSocket and, for an editor's
/// session, saves them (`session_relay.rs`). Without an attached WebSocket for
/// `keep_alive`, the session stops; `on_end` is then called.
pub(crate) fn start(
    socket: WebSocket,
    state: AppState,
    user_id: i64,
    target: MysqlTarget,
    recorder: Option<Recorder>,
    keep_alive: Duration,
    on_end: Box<dyn FnOnce() + Send + Sync>,
) -> SessionHandle {
    let (sender, messages) = mpsc::channel(OUTBOX_SIZE);
    let (request_sender, requests) = mpsc::channel(1);
    let (stop_sender, stop) = watch::channel(false);
    let (commands, command_receiver) = mpsc::unbounded_channel();
    let cancellation = Arc::new(Cancellation::default());
    let relay = Relay {
        messages,
        commands: command_receiver,
        requests: request_sender,
        outbox: sender.downgrade(),
        cancellation: Arc::clone(&cancellation),
        target: target.clone(),
        recorder,
        stop: stop_sender,
        keep_alive,
        on_end: Some(on_end),
    };
    tokio::spawn(serve(
        Outbox { sender },
        requests,
        stop,
        state,
        user_id,
        target,
        cancellation,
    ));
    tokio::spawn(relay.run(socket));
    SessionHandle::new(commands)
}

/// Runs a session: connection to the target server, then the client's
/// requests until the session stops.
async fn serve(
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
fn chosen_statements(statements: &[Statement], cursor: Option<usize>) -> Vec<&Statement> {
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
fn write_refusal(sql: &str, dialect: Dialect) -> Option<String> {
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

#[cfg(test)]
mod tests {
    use futures_util::{SinkExt, StreamExt};
    use secrecy::SecretString;
    use serde_json::{Value, json};
    use testcontainers_modules::mariadb::Mariadb;
    use testcontainers_modules::testcontainers::runners::AsyncRunner;
    use tokio::net::TcpStream;
    use tokio_tungstenite::tungstenite::Message as ClientMessage;
    use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

    use crate::api::{AppState, router};
    use crate::connections::{self, NewConnection};
    use crate::editors::{self, NewEditor};
    use crate::master_key::MasterKey;
    use crate::store;
    use prisme_dialects::Dialect;

    type ClientSocket = WebSocketStream<MaybeTlsStream<TcpStream>>;

    /// Starts Prisme on a free port, with a saved connection to `host:port`
    /// (root account without a password, `test` database) and an editor on
    /// this connection, and returns the server's address, the id of the
    /// connection and that of the editor.
    async fn start(host: &str, port: u16) -> (String, i64, i64) {
        let db = store::open_in_memory();
        let key = MasterKey::generate();
        let user_id = store::ensure_user(&db, "test@example.com").unwrap();
        let new = NewConnection {
            name: "test".to_string(),
            host: host.to_string(),
            port,
            username: "root".to_string(),
            password: SecretString::from(""),
            database_name: "test".to_string(),
            // On-connect queries: the variable exists on each open connection,
            // and double quotes delimit identifiers there.
            init_sql: "SET @prisme_init = 1; SET SESSION sql_mode = 'ANSI_QUOTES'".to_string(),
            session_timeout_minutes: 5,
            show_system_schemas: true,
        };
        let id = connections::insert(&db, &key, user_id, &new).unwrap();
        let new_editor = NewEditor {
            connection_id: Some(id),
        };
        let editor = editors::create(&db, user_id, &new_editor).unwrap();

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let app = router(AppState::new(db, key, user_id));
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (address, id, editor.id)
    }

    /// Reads the next text message of the server, as JSON.
    async fn next_json(socket: &mut ClientSocket) -> Value {
        loop {
            if let ClientMessage::Text(text) = socket.next().await.unwrap().unwrap() {
                return serde_json::from_str(&text).unwrap();
            }
        }
    }

    /// Result of a query of a batch, as received by the client.
    #[derive(Debug, Default)]
    struct Received {
        /// `columns` message, or Null.
        columns: Value,
        /// Number of rows received.
        rows: usize,
        /// Last message of the query: `done` or `error`.
        end: Value,
        /// Has the `started` message arrived?
        started: bool,
    }

    /// Sends a request, then reads the messages until `batch_done`.
    /// Returns the `batch` message (or a session error) and the results, by query number.
    async fn run(socket: &mut ClientSocket, mut request: Value) -> (Value, Vec<Received>) {
        request["type"] = json!("run");
        socket
            .send(ClientMessage::Text(request.to_string().into()))
            .await
            .unwrap();
        let mut batch = Value::Null;
        let mut results: Vec<Received> = Vec::new();
        loop {
            let message = next_json(socket).await;
            let kind = message["type"].as_str().unwrap().to_string();
            if kind == "batch_done" {
                return (batch, results);
            }
            if kind == "error" && message["query"].is_null() {
                return (message, results);
            }
            if kind == "batch" {
                let count = message["queries"].as_array().unwrap().len();
                results = (0..count).map(|_| Received::default()).collect();
                batch = message;
                continue;
            }
            let result = &mut results[message["query"].as_u64().unwrap() as usize];
            match kind.as_str() {
                "started" => result.started = true,
                "columns" => result.columns = message,
                "rows" => result.rows += message["rows"].as_array().unwrap().len(),
                _ => result.end = message,
            }
        }
    }

    /// Runs the query under the cursor (here, the only text sent) and returns its result.
    async fn run_one(socket: &mut ClientSocket, sql: &str, row_limit: Option<u32>) -> Received {
        let mut request = json!({ "sql": sql, "cursor": 0 });
        if let Some(row_limit) = row_limit {
            request["row_limit"] = json!(row_limit);
        }
        run(socket, request).await.1.remove(0)
    }

    #[test]
    fn chooses_the_statement_under_the_cursor_or_all_of_them() {
        let statements = prisme_splitter::split("SELECT 1;\nSELECT 2;");
        let sql = |chosen: Vec<&prisme_splitter::Statement>| {
            chosen
                .into_iter()
                .map(|statement| statement.sql.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            sql(super::chosen_statements(&statements, Some(12))),
            ["SELECT 2"]
        );
        assert_eq!(
            sql(super::chosen_statements(&statements, None)),
            ["SELECT 1", "SELECT 2"]
        );
        assert!(super::chosen_statements(&[], Some(0)).is_empty());
    }

    #[tokio::test]
    async fn cancels_one_query_or_the_whole_batch() {
        // No running query: stopping does not contact the target server.
        let target = super::MysqlTarget {
            host: "127.0.0.1".to_string(),
            port: 1,
            username: "root".to_string(),
            password: secrecy::SecretString::from("secret"),
            database_name: String::new(),
            init_statements: Vec::new(),
        };
        let cancellation = super::Cancellation::default();
        cancellation.cancel_query(&target, 1).await;
        assert!(cancellation.is_cancelled(1));
        assert!(!cancellation.is_cancelled(0));

        cancellation.cancel(&target).await;
        assert!(cancellation.is_cancelled(0));

        // New batch: no stop anymore.
        cancellation.reset();
        assert!(!cancellation.is_cancelled(0));
        assert!(!cancellation.is_cancelled(1));
    }

    #[test]
    fn refuses_writes_and_unparsable_queries() {
        assert_eq!(super::write_refusal("SELECT 1", Dialect::MariaDb), None);
        assert_eq!(super::write_refusal("SHOW TABLES", Dialect::MariaDb), None);
        assert!(super::write_refusal("DELETE FROM t", Dialect::MariaDb).is_some());
        assert!(
            super::write_refusal("WITH x AS (SELECT 1) DELETE FROM t", Dialect::MariaDb).is_some()
        );
        assert!(
            super::write_refusal("SELECT * FROM t PROCEDURE ANALYSE()", Dialect::MariaDb).is_some()
        );
    }

    #[tokio::test]
    async fn unreachable_server_sends_an_error() {
        // Port 1: no server listens there.
        let (address, id, _) = start("127.0.0.1", 1).await;
        let (mut socket, _) = connect_async(format!("ws://{address}/api/connections/{id}/query"))
            .await
            .unwrap();

        assert_eq!(next_json(&mut socket).await["type"], "error");
    }

    #[tokio::test]
    async fn unknown_connection_is_refused() {
        let (address, _, _) = start("127.0.0.1", 1).await;
        assert!(
            connect_async(format!("ws://{address}/api/connections/999/query"))
                .await
                .is_err()
        );
    }

    #[tokio::test]
    #[ignore = "requires Docker Desktop running"]
    async fn streams_results_with_the_default_limit() {
        let server = Mariadb::default().start().await.unwrap();
        let host = server.get_host().await.unwrap().to_string();
        let port = server.get_host_port_ipv4(3306).await.unwrap();
        let (address, id, _) = start(&host, port).await;
        let (mut socket, _) = connect_async(format!("ws://{address}/api/connections/{id}/query"))
            .await
            .unwrap();

        let ready = next_json(&mut socket).await;
        assert_eq!(ready["type"], "ready");
        assert_eq!(ready["server"]["flavor"], "mariadb");
        assert_eq!(ready["server"]["ansi_quotes"], true);
        assert_eq!(ready["database"], "test");

        // No limit written: 1,000 rows shown out of 2,500, truncated result.
        let result = run_one(&mut socket, "SELECT seq FROM seq_1_to_2500", Some(1000)).await;
        assert_eq!(result.columns["limit"], "added");
        assert_eq!(
            result.columns["sql"],
            "SELECT seq FROM seq_1_to_2500 LIMIT 1001"
        );
        assert_eq!(result.rows, 1000);
        assert_eq!(result.end["truncated"], true);

        // Written limit: it applies as is, beyond the default limit.
        let result = run_one(
            &mut socket,
            "SELECT seq FROM seq_1_to_2500 LIMIT 1500",
            None,
        )
        .await;
        assert_eq!(result.columns["limit"], "user_limit");
        assert_eq!(result.rows, 1500);
        assert_eq!(result.end["truncated"], false);

        // An error does not interrupt the session.
        let result = run_one(&mut socket, "SELECT * FROM missing", None).await;
        assert_eq!(result.end["type"], "error");
        let result = run_one(&mut socket, "SELECT 1", None).await;
        assert_eq!(
            (result.rows, result.end["type"].as_str()),
            (1, Some("done"))
        );

        // USE changes the current database, announced at the end of the query.
        let result = run_one(&mut socket, "USE mysql", None).await;
        assert_eq!(result.end["database"], "mysql");

        // use changes the current database without a result; an unknown database gives a session error.
        for database in ["test", "missing", "mysql"] {
            let request = json!({ "type": "use", "database": database });
            socket
                .send(ClientMessage::Text(request.to_string().into()))
                .await
                .unwrap();
            let answer = next_json(&mut socket).await;
            if database == "missing" {
                assert_eq!(
                    (answer["type"].as_str(), answer["query"].is_null()),
                    (Some("error"), true)
                );
            } else {
                assert_eq!(answer, json!({ "type": "database", "database": database }));
            }
        }

        // Cursor on the empty line between two queries: the previous one runs.
        let text = "SELECT 'a';\n\nSELECT 'b', 'c';";
        let (batch, results) = run(&mut socket, json!({ "sql": text, "cursor": 12 })).await;
        assert_eq!(batch["queries"][0]["sql"], "SELECT 'a'");
        assert_eq!(results.len(), 1);

        // On-connect query run on the session's connection and on those of the
        // parallel queries.
        let check = "SELECT 1 FROM DUAL WHERE @prisme_init = 1";
        let (_, results) = run(&mut socket, json!({ "sql": check, "cursor": 0 })).await;
        assert_eq!(results[0].rows, 1);
        let request = json!({ "sql": format!("{check}; {check};") });
        let (_, results) = run(&mut socket, request).await;
        assert!(results.iter().all(|result| result.rows == 1));

        // Without a cursor: all the queries, in parallel, on the current database (mysql).
        let request = json!({ "sql": "SELECT 1; SELECT DATABASE(), 2;" });
        let (batch, results) = run(&mut socket, request).await;
        assert_eq!(batch["queries"].as_array().unwrap().len(), 2);
        assert_eq!(results[0].rows, 1);
        assert_eq!(results[1].columns["columns"], json!(["DATABASE()", "2"]));
        assert_eq!(results[1].columns["right_aligned"], json!([false, true]));
        let families: Vec<&Value> = results[1].columns["types"]
            .as_array()
            .unwrap()
            .iter()
            .map(|column| &column["family"])
            .collect();
        assert_eq!(families, [&json!("text"), &json!("integer")]);
        assert_eq!(results[1].end["database"], "mysql");
        assert!(results.iter().all(|result| result.started));

        // Stop: the running queries are interrupted well before their end.
        let started = std::time::Instant::now();
        let request = json!({ "type": "run", "sql": "SELECT SLEEP(30); SELECT SLEEP(30);" });
        socket
            .send(ClientMessage::Text(request.to_string().into()))
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        let cancel = json!({ "type": "cancel" }).to_string();
        socket
            .send(ClientMessage::Text(cancel.into()))
            .await
            .unwrap();
        let mut errors = 0;
        loop {
            let message = next_json(&mut socket).await;
            match message["type"].as_str().unwrap() {
                "error" => {
                    assert_eq!(message["message"], "query interrupted by the user");
                    errors += 1;
                }
                "batch_done" => break,
                _ => {}
            }
        }
        assert_eq!(errors, 2);
        assert!(started.elapsed() < std::time::Duration::from_secs(10));

        // Closing a tab: only its query is interrupted, the other one goes to the end.
        let started = std::time::Instant::now();
        let request = json!({ "type": "run", "sql": "SELECT SLEEP(30); SELECT SLEEP(1);" });
        socket
            .send(ClientMessage::Text(request.to_string().into()))
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        let cancel = json!({ "type": "cancel_query", "query": 0 }).to_string();
        socket
            .send(ClientMessage::Text(cancel.into()))
            .await
            .unwrap();
        let (mut interrupted, mut finished) = (Vec::new(), Vec::new());
        loop {
            let message = next_json(&mut socket).await;
            match message["type"].as_str().unwrap() {
                "error" => interrupted.push(message["query"].clone()),
                "done" => finished.push(message["query"].clone()),
                "batch_done" => break,
                _ => {}
            }
        }
        assert_eq!((interrupted, finished), (vec![json!(0)], vec![json!(1)]));
        assert!(started.elapsed() < std::time::Duration::from_secs(10));

        // Nothing to run: session error.
        let (error, _) = run(&mut socket, json!({ "sql": " -- nothing" })).await;
        assert_eq!(error["message"], "no query to run");
    }

    #[tokio::test]
    #[ignore = "requires Docker Desktop running"]
    async fn editor_session_survives_a_disconnect() {
        let server = Mariadb::default().start().await.unwrap();
        let host = server.get_host().await.unwrap().to_string();
        let port = server.get_host_port_ipv4(3306).await.unwrap();
        let (address, _, editor_id) = start(&host, port).await;
        let url = format!("ws://{address}/api/editors/{editor_id}/session");
        let (mut socket, _) = connect_async(&url).await.unwrap();
        assert_eq!(next_json(&mut socket).await["database"], "test");

        // The current database changes on the session's connection.
        let result = run_one(&mut socket, "USE mysql", None).await;
        assert_eq!(result.end["database"], "mysql");

        // A batch starts, then the page disappears before its end.
        let request = json!({ "type": "run", "sql": "SELECT SLEEP(2); SELECT 1;" });
        socket
            .send(ClientMessage::Text(request.to_string().into()))
            .await
            .unwrap();
        let batch = next_json(&mut socket).await;
        assert_eq!(batch["type"], "batch");
        drop(socket);
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;

        // The page comes back: same session (mysql database), running batch
        // resumed with the same tabs, then continued to the end.
        let (mut socket, _) = connect_async(&url).await.unwrap();
        let ready = next_json(&mut socket).await;
        assert_eq!(
            (ready["type"].as_str(), ready["database"].as_str()),
            (Some("ready"), Some("mysql"))
        );
        let resume = next_json(&mut socket).await;
        assert_eq!(resume["type"], "resume");
        assert_eq!(resume["running"], true);
        let tabs = |message: &Value, key: &str| -> Vec<Value> {
            message["queries"]
                .as_array()
                .unwrap()
                .iter()
                .map(|query| query[key].clone())
                .collect()
        };
        assert_eq!(tabs(&resume, "tab"), tabs(&batch, "tab"));
        let mut done: Vec<u64> = resume["queries"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|query| query["result"]["done"] == true)
            .map(|query| query["query"].as_u64().unwrap())
            .collect();
        loop {
            let message = next_json(&mut socket).await;
            match message["type"].as_str().unwrap() {
                "done" => done.push(message["query"].as_u64().unwrap()),
                "batch_done" => break,
                kind => assert_ne!(kind, "error", "{message}"),
            }
        }
        done.sort_unstable();
        assert_eq!(done, [0, 1]);
    }
}
