//! Messages of the query session, as JSON text: what the client sends, and
//! what the session answers. The types are exported to the interface by `ts-rs`.

use prisme_connectors::mysql::{Cell, ColumnTypeInfo, ServerInfo};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::recorder::ResumedQuery;

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
    /// Run queries, numbered by the relay when it received them (see
    /// `Cancellation::receive_batch`).
    Run {
        /// Number of the batch.
        batch: u64,
        /// Queries to run.
        request: QueryRequest,
    },
    /// Change the current database.
    Use(String),
}

/// Run request sent by the client.
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub(crate) struct QueryRequest {
    /// Whole text of the editor, one or more queries.
    pub(super) sql: String,
    /// Position of the cursor in UTF-16 units: only the query under the
    /// cursor runs. Absent: the queries of `selection`, or all of them, in
    /// parallel.
    #[ts(optional)]
    pub(super) cursor: Option<usize>,
    /// Passages selected in the text, without a cursor: the queries they
    /// touch run, in parallel. Absent or empty: all the queries.
    #[ts(optional)]
    pub(super) selection: Option<Vec<SelectionRange>>,
    /// Row limit for these queries only, among the allowed choices.
    #[ts(optional)]
    pub(super) row_limit: Option<u32>,
}

/// Passage of the editor's text selected by the user.
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub(crate) struct SelectionRange {
    /// Start of the passage, in UTF-16 units.
    pub(super) start: usize,
    /// End of the passage, in UTF-16 units, excluded.
    pub(super) end: usize,
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
