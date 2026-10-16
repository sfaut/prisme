//! SQL editors of the user (tabs at the top of the screen) and their result
//! tabs, saved in the internal database: everything survives a page reload or
//! the browser being closed.
//!
//! Each editor has its text, its connection and its own query session
//! (`GET /api/editors/{id}/session`). The results are saved by the server
//! itself as the run goes (`recorder.rs`): the browser never sends them back.
//! Closing an editor marks it as closed (`closed_at`); it stays reopenable for
//! 24 hours, then it is purged.
//!
//! Routes:
//!
//! | Method and path                         | Role                                                    |
//! |-----------------------------------------|---------------------------------------------------------|
//! | `GET /api/editors`                      | Open editors, in order.                                 |
//! | `POST /api/editors`                     | Creates an empty editor, after the others.              |
//! | `PUT /api/editors/{id}`                 | Changes the name, the text or the connection (204).     |
//! | `DELETE /api/editors/{id}`              | Closes the editor; reopenable for 24 hours (204).       |
//! | `PUT /api/editors/order`                | Orders the open editors as given (204).                 |
//! | `GET /api/editors/closed`               | Editors closed less than 24 hours ago.                  |
//! | `POST /api/editors/{id}/reopen`         | Reopens a closed editor (204).                          |
//! | `GET /api/editors/{id}/results`         | Result tabs of the editor, rows included.               |
//! | `PUT /api/editors/{id}/results/order`   | Orders the result tabs as given (204).                  |
//! | `PUT /api/results/{id}`                 | Renames or pins a result tab (204).                     |
//! | `DELETE /api/results/{id}`              | Deletes a result tab and its rows (204).                |

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use prisme_connectors::mysql::{Cell, ColumnTypeInfo};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::api::AppState;
use crate::api_error::ApiError;
use crate::auth::CurrentUser;
use crate::query_session::LimitReport;

/// Maximum length of the name of an editor or a result tab, in characters.
const NAME_MAX_LENGTH: usize = 100;

/// Duration during which a closed editor stays reopenable, in the format of
/// SQLite's date modifiers.
const REOPEN_DELAY: &str = "-1 day";

/// Current instant, in the format of the internal database's date columns.
const NOW: &str = "strftime('%Y-%m-%dT%H:%M:%fZ', 'now')";

/// Errors managing editors.
#[derive(Debug, thiserror::Error)]
pub enum EditorsError {
    /// Invalid input: the message says which field and why.
    #[error("{0}")]
    Invalid(String),

    /// Error of the internal database.
    #[error("internal database: {0}")]
    Sqlite(#[from] rusqlite::Error),

    /// Unreadable saved result.
    #[error("unreadable saved result: {0}")]
    Json(#[from] serde_json::Error),
}

/// Open editor, as the interface receives it.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct Editor {
    /// Id of the editor.
    pub id: i64,
    /// Name given by the user, empty string for the default name.
    pub name: String,
    /// Connection of the editor, absent as long as none is chosen.
    pub connection_id: Option<i64>,
    /// SQL text of the editor.
    pub sql: String,
}

/// Saved result tab.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct ResultTab {
    /// Id of the tab.
    pub id: i64,
    /// Number of the query in its batch.
    pub query: u32,
    /// Text of the query.
    pub sql: String,
    /// Name given by the user, empty string for the default label.
    pub name: String,
    /// Pinned tab: it stays when the queries are run again.
    pub pinned: bool,
    /// State of the result.
    pub result: SavedResult,
    /// Rows received.
    pub rows: Vec<Vec<Cell>>,
}

/// Columns of a result and what comes with them (`columns` message).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SavedColumns {
    /// Names of the columns.
    pub columns: Vec<String>,
    /// For each column, are its values right-aligned?
    pub right_aligned: Vec<bool>,
    /// Type of each column; empty for a result saved before columns were
    /// typed.
    #[serde(default)]
    pub types: Vec<ColumnTypeInfo>,
    /// Text actually run.
    pub sql: String,
    /// Handling of the row limit.
    pub limit: LimitReport,
    /// Maximum number of rows, `null` if unknown.
    pub expected_rows: Option<u64>,
    /// Execution time measured by the server, in microseconds.
    pub execution_us: u64,
}

/// Saved state of a result, filled as the session's messages arrive.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(default)]
pub struct SavedResult {
    /// Has the query started?
    pub started: bool,
    /// Has the query ended, successfully or not?
    pub done: bool,
    /// Failure message, absent if the query succeeded or is still running.
    pub error: Option<String>,
    /// Columns, absent as long as they are not received (or without a result).
    pub columns: Option<SavedColumns>,
    /// Number of rows received.
    pub row_count: usize,
    /// Was the result truncated by the limit?
    pub truncated: bool,
    /// Rows affected by a write.
    pub affected_rows: u64,
    /// Transfer time of the rows, measured by the server, in microseconds.
    pub transfer_us: u64,
    /// Size of the result's messages, in bytes.
    pub bytes: u64,
}

/// Editor closed less than 24 hours ago, reopenable.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct ClosedEditor {
    /// Id of the editor.
    pub id: i64,
    /// Name given by the user, empty string for the default name.
    pub name: String,
    /// Start of the SQL text, to recognise it.
    pub sql: String,
    /// Closing date, in UTC, in ISO 8601 format.
    pub closed_at: String,
}

/// Change of an editor: name, text and connection, all sent.
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct EditorUpdate {
    /// Name, empty string for the default name.
    pub name: String,
    /// SQL text.
    pub sql: String,
    /// Connection, absent for none.
    pub connection_id: Option<i64>,
}

/// Change of a result tab.
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct ResultTabUpdate {
    /// Name, empty string for the default label.
    pub name: String,
    /// Pinned tab?
    pub pinned: bool,
}

/// New order of a list: ids in display order.
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct Order {
    /// Ids, from first to last.
    pub ids: Vec<i64>,
}

/// Creation of an editor.
#[derive(Debug, Default, Deserialize, TS)]
#[ts(export)]
#[serde(default)]
pub struct NewEditor {
    /// Initial connection, absent for none.
    #[ts(optional)]
    pub connection_id: Option<i64>,
}

/// Checks the length of a name.
fn check_name(name: &str) -> Result<(), EditorsError> {
    let length = name.chars().count();
    if length > NAME_MAX_LENGTH {
        return Err(EditorsError::Invalid(format!(
            "name: {NAME_MAX_LENGTH} characters at most, {length} received"
        )));
    }
    Ok(())
}

/// Deletes the editors closed more than 24 hours ago, all users together,
/// with their result tabs. Returns their number.
pub fn purge(db: &Connection) -> Result<usize, EditorsError> {
    Ok(db.execute(
        &format!(
            "DELETE FROM editors WHERE closed_at != ''
             AND closed_at < strftime('%Y-%m-%dT%H:%M:%fZ', 'now', '{REOPEN_DELAY}')"
        ),
        [],
    )?)
}

/// Open editors of the user, in order (without their result tabs, read
/// editor by editor: `results_of`).
pub fn list_open(db: &Connection, user_id: i64) -> Result<Vec<Editor>, EditorsError> {
    purge(db)?;
    let mut statement = db.prepare(
        "SELECT id, name, connection_id, sql FROM editors
         WHERE user_id = ?1 AND closed_at = '' ORDER BY position, id",
    )?;
    let editors = statement
        .query_map(params![user_id], |row| {
            Ok(Editor {
                id: row.get(0)?,
                name: row.get(1)?,
                connection_id: row.get(2)?,
                sql: row.get(3)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(editors)
}

/// Result tabs of the open editor `editor_id`, if it belongs to the user;
/// `None` otherwise.
pub fn results_of(
    db: &Connection,
    user_id: i64,
    editor_id: i64,
) -> Result<Option<Vec<ResultTab>>, EditorsError> {
    if connection_of(db, user_id, editor_id)?.is_none() {
        return Ok(None);
    }
    Ok(Some(result_tabs(db, editor_id)?))
}

/// Result tabs of an editor, in display order.
fn result_tabs(db: &Connection, editor_id: i64) -> Result<Vec<ResultTab>, EditorsError> {
    let mut statement = db.prepare(
        "SELECT id, query, sql, name, pinned, result, rows FROM result_tabs
         WHERE editor_id = ?1 ORDER BY position, id",
    )?;
    let rows = statement.query_map(params![editor_id], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, u32>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, bool>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, String>(6)?,
        ))
    })?;
    let mut tabs = Vec::new();
    for row in rows {
        let (id, query, sql, name, pinned, result, rows) = row?;
        tabs.push(ResultTab {
            id,
            query,
            sql,
            name,
            pinned,
            result: serde_json::from_str(&result)?,
            rows: serde_json::from_str(&rows)?,
        });
    }
    Ok(tabs)
}

/// Connection of the open editor `id`, if it belongs to the user: `None` if
/// the editor does not exist, `Some(None)` if it has no connection.
pub fn connection_of(
    db: &Connection,
    user_id: i64,
    id: i64,
) -> Result<Option<Option<i64>>, EditorsError> {
    Ok(db
        .query_row(
            "SELECT connection_id FROM editors WHERE id = ?1 AND user_id = ?2 AND closed_at = ''",
            params![id, user_id],
            |row| row.get(0),
        )
        .optional()?)
}

/// Creates an empty editor after the open editors and returns it.
pub fn create(db: &Connection, user_id: i64, new: &NewEditor) -> Result<Editor, EditorsError> {
    db.execute(
        "INSERT INTO editors (user_id, connection_id, position)
         SELECT ?1, ?2, COALESCE(MAX(position) + 1, 0) FROM editors
         WHERE user_id = ?1 AND closed_at = ''",
        params![user_id, new.connection_id],
    )?;
    Ok(Editor {
        id: db.last_insert_rowid(),
        name: String::new(),
        connection_id: new.connection_id,
        sql: String::new(),
    })
}

/// Changes the open editor `id`. Returns `false` if it does not exist or
/// belongs to another user.
pub fn update(
    db: &Connection,
    user_id: i64,
    id: i64,
    changed: &EditorUpdate,
) -> Result<bool, EditorsError> {
    check_name(&changed.name)?;
    let updated = db.execute(
        &format!(
            "UPDATE editors SET name = ?3, sql = ?4, connection_id = ?5, updated_at = {NOW}
             WHERE id = ?1 AND user_id = ?2 AND closed_at = ''"
        ),
        params![
            id,
            user_id,
            changed.name,
            changed.sql,
            changed.connection_id
        ],
    )?;
    Ok(updated == 1)
}

/// Orders the user's open editors as given.
pub fn reorder(db: &Connection, user_id: i64, order: &Order) -> Result<(), EditorsError> {
    for (position, id) in order.ids.iter().enumerate() {
        db.execute(
            "UPDATE editors SET position = ?3 WHERE id = ?1 AND user_id = ?2",
            params![id, user_id, position as i64],
        )?;
    }
    Ok(())
}

/// Closes editor `id`: it stays reopenable for 24 hours. Returns `false` if
/// it does not exist, is already closed or belongs to another user.
pub fn close(db: &Connection, user_id: i64, id: i64) -> Result<bool, EditorsError> {
    let closed = db.execute(
        &format!(
            "UPDATE editors SET closed_at = {NOW}
             WHERE id = ?1 AND user_id = ?2 AND closed_at = ''"
        ),
        params![id, user_id],
    )?;
    Ok(closed == 1)
}

/// Editors closed less than 24 hours ago, from the most recent to the oldest.
pub fn closed_recently(db: &Connection, user_id: i64) -> Result<Vec<ClosedEditor>, EditorsError> {
    purge(db)?;
    let mut statement = db.prepare(
        "SELECT id, name, substr(sql, 1, 200), closed_at FROM editors
         WHERE user_id = ?1 AND closed_at != '' ORDER BY closed_at DESC",
    )?;
    let editors = statement
        .query_map(params![user_id], |row| {
            Ok(ClosedEditor {
                id: row.get(0)?,
                name: row.get(1)?,
                sql: row.get(2)?,
                closed_at: row.get(3)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    Ok(editors)
}

/// Reopens the closed editor `id`, after the open editors. Returns `false` if
/// it does not exist, is already open or belongs to another user.
pub fn reopen(db: &Connection, user_id: i64, id: i64) -> Result<bool, EditorsError> {
    let reopened = db.execute(
        "UPDATE editors SET closed_at = '',
             position = (SELECT COALESCE(MAX(position) + 1, 0) FROM editors
                         WHERE user_id = ?2 AND closed_at = '')
         WHERE id = ?1 AND user_id = ?2 AND closed_at != ''",
        params![id, user_id],
    )?;
    Ok(reopened == 1)
}

/// Renames or pins the result tab `id`, if it belongs to an editor of the
/// user. Returns `false` otherwise.
pub fn update_result_tab(
    db: &Connection,
    user_id: i64,
    id: i64,
    changed: &ResultTabUpdate,
) -> Result<bool, EditorsError> {
    check_name(&changed.name)?;
    let updated = db.execute(
        "UPDATE result_tabs SET name = ?3, pinned = ?4
         WHERE id = ?1 AND editor_id IN (SELECT id FROM editors WHERE user_id = ?2)",
        params![id, user_id, changed.name, changed.pinned],
    )?;
    Ok(updated == 1)
}

/// Deletes the result tab `id` and its rows, if it belongs to an editor of
/// the user. Returns `false` otherwise.
pub fn delete_result_tab(db: &Connection, user_id: i64, id: i64) -> Result<bool, EditorsError> {
    let deleted = db.execute(
        "DELETE FROM result_tabs
         WHERE id = ?1 AND editor_id IN (SELECT id FROM editors WHERE user_id = ?2)",
        params![id, user_id],
    )?;
    Ok(deleted == 1)
}

/// Orders the result tabs of editor `editor_id` as given.
pub fn reorder_result_tabs(
    db: &Connection,
    user_id: i64,
    editor_id: i64,
    order: &Order,
) -> Result<(), EditorsError> {
    for (position, id) in order.ids.iter().enumerate() {
        db.execute(
            "UPDATE result_tabs SET position = ?3
             WHERE id = ?1 AND editor_id = ?4
               AND editor_id IN (SELECT id FROM editors WHERE user_id = ?2)",
            params![id, user_id, position as i64, editor_id],
        )?;
    }
    Ok(())
}

/// Start of a batch in editor `editor_id`: the unpinned tabs disappear, one
/// tab per query is created after the pinned tabs. Returns the ids of the new
/// tabs, in the order of the queries.
pub fn start_batch(
    db: &Connection,
    editor_id: i64,
    queries: &[(u32, String)],
) -> Result<Vec<i64>, EditorsError> {
    db.execute(
        "DELETE FROM result_tabs WHERE editor_id = ?1 AND pinned = 0",
        params![editor_id],
    )?;
    let first: i64 = db.query_row(
        "SELECT COALESCE(MAX(position) + 1, 0) FROM result_tabs WHERE editor_id = ?1",
        params![editor_id],
        |row| row.get(0),
    )?;
    let mut ids = Vec::with_capacity(queries.len());
    for (offset, (query, sql)) in queries.iter().enumerate() {
        db.execute(
            "INSERT INTO result_tabs (editor_id, position, query, sql) VALUES (?1, ?2, ?3, ?4)",
            params![editor_id, first + offset as i64, query, sql],
        )?;
        ids.push(db.last_insert_rowid());
    }
    Ok(ids)
}

/// Saves the state and the rows of a result tab. Without effect if the tab
/// was deleted in the meantime (closed by the user).
pub fn save_result(
    db: &Connection,
    tab_id: i64,
    result: &SavedResult,
    rows: &[Vec<Cell>],
) -> Result<(), EditorsError> {
    db.execute(
        "UPDATE result_tabs SET result = ?2, rows = ?3 WHERE id = ?1",
        params![
            tab_id,
            serde_json::to_string(result)?,
            serde_json::to_string(rows)?
        ],
    )?;
    Ok(())
}

/// Routes of the editors.
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/editors", get(list_editors).post(create_editor))
        .route("/api/editors/order", put(reorder_editors))
        .route("/api/editors/closed", get(list_closed_editors))
        .route("/api/editors/{id}", put(update_editor).delete(close_editor))
        .route("/api/editors/{id}/reopen", post(reopen_editor))
        .route("/api/editors/{id}/results", get(list_results))
        .route("/api/editors/{id}/results/order", put(reorder_results))
        .route(
            "/api/results/{id}",
            put(update_result).delete(delete_result),
        )
}

/// `GET /api/editors`: open editors, in order.
async fn list_editors(
    State(state): State<AppState>,
    user: CurrentUser,
) -> Result<Json<Vec<Editor>>, ApiError> {
    let editors = state.with_db(move |db, _| list_open(db, user.id)).await?;
    Ok(Json(editors))
}

/// `POST /api/editors`: creates an empty editor.
async fn create_editor(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(new): Json<NewEditor>,
) -> Result<(StatusCode, Json<Editor>), ApiError> {
    let editor = state
        .with_db(move |db, _| create(db, user.id, &new))
        .await?;
    Ok((StatusCode::CREATED, Json(editor)))
}

/// Empty response (204) or 404 depending on whether the target was found.
fn no_content_or_not_found(found: bool) -> Result<StatusCode, ApiError> {
    if found {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound)
    }
}

/// `PUT /api/editors/{id}`: changes the name, the text or the connection.
async fn update_editor(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<i64>,
    Json(changed): Json<EditorUpdate>,
) -> Result<StatusCode, ApiError> {
    let found = state
        .with_db(move |db, _| update(db, user.id, id, &changed))
        .await?;
    no_content_or_not_found(found)
}

/// `DELETE /api/editors/{id}`: closes the editor.
async fn close_editor(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    let found = state.with_db(move |db, _| close(db, user.id, id)).await?;
    if found {
        // A closed editor no longer has a session: its connection closes.
        state.sessions.stop_editor(id);
    }
    no_content_or_not_found(found)
}

/// `PUT /api/editors/order`: orders the open editors.
async fn reorder_editors(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(order): Json<Order>,
) -> Result<StatusCode, ApiError> {
    state
        .with_db(move |db, _| reorder(db, user.id, &order))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /api/editors/closed`: editors closed less than 24 hours ago.
async fn list_closed_editors(
    State(state): State<AppState>,
    user: CurrentUser,
) -> Result<Json<Vec<ClosedEditor>>, ApiError> {
    let editors = state
        .with_db(move |db, _| closed_recently(db, user.id))
        .await?;
    Ok(Json(editors))
}

/// `POST /api/editors/{id}/reopen`: reopens a closed editor.
async fn reopen_editor(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    let found = state.with_db(move |db, _| reopen(db, user.id, id)).await?;
    no_content_or_not_found(found)
}

/// `GET /api/editors/{id}/results`: result tabs of the editor.
async fn list_results(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<i64>,
) -> Result<Json<Vec<ResultTab>>, ApiError> {
    let results = state
        .with_db(move |db, _| results_of(db, user.id, id))
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(results))
}

/// `PUT /api/editors/{id}/results/order`: orders the result tabs.
async fn reorder_results(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<i64>,
    Json(order): Json<Order>,
) -> Result<StatusCode, ApiError> {
    state
        .with_db(move |db, _| reorder_result_tabs(db, user.id, id, &order))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `PUT /api/results/{id}`: renames or pins a result tab.
async fn update_result(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<i64>,
    Json(changed): Json<ResultTabUpdate>,
) -> Result<StatusCode, ApiError> {
    let found = state
        .with_db(move |db, _| update_result_tab(db, user.id, id, &changed))
        .await?;
    no_content_or_not_found(found)
}

/// `DELETE /api/results/{id}`: deletes a result tab.
async fn delete_result(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    let found = state
        .with_db(move |db, _| delete_result_tab(db, user.id, id))
        .await?;
    no_content_or_not_found(found)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store;

    /// Test database with one user.
    fn setup() -> (Connection, i64) {
        let db = store::open_in_memory();
        let user_id = store::ensure_user(&db, "test@example.com").unwrap();
        (db, user_id)
    }

    #[test]
    fn creates_updates_orders_and_lists_editors() {
        let (db, user_id) = setup();
        let first = create(&db, user_id, &NewEditor::default()).unwrap();
        let second = create(&db, user_id, &NewEditor::default()).unwrap();
        let changed = EditorUpdate {
            name: "Sales".to_string(),
            sql: "SELECT 1".to_string(),
            connection_id: None,
        };
        assert!(update(&db, user_id, first.id, &changed).unwrap());
        reorder(
            &db,
            user_id,
            &Order {
                ids: vec![second.id, first.id],
            },
        )
        .unwrap();
        let editors = list_open(&db, user_id).unwrap();
        assert_eq!(
            editors.iter().map(|e| e.id).collect::<Vec<_>>(),
            [second.id, first.id]
        );
        assert_eq!(
            (editors[1].name.as_str(), editors[1].sql.as_str()),
            ("Sales", "SELECT 1")
        );
        // Another user neither sees nor changes anything.
        let other = store::ensure_user(&db, "other@example.com").unwrap();
        assert!(list_open(&db, other).unwrap().is_empty());
        assert!(!update(&db, other, first.id, &changed).unwrap());
    }

    #[test]
    fn closes_reopens_and_purges_after_a_day() {
        let (db, user_id) = setup();
        let editor = create(&db, user_id, &NewEditor::default()).unwrap();
        assert!(close(&db, user_id, editor.id).unwrap());
        assert!(list_open(&db, user_id).unwrap().is_empty());
        assert_eq!(closed_recently(&db, user_id).unwrap().len(), 1);
        assert!(reopen(&db, user_id, editor.id).unwrap());
        assert_eq!(list_open(&db, user_id).unwrap().len(), 1);

        // Closed more than 24 hours ago: purged.
        assert!(close(&db, user_id, editor.id).unwrap());
        db.execute(
            "UPDATE editors SET closed_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now', '-25 hours')",
            [],
        )
        .unwrap();
        assert!(closed_recently(&db, user_id).unwrap().is_empty());
        assert!(!reopen(&db, user_id, editor.id).unwrap());
    }

    #[test]
    fn keeps_pinned_result_tabs_when_a_batch_starts() {
        let (db, user_id) = setup();
        let editor = create(&db, user_id, &NewEditor::default()).unwrap();
        let first = start_batch(
            &db,
            editor.id,
            &[(0, "SELECT 1".into()), (1, "SELECT 2".into())],
        )
        .unwrap();
        let result = SavedResult {
            started: true,
            done: true,
            row_count: 1,
            ..SavedResult::default()
        };
        save_result(&db, first[0], &result, &[vec![Some("1".to_string())]]).unwrap();
        let pin = ResultTabUpdate {
            name: "Kept".to_string(),
            pinned: true,
        };
        assert!(update_result_tab(&db, user_id, first[0], &pin).unwrap());

        let second = start_batch(&db, editor.id, &[(0, "SELECT 3".into())]).unwrap();
        let tabs = results_of(&db, user_id, editor.id).unwrap().unwrap();
        assert_eq!(
            tabs.iter().map(|t| t.id).collect::<Vec<_>>(),
            [first[0], second[0]]
        );
        assert_eq!(tabs[0].rows, [vec![Some("1".to_string())]]);
        assert!(tabs[0].result.done && tabs[0].pinned);

        assert!(delete_result_tab(&db, user_id, second[0]).unwrap());
        assert_eq!(
            results_of(&db, user_id, editor.id).unwrap().unwrap().len(),
            1
        );
        assert_eq!(
            results_of(&db, user_id, editor.id).unwrap().unwrap().len(),
            1
        );
        let other = store::ensure_user(&db, "other@example.com").unwrap();
        assert!(results_of(&db, other, editor.id).unwrap().is_none());
    }
}
