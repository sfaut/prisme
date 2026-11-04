//! Recorder of an editor's results, plugged into the relay of the messages of
//! its query session (`session_relay.rs`).
//!
//! Each message sent to the client first goes through the recorder:
//!
//! - `batch`: the editor's unpinned tabs disappear, one tab per query is
//!   created; its id goes to the client with the query. A refresh keeps
//!   the tabs: its query runs again in its own tab, as its next run;
//! - `started`, `columns`: the state of each query is noted in memory;
//! - `rows`: the rows are written to the internal database as they arrive,
//!   in chunks of about `CHUNK_ROWS` rows: only the chunk being filled and
//!   the one being written stay in memory, whatever the size of the result.
//!   A chunk is written in a task of its own, while the next one fills: the
//!   messages go on to the client during the write, which waits only if the
//!   previous chunk is still being written;
//! - `done`, `error`: the last chunk, then the state of the result, are
//!   written. A refresh writes the chunks of its run beside the saved result,
//!   which they replace only once it succeeds: on failure, they are deleted
//!   and the saved result stays as it was.
//!
//! The browser therefore never sends the results back: after a reload, it
//! reads them again as the server received them, chunk by chunk. At the end
//! of the session, an unfinished query is saved as failed.
//!
//! The state of the last batch stays in memory until the next one, finished
//! queries included: a WebSocket that reattaches to the session receives it
//! as is (`snapshot`), then the rows received so far, read again from their
//! chunks, even if a query ended between the page reading the saved tabs and
//! reattaching.

use std::collections::HashMap;
use std::time::Instant;

use prisme_connectors::mysql::Cell;
use serde::Serialize;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use ts_rs::TS;

use crate::api::AppState;
use crate::api_error::ApiError;
use crate::editors::{self, SavedColumns, SavedResult};
use crate::error_detail::ErrorDetail;
use crate::query_session::ServerMessage;
use crate::query_session::protocol::BatchQuery;

/// Rows of a chunk: the rows received are written once they reach this
/// number, at the end of the `rows` message that reaches it. About 1 MB of
/// JSON for 10 short columns: far below the largest value SQLite holds
/// (1,000,000,000 bytes) and the longest string of the page (536,870,888
/// characters), and few writes, 100 per million rows.
const CHUNK_ROWS: usize = 10_000;

/// Size of a chunk, in bytes of the `rows` messages of its rows: wide rows
/// are written before they reach `CHUNK_ROWS`.
const CHUNK_BYTES: u64 = 8_000_000;

/// Query of the last batch, as sent to a WebSocket that reattaches.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub(crate) struct ResumedQuery {
    /// Number of the query in the batch, repeated by its next messages.
    query: u32,
    /// Saved result tab of the query.
    tab: i64,
    /// Text of the query, without a separator.
    sql: String,
    /// State of the result.
    result: SavedResult,
    /// Number of rows received so far, sent after `resume` by `resumed_rows`
    /// messages; 0 for a refresh that failed, whose tab keeps its saved
    /// result.
    resumed_rows: usize,
    /// Time elapsed since the query started, in microseconds; absent while
    /// it waits for its turn.
    started_us_ago: Option<u64>,
    /// Time elapsed since its columns were received, in microseconds; absent
    /// before.
    columns_us_ago: Option<u64>,
    /// Does the query refresh its tab? The saved result of the tab then
    /// stays until the query ends successfully.
    refresh: bool,
}

/// Rows a query of the last batch has received so far, where they are: in
/// the chunks of its run, then in memory.
pub(crate) struct ReceivedRows {
    /// Number of the query in the batch.
    pub(crate) query: u32,
    /// Result tab of the query.
    pub(crate) tab_id: i64,
    /// Run of the tab whose chunks hold its rows.
    pub(crate) run: i64,
    /// Number of chunks written, from position 0.
    pub(crate) chunks: u32,
    /// Rows received since the last chunk.
    pub(crate) pending: Vec<Vec<Cell>>,
}

/// Last batch, as a WebSocket that reattaches receives it.
pub(crate) struct Snapshot {
    /// Queries of the batch, by number, without their rows.
    pub(crate) queries: Vec<ResumedQuery>,
    /// Rows received by the queries that have some, by number.
    pub(crate) rows: Vec<ReceivedRows>,
}

/// Recorder of an editor's results.
pub struct Recorder {
    /// State of the application, for the internal database.
    state: AppState,
    /// Editor whose results are recorded.
    editor_id: i64,
    /// Queries of the last batch, by number.
    records: Mutex<HashMap<u32, Record>>,
}

/// Result of a query being recorded.
struct Record {
    /// Result tab of the query.
    tab_id: i64,
    /// Text of the query.
    sql: String,
    /// State of the result.
    result: SavedResult,
    /// Run of the tab whose chunks the query writes: 0 for a new tab, the
    /// one after the saved run for a refresh.
    run: i64,
    /// Number of chunks written.
    chunks: u32,
    /// Number of rows of the chunks written.
    written_rows: usize,
    /// Rows received since the chunk being written, or since the last chunk
    /// written.
    pending: Vec<Vec<Cell>>,
    /// Size of the `rows` messages of `pending`, in bytes.
    pending_bytes: u64,
    /// Chunk being written, the one after the `chunks` written.
    writing: Option<Writing>,
    /// Has a chunk failed to be written? The failure is logged once; its rows
    /// go back to the pending ones, and go with the next chunk.
    write_failed: bool,
    /// Instant the query started.
    started_at: Option<Instant>,
    /// Instant the columns were received, for the transfer time.
    columns_at: Option<Instant>,
    /// Does the query refresh its tab? Only a complete result is then saved.
    refresh: bool,
}

impl Recorder {
    /// Creates the recorder of the results of editor `editor_id`.
    pub fn new(state: AppState, editor_id: i64) -> Self {
        Self {
            state,
            editor_id,
            records: Mutex::new(HashMap::new()),
        }
    }

    /// Takes note of a message before it is sent to the client. For `batch`,
    /// completes each query with the id of its tab.
    pub async fn observe(&self, message: &mut ServerMessage) {
        let mut records = self.records.lock().await;
        let bytes = serde_json::to_vec(message).map_or(0, |json| json.len() as u64);
        match message {
            ServerMessage::Batch { queries } => {
                records.clear();
                match queries.as_mut_slice() {
                    [query] if query.tab > 0 => self.refresh(&mut records, query).await,
                    _ => self.create_tabs(&mut records, queries).await,
                }
            }
            ServerMessage::Started { query } => {
                if let Some(record) = records.get_mut(query) {
                    record.result.started = true;
                    record.started_at = Some(Instant::now());
                }
            }
            ServerMessage::Columns {
                query,
                columns,
                right_aligned,
                types,
                sql,
                limit,
                expected_rows,
                execution_us,
                executed_at,
            } => {
                if let Some(record) = records.get_mut(query) {
                    record.result.columns = Some(SavedColumns {
                        columns: columns.clone(),
                        right_aligned: right_aligned.clone(),
                        types: types.clone(),
                        sql: sql.clone(),
                        limit: limit.clone(),
                        expected_rows: *expected_rows,
                        execution_us: *execution_us,
                        executed_at: *executed_at,
                    });
                    record.result.bytes += bytes;
                    record.columns_at = Some(Instant::now());
                }
            }
            ServerMessage::Rows { query, rows } => {
                if let Some(record) = records.get_mut(query) {
                    record.pending.extend(rows.iter().cloned());
                    record.pending_bytes += bytes;
                    record.result.row_count += rows.len();
                    record.result.bytes += bytes;
                    if record.pending.len() >= CHUNK_ROWS || record.pending_bytes >= CHUNK_BYTES {
                        self.write_chunk(record).await;
                    }
                }
            }
            ServerMessage::Done {
                query,
                truncated,
                affected_rows,
                ..
            } => {
                if let Some(record) = records.get_mut(query) {
                    record.result.done = true;
                    record.result.truncated = *truncated;
                    record.result.affected_rows = *affected_rows;
                    record.result.bytes += bytes;
                    // Saturates: 2^64 microseconds is about 584,000 years.
                    record.result.transfer_us = record.columns_at.map_or(0, |at| {
                        u64::try_from(at.elapsed().as_micros()).unwrap_or(u64::MAX)
                    });
                    self.flush(record).await;
                    self.save(record).await;
                }
            }
            ServerMessage::Error {
                query: Some(query),
                message,
                detail,
            } => {
                if let Some(record) = records.get_mut(query) {
                    record.result.done = true;
                    record.result.error = Some(message.clone());
                    record.result.error_detail = Some(detail.clone());
                    self.end_failed(record).await;
                }
            }
            _ => {}
        }
    }

    /// Last batch, query by query, where it stands, with where the rows
    /// received so far are.
    pub(crate) async fn snapshot(&self) -> Snapshot {
        let mut records = self.records.lock().await;
        // The chunks being written first: the rows announced are then in the
        // chunks written or in memory.
        for record in records.values_mut() {
            settle(record).await;
        }
        let mut queries: Vec<ResumedQuery> = records
            .iter()
            .map(|(&query, record)| ResumedQuery {
                query,
                tab: record.tab_id,
                sql: record.sql.clone(),
                result: record.result.clone(),
                resumed_rows: record.written_rows + record.pending.len(),
                started_us_ago: record.started_at.map(micros_since),
                columns_us_ago: record.columns_at.map(micros_since),
                refresh: record.refresh,
            })
            .collect();
        queries.sort_by_key(|query| query.query);
        let mut rows: Vec<ReceivedRows> = records
            .iter()
            .filter(|(_, record)| record.chunks > 0 || !record.pending.is_empty())
            .map(|(&query, record)| ReceivedRows {
                query,
                tab_id: record.tab_id,
                run: record.run,
                chunks: record.chunks,
                pending: record.pending.clone(),
            })
            .collect();
        rows.sort_by_key(|received| received.query);
        Snapshot { queries, rows }
    }

    /// Chunk `position` of run `run` of the result tab `tab_id`, as saved:
    /// the JSON text of its rows. `None` if it does not exist.
    pub(crate) async fn chunk(
        &self,
        tab_id: i64,
        run: i64,
        position: u32,
    ) -> Result<Option<String>, ApiError> {
        self.state
            .with_db(move |db, _| editors::chunk(db, tab_id, run, position))
            .await
    }

    /// End of the session: unfinished queries are saved as failed, except a
    /// refresh, whose tab keeps its saved result.
    pub async fn finish(&self) {
        let mut records = self.records.lock().await;
        for record in records.values_mut().filter(|record| !record.result.done) {
            record.result.done = true;
            let detail = ErrorDetail::SessionClosed;
            record.result.error = Some(detail.to_string());
            record.result.error_detail = Some(detail);
            self.end_failed(record).await;
        }
        records.clear();
    }

    /// New batch: the editor's unpinned tabs disappear, and each query gets a
    /// new tab, whose id and configuration of the Pivoteur it takes.
    async fn create_tabs(&self, records: &mut HashMap<u32, Record>, queries: &mut [BatchQuery]) {
        let editor_id = self.editor_id;
        let wanted: Vec<_> = queries.iter().map(|q| (q.query, q.sql.clone())).collect();
        let created = self
            .state
            .with_db(move |db, _| editors::start_batch(db, editor_id, &wanted))
            .await;
        match created {
            Ok(tabs) => {
                for (query, (tab_id, pivot)) in queries.iter_mut().zip(tabs) {
                    query.tab = tab_id;
                    query.pivot = Some(pivot);
                    records.insert(query.query, Record::new(tab_id, &query.sql, 0, false));
                }
            }
            Err(_) => tracing::warn!("result tabs not created, editor {editor_id}"),
        }
    }

    /// Refresh of a tab: the query runs again in it, as its next run, the
    /// other tabs stay. A tab that is not the editor's (closed in the
    /// meantime, or another editor's) loses its id: the result is then not
    /// recorded.
    async fn refresh(&self, records: &mut HashMap<u32, Record>, query: &mut BatchQuery) {
        let editor_id = self.editor_id;
        let tab_id = query.tab;
        let started = self
            .state
            .with_db(move |db, _| editors::start_refresh(db, editor_id, tab_id))
            .await;
        if let Ok(Some(run)) = started {
            records.insert(query.query, Record::new(tab_id, &query.sql, run, true));
        } else {
            query.tab = 0;
        }
    }

    /// Starts the write of the pending rows as the next chunk of the query,
    /// once the chunk being written, if any, is written: one write at a time,
    /// in the order of the rows. Without effect if no rows are pending.
    async fn write_chunk(&self, record: &mut Record) {
        settle(record).await;
        if record.pending.is_empty() {
            return;
        }
        let rows = std::mem::take(&mut record.pending);
        let bytes = std::mem::take(&mut record.pending_bytes);
        let (tab_id, run, position) = (record.tab_id, record.run, record.chunks);
        let task = tokio::spawn(write(self.state.clone(), tab_id, run, position, rows));
        record.writing = Some(Writing { bytes, task });
    }

    /// Writes the pending rows as the last chunk of the query, and waits
    /// until every chunk is written, or failed.
    async fn flush(&self, record: &mut Record) {
        self.write_chunk(record).await;
        settle(record).await;
    }

    /// End of a query that failed or that the end of the session interrupted:
    /// its rows received so far are saved with its state; a refresh deletes
    /// the chunks of its run, and its tab keeps its saved result.
    async fn end_failed(&self, record: &mut Record) {
        if record.refresh {
            // A chunk still being written would land after the deletion.
            settle(record).await;
            let (tab_id, run) = (record.tab_id, record.run);
            let discarded = self
                .state
                .with_db(move |db, _| editors::discard_run(db, tab_id, run))
                .await;
            if discarded.is_err() {
                tracing::warn!("rows of a failed refresh not deleted, tab {tab_id}");
            }
            record.chunks = 0;
            record.written_rows = 0;
            record.pending.clear();
            record.pending_bytes = 0;
        } else {
            self.flush(record).await;
            self.save(record).await;
        }
    }

    /// Writes the state of the result of a query to the internal database,
    /// its rows being already in their chunks; for a refresh, that succeeded,
    /// its run then replaces the saved one. A failure is logged, without
    /// interrupting the session.
    async fn save(&self, record: &Record) {
        let (tab_id, run, refresh) = (record.tab_id, record.run, record.refresh);
        let result = record.result.clone();
        let saved = self
            .state
            .with_db(move |db, _| {
                if refresh {
                    editors::finish_refresh(db, tab_id, run, &result)
                } else {
                    editors::save_result(db, tab_id, &result)
                }
            })
            .await;
        if saved.is_err() {
            tracing::warn!("result not saved, tab {tab_id}");
        }
    }
}

/// Waits for the chunk of `record` being written, if any: written, it
/// counts among its chunks; failed, its rows go back before the pending ones,
/// to go with the next chunk, at the same position.
async fn settle(record: &mut Record) {
    let Some(writing) = record.writing.take() else {
        return;
    };
    let failed = match writing.task.await {
        Ok(Ok(count)) => {
            record.chunks += 1;
            record.written_rows += count;
            return;
        }
        Ok(Err(mut rows)) => {
            rows.append(&mut record.pending);
            record.pending = rows;
            record.pending_bytes += writing.bytes;
            "rows not saved"
        }
        // The task panicked: its rows are lost, the next chunks follow.
        Err(_) => "rows lost",
    };
    if !record.write_failed {
        record.write_failed = true;
        tracing::warn!("{failed}, tab {}", record.tab_id);
    }
}

impl Record {
    /// Empty result of tab `tab_id`, for the query `sql`, which writes the
    /// chunks of run `run` and `refresh`es the tab or not.
    fn new(tab_id: i64, sql: &str, run: i64, refresh: bool) -> Self {
        Self {
            tab_id,
            sql: sql.to_string(),
            result: SavedResult::default(),
            run,
            chunks: 0,
            written_rows: 0,
            pending: Vec::new(),
            pending_bytes: 0,
            writing: None,
            write_failed: false,
            started_at: None,
            columns_at: None,
            refresh,
        }
    }
}

/// Chunk being written, in a task of its own.
struct Writing {
    /// Size of the `rows` messages of its rows, in bytes.
    bytes: u64,
    /// Write of the chunk: the number of its rows once written, its rows if
    /// the write failed.
    task: JoinHandle<Result<usize, Vec<Vec<Cell>>>>,
}

/// Writes `rows` as chunk `position` of run `run` of the result tab `tab_id`,
/// and returns their number, or the rows themselves if the write failed.
async fn write(
    state: AppState,
    tab_id: i64,
    run: i64,
    position: u32,
    rows: Vec<Vec<Cell>>,
) -> Result<usize, Vec<Vec<Cell>>> {
    let text = match serde_json::to_string(&rows) {
        Ok(text) => text,
        Err(error) => {
            tracing::error!("rows not serialised, tab {tab_id}: {error}");
            return Err(rows);
        }
    };
    let written = state
        .with_db(move |db, _| editors::save_chunk(db, tab_id, run, position, &text))
        .await;
    match written {
        Ok(()) => Ok(rows.len()),
        Err(_) => Err(rows),
    }
}

/// Time elapsed since `instant`, in microseconds.
fn micros_since(instant: Instant) -> u64 {
    u64::try_from(instant.elapsed().as_micros()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editors::NewEditor;
    use crate::exports;
    use crate::master_key::MasterKey;
    use crate::query_session::LimitReport;
    use crate::store;

    /// Rows of the chunks of the saved result of each tab of an editor, in
    /// display order, read as the page reads them.
    async fn saved_rows(
        state: &AppState,
        user_id: i64,
        editor_id: i64,
    ) -> Vec<(i64, Vec<Vec<Cell>>)> {
        state
            .with_db(move |db, _| {
                let tabs = editors::results_of(db, user_id, editor_id)?.unwrap();
                let mut saved = Vec::new();
                for tab in tabs {
                    let mut rows = Vec::new();
                    for position in 0..tab.chunks {
                        let text = editors::saved_chunk(db, user_id, tab.id, position)?.unwrap();
                        rows.extend(serde_json::from_str::<Vec<Vec<Cell>>>(&text)?);
                    }
                    saved.push((tab.id, rows));
                }
                Ok::<_, editors::EditorsError>(saved)
            })
            .await
            .unwrap()
    }

    /// Columns message of query 0, of a single column.
    fn columns() -> ServerMessage {
        ServerMessage::Columns {
            query: 0,
            columns: vec!["x".to_string()],
            right_aligned: vec![true],
            types: Vec::new(),
            sql: "SELECT 1".to_string(),
            limit: LimitReport::NotApplicable,
            expected_rows: None,
            execution_us: 0,
            executed_at: 0,
        }
    }

    /// Rows message of query 0, of the rows `from..to` of a single column.
    fn rows(from: usize, to: usize) -> ServerMessage {
        ServerMessage::Rows {
            query: 0,
            rows: (from..to).map(|row| vec![Some(row.to_string())]).collect(),
        }
    }

    /// Done message of query 0.
    fn done() -> ServerMessage {
        ServerMessage::Done {
            query: 0,
            row_count: 1,
            truncated: false,
            affected_rows: 0,
            elapsed_ms: 1,
            database: None,
        }
    }

    #[tokio::test]
    async fn records_each_result_and_closes_unfinished_ones() {
        let db = store::open_in_memory();
        let user_id = store::ensure_user(&db, "test@example.com").unwrap();
        let editor = editors::create(&db, user_id, &NewEditor::default()).unwrap();
        let state = AppState::new(db, MasterKey::generate(), user_id, exports::unused_folder());
        let recorder = Recorder::new(state.clone(), editor.id);

        let mut batch = ServerMessage::Batch {
            queries: (0..2)
                .map(|query| BatchQuery {
                    query,
                    tab: 0,
                    sql: format!("SELECT {query}"),
                    start: 0,
                    end: 8,
                    pivot: None,
                })
                .collect(),
        };
        recorder.observe(&mut batch).await;
        let ServerMessage::Batch { queries } = &batch else {
            unreachable!()
        };
        assert!(queries.iter().all(|query| query.tab > 0));
        assert!(queries.iter().all(|query| query.pivot.is_some()));

        for mut message in [
            ServerMessage::Started { query: 0 },
            ServerMessage::Columns {
                query: 0,
                columns: vec!["x".to_string()],
                right_aligned: vec![true],
                types: Vec::new(),
                sql: "SELECT 0".to_string(),
                limit: LimitReport::NotApplicable,
                expected_rows: None,
                execution_us: 120,
                executed_at: 1_792_500_000_000,
            },
            rows(0, 1),
            done(),
            ServerMessage::Started { query: 1 },
        ] {
            recorder.observe(&mut message).await;
        }
        let snapshot = recorder.snapshot().await;
        assert_eq!(snapshot.queries.len(), 2);
        assert!(snapshot.queries[0].result.done && snapshot.queries[0].resumed_rows == 1);
        assert!(!snapshot.queries[1].result.done && snapshot.queries[1].started_us_ago.is_some());
        assert!(snapshot.queries[1].columns_us_ago.is_none());
        // The rows of the ended query are in its single chunk.
        let [received] = snapshot.rows.as_slice() else {
            panic!("rows of a single query expected")
        };
        assert_eq!((received.query, received.chunks), (0, 1));
        assert!(received.pending.is_empty());
        recorder.finish().await;
        assert!(recorder.snapshot().await.queries.is_empty());

        let editor_id = editor.id;
        let tabs = state
            .with_db(move |db, _| editors::results_of(db, user_id, editor_id))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            saved_rows(&state, user_id, editor_id).await[0].1,
            [vec![Some("0".to_string())]]
        );
        assert!(tabs[0].result.done && tabs[0].result.error.is_none());
        let columns = tabs[0].result.columns.as_ref().unwrap();
        assert_eq!(columns.execution_us, 120);
        assert_eq!(columns.executed_at, 1_792_500_000_000);
        assert!(tabs[0].result.bytes > 0);
        assert_eq!(
            tabs[1].result.error_detail,
            Some(ErrorDetail::SessionClosed)
        );
        assert_eq!(
            tabs[1].result.error.as_deref(),
            Some("session closed before the end of the query")
        );
    }

    #[tokio::test]
    async fn writes_the_rows_in_chunks_as_they_arrive() {
        let db = store::open_in_memory();
        let user_id = store::ensure_user(&db, "test@example.com").unwrap();
        let editor = create_editor(&db, user_id);
        let state = AppState::new(db, MasterKey::generate(), user_id, exports::unused_folder());
        let recorder = Recorder::new(state.clone(), editor);
        recorder.observe(&mut single_query(0)).await;
        recorder.observe(&mut columns()).await;

        // A chunk is written at the end of the message that fills it.
        let total = 2 * CHUNK_ROWS + 700;
        for from in (0..total).step_by(500) {
            recorder
                .observe(&mut rows(from, total.min(from + 500)))
                .await;
        }
        let snapshot = recorder.snapshot().await;
        assert_eq!(snapshot.queries[0].resumed_rows, total);
        let received = &snapshot.rows[0];
        assert_eq!(received.chunks, 2);
        assert_eq!(received.pending.len(), 700);
        let first = recorder
            .chunk(received.tab_id, received.run, 0)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            serde_json::from_str::<Vec<Vec<Cell>>>(&first)
                .unwrap()
                .len(),
            CHUNK_ROWS
        );

        // The end writes the last chunk; the page reads them in order.
        recorder.observe(&mut done()).await;
        let saved = saved_rows(&state, user_id, editor).await;
        let expected: Vec<Vec<Cell>> = (0..total).map(|row| vec![Some(row.to_string())]).collect();
        assert_eq!(saved[0].1, expected);
        assert!(recorder.snapshot().await.rows[0].pending.is_empty());
    }

    #[tokio::test]
    async fn a_chunk_that_fails_goes_with_the_next_one() {
        let db = store::open_in_memory();
        let user_id = store::ensure_user(&db, "test@example.com").unwrap();
        let editor = create_editor(&db, user_id);
        let state = AppState::new(db, MasterKey::generate(), user_id, exports::unused_folder());
        let recorder = Recorder::new(state.clone(), editor);
        recorder.observe(&mut single_query(0)).await;
        recorder.observe(&mut columns()).await;
        let rename = |from: &'static str, to: &'static str| {
            state.with_db(move |db, _| {
                db.execute_batch(&format!("ALTER TABLE {from} RENAME TO {to}"))
                    .map_err(editors::EditorsError::from)
            })
        };

        // Without its table, the first chunk is not written: its rows wait.
        rename("result_chunks", "hidden_chunks").await.unwrap();
        recorder.observe(&mut rows(0, CHUNK_ROWS)).await;
        let snapshot = recorder.snapshot().await;
        rename("hidden_chunks", "result_chunks").await.unwrap();
        assert_eq!(snapshot.queries[0].resumed_rows, CHUNK_ROWS);
        assert_eq!(snapshot.rows[0].chunks, 0);
        assert_eq!(snapshot.rows[0].pending.len(), CHUNK_ROWS);

        // They go with the next chunk, at the same position.
        recorder
            .observe(&mut rows(CHUNK_ROWS, CHUNK_ROWS + 3))
            .await;
        recorder.observe(&mut done()).await;
        let saved = saved_rows(&state, user_id, editor).await;
        let expected: Vec<Vec<Cell>> = (0..CHUNK_ROWS + 3)
            .map(|row| vec![Some(row.to_string())])
            .collect();
        assert_eq!(saved[0].1, expected);
        assert_eq!(recorder.snapshot().await.rows[0].chunks, 1);
    }

    /// Batch of a single query, refreshing tab `tab` (0 for a new tab).
    fn single_query(tab: i64) -> ServerMessage {
        ServerMessage::Batch {
            queries: vec![BatchQuery {
                query: 0,
                tab,
                sql: "SELECT 1".to_string(),
                start: 0,
                end: 8,
                pivot: None,
            }],
        }
    }

    /// Tab id given by a batch message.
    fn tab_of(batch: &ServerMessage) -> i64 {
        let ServerMessage::Batch { queries } = batch else {
            unreachable!()
        };
        queries[0].tab
    }

    #[tokio::test]
    async fn a_refresh_keeps_the_tabs_and_saves_only_a_complete_result() {
        let db = store::open_in_memory();
        let user_id = store::ensure_user(&db, "test@example.com").unwrap();
        let editor = create_editor(&db, user_id);
        let other = create_editor(&db, user_id);
        let state = AppState::new(db, MasterKey::generate(), user_id, exports::unused_folder());
        let recorder = Recorder::new(state.clone(), editor);
        let saved = |row: &str| -> [ServerMessage; 4] {
            [
                ServerMessage::Started { query: 0 },
                columns(),
                ServerMessage::Rows {
                    query: 0,
                    rows: vec![vec![Some(row.to_string())]],
                },
                done(),
            ]
        };

        let mut batch = single_query(0);
        recorder.observe(&mut batch).await;
        let tab = tab_of(&batch);
        for mut message in saved("first") {
            recorder.observe(&mut message).await;
        }

        // A failed refresh leaves the saved result as it was, and resends no rows.
        let mut refresh = single_query(tab);
        recorder.observe(&mut refresh).await;
        assert_eq!(tab_of(&refresh), tab);
        assert!(recorder.snapshot().await.queries[0].refresh);
        let [mut started, mut columns, mut rows, _] = saved("second");
        recorder.observe(&mut started).await;
        recorder.observe(&mut columns).await;
        recorder.observe(&mut rows).await;
        recorder
            .observe(&mut ServerMessage::Error {
                query: Some(0),
                message: "query interrupted by the user".to_string(),
                detail: ErrorDetail::QueryInterrupted,
            })
            .await;
        let snapshot = recorder.snapshot().await;
        assert_eq!(snapshot.queries[0].resumed_rows, 0);
        assert!(snapshot.rows.is_empty());
        let first = vec![vec![Some("first".to_string())]];
        assert_eq!(saved_rows(&state, user_id, editor).await, [(tab, first)]);

        // A complete refresh replaces it, in the same tab.
        recorder.observe(&mut single_query(tab)).await;
        for mut message in saved("second") {
            recorder.observe(&mut message).await;
        }
        let second = vec![vec![Some("second".to_string())]];
        assert_eq!(saved_rows(&state, user_id, editor).await, [(tab, second)]);

        // The tab of another editor is not refreshed.
        let foreign = state
            .with_db(move |db, _| editors::start_batch(db, other, &[(0, "SELECT 2".into())]))
            .await
            .unwrap()[0]
            .0;
        let mut refresh = single_query(foreign);
        recorder.observe(&mut refresh).await;
        assert_eq!(tab_of(&refresh), 0);
        assert!(recorder.snapshot().await.queries.is_empty());
    }

    /// Creates an editor of the user, and returns its id.
    fn create_editor(db: &rusqlite::Connection, user_id: i64) -> i64 {
        editors::create(db, user_id, &NewEditor::default())
            .unwrap()
            .id
    }
}
