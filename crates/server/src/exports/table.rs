//! Table of the exports in the internal database (`exports`): an export is
//! saved as it starts, its outcome as it ends.

use std::collections::HashSet;

use rusqlite::types::Type;
use rusqlite::{Connection, OptionalExtension, Row, params};

use super::{ExportInfo, ExportState, folder};
use crate::error_detail::ErrorDetail;

/// Current instant, in the format of the internal database's date columns.
const NOW: &str = "strftime('%Y-%m-%dT%H:%M:%fZ', 'now')";

/// Columns of `exports` read into an `ExportInfo`, in order.
const COLUMNS: &str = "id, name, connection_name, sql, state, row_count, byte_count,
     expected_rows, created_at, ended_at, error";

/// Export to save, as it starts.
pub struct NewExport<'a> {
    /// Name of the connection.
    pub connection_name: &'a str,
    /// Name of the file offered on download.
    pub name: &'a str,
    /// Query exported.
    pub sql: &'a str,
    /// Rows announced by the interface, if known.
    pub expected_rows: Option<u64>,
}

/// Saves a running export of `user_id`, created at instant `millis` (ms
/// since the Unix epoch), and returns it with the name of its file in the
/// exports folder.
pub fn insert(
    db: &Connection,
    user_id: i64,
    new: &NewExport<'_>,
    millis: u128,
) -> rusqlite::Result<(ExportInfo, String)> {
    let transaction = db.unchecked_transaction()?;
    let id: i64 = transaction.query_row(
        "INSERT INTO exports (user_id, connection_name, name, sql, expected_rows)
         VALUES (?1, ?2, ?3, ?4, ?5) RETURNING id",
        params![
            user_id,
            new.connection_name,
            new.name,
            new.sql,
            stored(new.expected_rows.unwrap_or(0)),
        ],
        |row| row.get(0),
    )?;
    let file = folder::file_name(id, millis);
    let info = transaction.query_row(
        &format!("UPDATE exports SET file = ?1 WHERE id = ?2 RETURNING {COLUMNS}"),
        params![file, id],
        info_of,
    )?;
    transaction.commit()?;
    Ok((info, file))
}

/// Saves the outcome of export `id`, and returns it; `None` if the export
/// was removed in the meantime.
pub fn finish(
    db: &Connection,
    id: i64,
    state: ExportState,
    rows: u64,
    bytes: u64,
    error: Option<&ErrorDetail>,
) -> rusqlite::Result<Option<ExportInfo>> {
    let error = error.map_or_else(String::new, |detail| {
        serde_json::to_string(detail).expect("an error detail always serialises: plain data")
    });
    db.query_row(
        &format!(
            "UPDATE exports SET state = ?1, row_count = ?2, byte_count = ?3, error = ?4,
                 ended_at = {NOW}
             WHERE id = ?5 RETURNING {COLUMNS}"
        ),
        params![state.as_str(), stored(rows), stored(bytes), error, id],
        info_of,
    )
    .optional()
}

/// Marks the exports still running as interrupted: at startup, those that
/// Prisme was writing when it stopped. Their files stay. Returns their number.
pub fn interrupt_unfinished(db: &Connection) -> rusqlite::Result<usize> {
    db.execute(
        &format!(
            "UPDATE exports SET state = 'interrupted', ended_at = {NOW} WHERE state = 'running'"
        ),
        [],
    )
}

/// Exports of `user_id`, the most recent first, each with its file.
pub fn list(db: &Connection, user_id: i64) -> rusqlite::Result<Vec<(ExportInfo, String)>> {
    let mut statement = db.prepare(&format!(
        "SELECT {COLUMNS}, file FROM exports WHERE user_id = ?1 ORDER BY id DESC"
    ))?;
    statement
        .query_map([user_id], |row| Ok((info_of(row)?, row.get(11)?)))?
        .collect()
}

/// Files of all the exports, all users together.
pub fn files(db: &Connection) -> rusqlite::Result<HashSet<String>> {
    let mut statement = db.prepare("SELECT file FROM exports")?;
    statement.query_map([], |row| row.get(0))?.collect()
}

/// File of export `id` of `user_id` and the name offered on download;
/// `None` if the export does not exist or belongs to another user.
pub fn file_of(
    db: &Connection,
    user_id: i64,
    id: i64,
) -> rusqlite::Result<Option<(String, String)>> {
    db.query_row(
        "SELECT file, name FROM exports WHERE id = ?1 AND user_id = ?2",
        params![id, user_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .optional()
}

/// Deletes export `id` of `user_id` from the table. Returns `false` if it
/// does not exist or belongs to another user.
pub fn delete(db: &Connection, user_id: i64, id: i64) -> rusqlite::Result<bool> {
    let deleted = db.execute(
        "DELETE FROM exports WHERE id = ?1 AND user_id = ?2",
        params![id, user_id],
    )?;
    Ok(deleted > 0)
}

/// Counter in an `INTEGER` column, saturated.
fn stored(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

/// Reads a row of `exports` (columns `COLUMNS`).
fn info_of(row: &Row<'_>) -> rusqlite::Result<ExportInfo> {
    let state: String = row.get(4)?;
    let state = ExportState::parse(&state).ok_or_else(|| {
        let cause = format!("unknown export state {state:?}");
        rusqlite::Error::FromSqlConversionFailure(4, Type::Text, cause.into())
    })?;
    let error: String = row.get(10)?;
    let error = if error.is_empty() {
        None
    } else {
        let detail = serde_json::from_str(&error).map_err(|cause| {
            rusqlite::Error::FromSqlConversionFailure(10, Type::Text, cause.into())
        })?;
        Some(detail)
    };
    let expected_rows = row.get::<_, i64>(7)?.unsigned_abs();
    let ended_at: String = row.get(9)?;
    Ok(ExportInfo {
        id: row.get(0)?,
        name: row.get(1)?,
        connection_name: row.get(2)?,
        sql: row.get(3)?,
        state,
        rows: row.get::<_, i64>(5)?.unsigned_abs(),
        bytes: row.get::<_, i64>(6)?.unsigned_abs(),
        expected_rows: (expected_rows > 0).then_some(expected_rows),
        created_at: row.get(8)?,
        ended_at: (!ended_at.is_empty()).then_some(ended_at),
        error,
        file_missing: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store;

    #[test]
    fn saves_an_export_then_its_outcome() {
        let db = store::open_in_memory();
        let user_id = store::ensure_user(&db, "test@example.com").unwrap();
        let new = NewExport {
            connection_name: "local",
            name: "ventes.csv",
            sql: "SELECT 1",
            expected_rows: None,
        };
        let (export, file) = insert(&db, user_id, &new, 1_792_500_000_000).unwrap();
        assert_eq!(file, format!("{}-1792500000000.csv", export.id));
        assert_eq!(export.state, ExportState::Running);
        assert_eq!((export.expected_rows, export.ended_at), (None, None));

        let detail = ErrorDetail::ExportNotRead;
        let ended = finish(&db, export.id, ExportState::Failed, 3, 40, Some(&detail))
            .unwrap()
            .unwrap();
        assert_eq!(
            (ended.state, ended.rows, ended.bytes),
            (ExportState::Failed, 3, 40)
        );
        assert_eq!(ended.error, Some(detail));
        assert!(ended.ended_at.is_some());

        assert_eq!(list(&db, user_id).unwrap()[0].1, file);
        assert_eq!(files(&db).unwrap(), HashSet::from([file.clone()]));
        assert!(delete(&db, user_id, export.id).unwrap());
        assert!(
            finish(&db, export.id, ExportState::Ready, 0, 0, None)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn interrupts_the_exports_left_running() {
        let db = store::open_in_memory();
        let user_id = store::ensure_user(&db, "test@example.com").unwrap();
        let new = NewExport {
            connection_name: "local",
            name: "ventes.csv",
            sql: "SELECT 1",
            expected_rows: Some(10),
        };
        let (running, _) = insert(&db, user_id, &new, 1).unwrap();
        let (ready, _) = insert(&db, user_id, &new, 2).unwrap();
        finish(&db, ready.id, ExportState::Ready, 10, 20, None).unwrap();

        assert_eq!(interrupt_unfinished(&db).unwrap(), 1);
        let exports = list(&db, user_id).unwrap();
        let state_of = |id| {
            exports
                .iter()
                .find(|(export, _)| export.id == id)
                .unwrap()
                .0
                .state
        };
        assert_eq!(state_of(running.id), ExportState::Interrupted);
        assert_eq!(state_of(ready.id), ExportState::Ready);
        assert_eq!(exports[0].0.expected_rows, Some(10));
    }
}
