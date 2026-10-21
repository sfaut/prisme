//! Recorder of an editor's results, plugged into the relay of the messages of
//! its query session (`session_relay.rs`).
//!
//! Each message sent to the client first goes through the recorder:
//!
//! - `batch`: the editor's unpinned tabs disappear, one tab per query is
//!   created; its id goes to the client with the query. A refresh keeps
//!   the tabs: its query runs again in its own tab;
//! - `started`, `columns`, `rows`: the state and the rows of each query
//!   accumulate in memory (rows are bounded by the row limit);
//! - `done`, `error`: the complete result is written to the internal
//!   database. A refresh only writes a complete result: on failure, the
//!   saved one stays as it was.
//!
//! The browser therefore never sends the results back: after a reload, it
//! reads them again as the server received them. At the end of the session,
//! an unfinished query is saved as failed.
//!
//! The last batch stays in memory until the next one, finished queries
//! included: a WebSocket that reattaches to the session receives it as is
//! (`snapshot`), even if a query ended between the page reading the saved
//! tabs and reattaching.

use std::collections::HashMap;
use std::time::Instant;

use prisme_connectors::mysql::Cell;
use serde::Serialize;
use tokio::sync::Mutex;
use ts_rs::TS;

use crate::api::AppState;
use crate::editors::{self, SavedColumns, SavedResult};
use crate::error_detail::ErrorDetail;
use crate::query_session::ServerMessage;
use crate::query_session::protocol::BatchQuery;

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
    /// Rows received so far.
    rows: Vec<Vec<Cell>>,
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
    /// Rows received.
    rows: Vec<Vec<Cell>>,
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
                    record.rows.extend(rows.iter().cloned());
                    record.result.row_count = record.rows.len();
                    record.result.bytes += bytes;
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
                    if !record.refresh {
                        self.save(record).await;
                    }
                }
            }
            _ => {}
        }
    }

    /// Last batch, query by query, where it stands.
    pub(crate) async fn snapshot(&self) -> Vec<ResumedQuery> {
        let records = self.records.lock().await;
        let mut queries: Vec<ResumedQuery> = records
            .iter()
            .map(|(&query, record)| ResumedQuery {
                query,
                tab: record.tab_id,
                sql: record.sql.clone(),
                result: record.result.clone(),
                rows: record.rows.clone(),
                started_us_ago: record.started_at.map(micros_since),
                columns_us_ago: record.columns_at.map(micros_since),
                refresh: record.refresh,
            })
            .collect();
        queries.sort_by_key(|query| query.query);
        queries
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
            if !record.refresh {
                self.save(record).await;
            }
        }
        records.clear();
    }

    /// New batch: the editor's unpinned tabs disappear, and each query gets a
    /// new tab, whose id it takes.
    async fn create_tabs(&self, records: &mut HashMap<u32, Record>, queries: &mut [BatchQuery]) {
        let editor_id = self.editor_id;
        let wanted: Vec<_> = queries.iter().map(|q| (q.query, q.sql.clone())).collect();
        let created = self
            .state
            .with_db(move |db, _| editors::start_batch(db, editor_id, &wanted))
            .await;
        match created {
            Ok(ids) => {
                for (query, tab_id) in queries.iter_mut().zip(ids) {
                    query.tab = tab_id;
                    records.insert(query.query, Record::new(tab_id, &query.sql, false));
                }
            }
            Err(_) => tracing::warn!("result tabs not created, editor {editor_id}"),
        }
    }

    /// Refresh of a tab: the query runs again in it, the other tabs stay. A
    /// tab that is not the editor's (closed in the meantime, or another
    /// editor's) loses its id: the result is then not recorded.
    async fn refresh(&self, records: &mut HashMap<u32, Record>, query: &mut BatchQuery) {
        let editor_id = self.editor_id;
        let tab_id = query.tab;
        let owned = self
            .state
            .with_db(move |db, _| editors::has_result_tab(db, editor_id, tab_id))
            .await;
        if matches!(owned, Ok(true)) {
            records.insert(query.query, Record::new(tab_id, &query.sql, true));
        } else {
            query.tab = 0;
        }
    }

    /// Writes the result of a query to the internal database; a failure is
    /// logged, without interrupting the session.
    async fn save(&self, record: &Record) {
        let tab_id = record.tab_id;
        let result = record.result.clone();
        let rows = record.rows.clone();
        let saved = self
            .state
            .with_db(move |db, _| editors::save_result(db, tab_id, &result, &rows))
            .await;
        if saved.is_err() {
            tracing::warn!("result not saved, tab {tab_id}");
        }
    }
}

impl Record {
    /// Empty result of tab `tab_id`, for the query `sql`, which `refresh`es the
    /// tab or not.
    fn new(tab_id: i64, sql: &str, refresh: bool) -> Self {
        Self {
            tab_id,
            sql: sql.to_string(),
            result: SavedResult::default(),
            rows: Vec::new(),
            started_at: None,
            columns_at: None,
            refresh,
        }
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
    use crate::master_key::MasterKey;
    use crate::query_session::LimitReport;
    use crate::store;

    #[tokio::test]
    async fn records_each_result_and_closes_unfinished_ones() {
        let db = store::open_in_memory();
        let user_id = store::ensure_user(&db, "test@example.com").unwrap();
        let editor = editors::create(&db, user_id, &NewEditor::default()).unwrap();
        let state = AppState::new(db, MasterKey::generate(), user_id);
        let recorder = Recorder::new(state.clone(), editor.id);

        let mut batch = ServerMessage::Batch {
            queries: (0..2)
                .map(|query| BatchQuery {
                    query,
                    tab: 0,
                    sql: format!("SELECT {query}"),
                    start: 0,
                    end: 8,
                })
                .collect(),
        };
        recorder.observe(&mut batch).await;
        let ServerMessage::Batch { queries } = &batch else {
            unreachable!()
        };
        assert!(queries.iter().all(|query| query.tab > 0));

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
            ServerMessage::Rows {
                query: 0,
                rows: vec![vec![Some("0".to_string())]],
            },
            ServerMessage::Done {
                query: 0,
                row_count: 1,
                truncated: false,
                affected_rows: 0,
                elapsed_ms: 1,
                database: None,
            },
            ServerMessage::Started { query: 1 },
        ] {
            recorder.observe(&mut message).await;
        }
        let snapshot = recorder.snapshot().await;
        assert_eq!(snapshot.len(), 2);
        assert!(snapshot[0].result.done && snapshot[0].rows.len() == 1);
        assert!(!snapshot[1].result.done && snapshot[1].started_us_ago.is_some());
        assert!(snapshot[1].columns_us_ago.is_none());
        recorder.finish().await;
        assert!(recorder.snapshot().await.is_empty());

        let editor_id = editor.id;
        let tabs = state
            .with_db(move |db, _| editors::results_of(db, user_id, editor_id))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(tabs[0].rows, [vec![Some("0".to_string())]]);
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

    /// Batch of a single query, refreshing tab `tab` (0 for a new tab).
    fn single_query(tab: i64) -> ServerMessage {
        ServerMessage::Batch {
            queries: vec![BatchQuery {
                query: 0,
                tab,
                sql: "SELECT 1".to_string(),
                start: 0,
                end: 8,
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
        let state = AppState::new(db, MasterKey::generate(), user_id);
        let recorder = Recorder::new(state.clone(), editor);
        let saved = |row: &str| -> [ServerMessage; 4] {
            [
                ServerMessage::Started { query: 0 },
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
                },
                ServerMessage::Rows {
                    query: 0,
                    rows: vec![vec![Some(row.to_string())]],
                },
                ServerMessage::Done {
                    query: 0,
                    row_count: 1,
                    truncated: false,
                    affected_rows: 0,
                    elapsed_ms: 1,
                    database: None,
                },
            ]
        };
        let rows_of = |state: AppState| async move {
            state
                .with_db(move |db, _| editors::results_of(db, user_id, editor))
                .await
                .unwrap()
                .unwrap()
                .into_iter()
                .map(|tab| (tab.id, tab.rows))
                .collect::<Vec<_>>()
        };

        let mut batch = single_query(0);
        recorder.observe(&mut batch).await;
        let tab = tab_of(&batch);
        for mut message in saved("first") {
            recorder.observe(&mut message).await;
        }

        // A failed refresh leaves the saved result as it was.
        let mut refresh = single_query(tab);
        recorder.observe(&mut refresh).await;
        assert_eq!(tab_of(&refresh), tab);
        assert!(recorder.snapshot().await[0].refresh);
        let [mut started, mut columns, ..] = saved("second");
        recorder.observe(&mut started).await;
        recorder.observe(&mut columns).await;
        recorder
            .observe(&mut ServerMessage::Error {
                query: Some(0),
                message: "query interrupted by the user".to_string(),
                detail: ErrorDetail::QueryInterrupted,
            })
            .await;
        let first = vec![vec![Some("first".to_string())]];
        assert_eq!(rows_of(state.clone()).await, [(tab, first)]);

        // A complete refresh replaces it, in the same tab.
        recorder.observe(&mut single_query(tab)).await;
        for mut message in saved("second") {
            recorder.observe(&mut message).await;
        }
        let second = vec![vec![Some("second".to_string())]];
        assert_eq!(rows_of(state.clone()).await, [(tab, second)]);

        // The tab of another editor is not refreshed.
        let foreign = state
            .with_db(move |db, _| editors::start_batch(db, other, &[(0, "SELECT 2".into())]))
            .await
            .unwrap()[0];
        let mut refresh = single_query(foreign);
        recorder.observe(&mut refresh).await;
        assert_eq!(tab_of(&refresh), 0);
        assert!(recorder.snapshot().await.is_empty());
    }

    /// Creates an editor of the user, and returns its id.
    fn create_editor(db: &rusqlite::Connection, user_id: i64) -> i64 {
        editors::create(db, user_id, &NewEditor::default())
            .unwrap()
            .id
    }
}
