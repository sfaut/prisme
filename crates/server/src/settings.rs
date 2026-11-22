//! Settings of the user: the default row limit, added to the queries that do
//! not write one (see `docs/V1.md`, section 2.3), the read-only mode of the
//! query sessions and the writes they allow (`docs/V1.md`, section 2.4), the
//! retention of the console's history and debug log, the indentation of
//! the SQL editor, the longest text of a `GROUP_CONCAT` of the Pivoteur, and
//! the shortest delay between two computations of the Pivoteur during the
//! reception of the rows.
//!
//! The settings are stored in the `users` table (see `migrations/0003_users_row_limit.sql`,
//! `migrations/0011_users_read_only_session.sql`,
//! `migrations/0014_users_console_retention.sql`,
//! `migrations/0016_users_indent_size.sql`,
//! `migrations/0025_users_concat_max_bytes.sql` and
//! `migrations/0026_users_pivot_refresh_ms.sql`,
//! `migrations/0039_users_remote_locations.sql`), the allowed writes in the
//! `user_allowed_writes` table (`migrations/0017_user_allowed_writes.sql`,
//! its kinds in `write_kinds` since `migrations/0038_write_kinds.sql`).
//! Synchronous functions (`rusqlite`), called from blocking tasks.

use crate::error_detail::{ErrorDetail, Field};
use prisme_dialects::classify::WriteKind;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Row limits offered to the user. The only place to change to modify the list.
pub const ROW_LIMIT_CHOICES: [u32; 5] = [1_000, 2_000, 5_000, 10_000, 50_000];

/// Bounds of a retention of the console, in days: the database checks the same.
pub const RETENTION_DAYS: std::ops::RangeInclusive<u16> = 1..=3650;

/// Bounds of the indentation of the SQL editor, in spaces: the database checks the same.
pub const INDENT_SIZE: std::ops::RangeInclusive<u8> = 1..=8;

/// Bounds of the longest text of a `GROUP_CONCAT`, in bytes: from room for a
/// few values to the default `group_concat_max_len` of MariaDB (1 MiB). The
/// database checks the same.
pub const CONCAT_MAX_BYTES: std::ops::RangeInclusive<u32> = 100..=1_048_576;

/// Bounds of the shortest delay between the starts of two computations of the
/// Pivoteur during the reception of the rows, in milliseconds: from 0.2 s to
/// one minute. The database checks the same.
pub const PIVOT_REFRESH_MS: std::ops::RangeInclusive<u32> = 200..=60_000;

/// Bounds of the wait for the connection to a web server, in seconds. The
/// database checks the same.
pub const REMOTE_CONNECT_TIMEOUT_SECONDS: std::ops::RangeInclusive<u32> = 1..=300;

/// Bounds of the wait for a whole download, in minutes: up to a day. The
/// database checks the same.
pub const REMOTE_DOWNLOAD_TIMEOUT_MINUTES: std::ops::RangeInclusive<u32> = 1..=1_440;

/// Bounds of the largest remote file downloaded, in mebibytes: up to
/// 100 GiB. The database checks the same.
pub const REMOTE_MAX_MEBIBYTES: std::ops::RangeInclusive<u32> = 1..=102_400;

/// Bounds of the days after which an unused copy of a remote file is
/// deleted. The database checks the same.
pub const REMOTE_CACHE_DAYS: std::ops::RangeInclusive<u32> = 1..=3_650;

/// Settings of a user, as returned by the API.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct Settings {
    /// Default row limit.
    pub row_limit: u32,
    /// Allowed limits, to build the interface's list of choices.
    #[ts(type = "Array<number>")]
    pub row_limit_choices: &'static [u32],
    /// Are the query sessions in read-only mode? On by default: the database
    /// then refuses the writes too.
    pub read_only_session: bool,
    /// Writes allowed when the sessions are not read-only, in the order of
    /// `WriteKind::ALL`; none by default.
    pub allowed_writes: Vec<WriteKind>,
    /// Days the queries of the console's history are kept, 30 by default.
    pub history_retention_days: u16,
    /// Days the lines of the console's debug log are kept, 30 by default.
    pub debug_retention_days: u16,
    /// Spaces of an indentation level in the SQL editor, 4 by default.
    pub indent_size: u8,
    /// Longest text of a `GROUP_CONCAT` of the Pivoteur, in bytes of UTF-8,
    /// 1000 by default.
    pub concat_max_bytes: u32,
    /// Shortest delay between the starts of two computations of the table of
    /// the Pivoteur while the rows of a result are being received, in
    /// milliseconds, 1000 by default.
    pub pivot_refresh_ms: u32,
    /// Longest wait for the connection to a web server of a remote
    /// database, in seconds, 10 by default.
    pub remote_connect_timeout_seconds: u32,
    /// Longest wait for the whole download of a remote database, in
    /// minutes, 5 by default.
    pub remote_download_timeout_minutes: u32,
    /// Largest remote database downloaded, in mebibytes, 1024 by default.
    pub remote_max_mebibytes: u32,
    /// Days after which a copy of a remote database no session opened is
    /// deleted from the cache, 30 by default.
    pub remote_cache_days: u32,
}

/// Change of settings requested by the user.
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct SettingsUpdate {
    /// New default row limit, among `ROW_LIMIT_CHOICES`.
    pub row_limit: u32,
    /// New read-only mode of the query sessions.
    pub read_only_session: bool,
    /// New writes allowed (the others are not).
    pub allowed_writes: Vec<WriteKind>,
    /// New retention of the history, in days (`RETENTION_DAYS`).
    pub history_retention_days: u16,
    /// New retention of the debug log, in days (`RETENTION_DAYS`).
    pub debug_retention_days: u16,
    /// New indentation of the SQL editor, in spaces (`INDENT_SIZE`).
    pub indent_size: u8,
    /// New longest text of a `GROUP_CONCAT`, in bytes (`CONCAT_MAX_BYTES`).
    pub concat_max_bytes: u32,
    /// New shortest delay between two computations of the Pivoteur during the
    /// reception, in milliseconds (`PIVOT_REFRESH_MS`).
    pub pivot_refresh_ms: u32,
    /// New wait for the connection to a web server, in seconds
    /// (`REMOTE_CONNECT_TIMEOUT_SECONDS`).
    pub remote_connect_timeout_seconds: u32,
    /// New wait for a whole download, in minutes
    /// (`REMOTE_DOWNLOAD_TIMEOUT_MINUTES`).
    pub remote_download_timeout_minutes: u32,
    /// New largest remote file, in mebibytes (`REMOTE_MAX_MEBIBYTES`).
    pub remote_max_mebibytes: u32,
    /// New delay of the purge of the cache, in days (`REMOTE_CACHE_DAYS`).
    pub remote_cache_days: u32,
}

/// Errors of settings management.
#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    /// Limit outside the list of choices.
    #[error("row_limit: {0} is not allowed, possible choices: {choices:?}", choices = ROW_LIMIT_CHOICES)]
    InvalidRowLimit(u32),

    /// Retention out of its bounds.
    #[error("{field}: between {min} and {max} days", min = RETENTION_DAYS.start(), max = RETENTION_DAYS.end())]
    RetentionOutOfRange {
        /// Field out of its bounds.
        field: Field,
    },

    /// Indentation out of its bounds.
    #[error("indent_size: between {min} and {max} spaces", min = INDENT_SIZE.start(), max = INDENT_SIZE.end())]
    IndentSizeOutOfRange,

    /// Longest text of a `GROUP_CONCAT` out of its bounds.
    #[error("concat_max_bytes: between {min} and {max} bytes", min = CONCAT_MAX_BYTES.start(), max = CONCAT_MAX_BYTES.end())]
    ConcatMaxBytesOutOfRange,

    /// Delay between two computations of the Pivoteur out of its bounds.
    #[error("pivot_refresh_ms: between {min} and {max} milliseconds", min = PIVOT_REFRESH_MS.start(), max = PIVOT_REFRESH_MS.end())]
    PivotRefreshMsOutOfRange,

    /// A preference of the remote locations out of its bounds.
    #[error("{field}: between {min} and {max}")]
    RemoteOutOfRange {
        /// Field out of its bounds.
        field: Field,
        /// Smallest value allowed.
        min: u32,
        /// Largest value allowed.
        max: u32,
    },

    /// Error of the internal database.
    #[error("internal database: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

impl SettingsError {
    /// What went wrong, for the interface; `Internal` for a failure of the
    /// internal database, detailed in the log only.
    pub fn detail(&self) -> ErrorDetail {
        match self {
            Self::InvalidRowLimit(value) => ErrorDetail::RowLimitNotAllowed {
                value: *value,
                choices: ROW_LIMIT_CHOICES.to_vec(),
            },
            Self::RetentionOutOfRange { field } => ErrorDetail::FieldRange {
                field: *field,
                min: (*RETENTION_DAYS.start()).into(),
                max: (*RETENTION_DAYS.end()).into(),
            },
            Self::IndentSizeOutOfRange => ErrorDetail::FieldRange {
                field: Field::IndentSize,
                min: (*INDENT_SIZE.start()).into(),
                max: (*INDENT_SIZE.end()).into(),
            },
            Self::ConcatMaxBytesOutOfRange => ErrorDetail::FieldRange {
                field: Field::ConcatMaxBytes,
                min: *CONCAT_MAX_BYTES.start(),
                max: *CONCAT_MAX_BYTES.end(),
            },
            Self::PivotRefreshMsOutOfRange => ErrorDetail::FieldRange {
                field: Field::PivotRefreshMs,
                min: *PIVOT_REFRESH_MS.start(),
                max: *PIVOT_REFRESH_MS.end(),
            },
            Self::RemoteOutOfRange { field, min, max } => ErrorDetail::FieldRange {
                field: *field,
                min: *min,
                max: *max,
            },
            Self::Sqlite(_) => ErrorDetail::Internal,
        }
    }
}

/// Checks that a row limit is among the allowed choices.
pub fn check_row_limit(row_limit: u32) -> Result<u32, SettingsError> {
    if ROW_LIMIT_CHOICES.contains(&row_limit) {
        Ok(row_limit)
    } else {
        Err(SettingsError::InvalidRowLimit(row_limit))
    }
}

/// Checks the row limit, the retentions, the indentation, the longest text
/// of a `GROUP_CONCAT` and the delay between two computations of the
/// Pivoteur of an update.
///
/// # Errors
///
/// Returns `SettingsError::InvalidRowLimit` if the limit is not among
/// `ROW_LIMIT_CHOICES`, `SettingsError::RetentionOutOfRange` if a retention
/// is out of `RETENTION_DAYS`, `SettingsError::IndentSizeOutOfRange` if the
/// indentation is out of `INDENT_SIZE`, `SettingsError::ConcatMaxBytesOutOfRange`
/// if the longest text is out of `CONCAT_MAX_BYTES`,
/// `SettingsError::PivotRefreshMsOutOfRange` if the delay is out of
/// `PIVOT_REFRESH_MS`, `SettingsError::RemoteOutOfRange` if a preference of
/// the remote locations is out of its bounds (`REMOTE_*`).
pub fn check_update(update: &SettingsUpdate) -> Result<(), SettingsError> {
    check_row_limit(update.row_limit)?;
    for (field, days) in [
        (Field::HistoryRetentionDays, update.history_retention_days),
        (Field::DebugRetentionDays, update.debug_retention_days),
    ] {
        if !RETENTION_DAYS.contains(&days) {
            return Err(SettingsError::RetentionOutOfRange { field });
        }
    }
    if !INDENT_SIZE.contains(&update.indent_size) {
        return Err(SettingsError::IndentSizeOutOfRange);
    }
    if !CONCAT_MAX_BYTES.contains(&update.concat_max_bytes) {
        return Err(SettingsError::ConcatMaxBytesOutOfRange);
    }
    if !PIVOT_REFRESH_MS.contains(&update.pivot_refresh_ms) {
        return Err(SettingsError::PivotRefreshMsOutOfRange);
    }
    for (field, value, bounds) in [
        (
            Field::RemoteConnectTimeoutSeconds,
            update.remote_connect_timeout_seconds,
            REMOTE_CONNECT_TIMEOUT_SECONDS,
        ),
        (
            Field::RemoteDownloadTimeoutMinutes,
            update.remote_download_timeout_minutes,
            REMOTE_DOWNLOAD_TIMEOUT_MINUTES,
        ),
        (
            Field::RemoteMaxMebibytes,
            update.remote_max_mebibytes,
            REMOTE_MAX_MEBIBYTES,
        ),
        (
            Field::RemoteCacheDays,
            update.remote_cache_days,
            REMOTE_CACHE_DAYS,
        ),
    ] {
        if !bounds.contains(&value) {
            return Err(SettingsError::RemoteOutOfRange {
                field,
                min: *bounds.start(),
                max: *bounds.end(),
            });
        }
    }
    Ok(())
}

/// Reads the settings of user `user_id`.
pub fn load(db: &Connection, user_id: i64) -> Result<Settings, SettingsError> {
    let settings = db.query_row(
        "SELECT row_limit, read_only_session, history_retention_days, debug_retention_days,
                indent_size, concat_max_bytes, pivot_refresh_ms, remote_connect_timeout_seconds,
                remote_download_timeout_minutes, remote_max_mebibytes, remote_cache_days
         FROM users WHERE id = ?1",
        params![user_id],
        |row| {
            Ok(Settings {
                row_limit: row.get(0)?,
                row_limit_choices: &ROW_LIMIT_CHOICES,
                read_only_session: row.get(1)?,
                allowed_writes: Vec::new(),
                history_retention_days: row.get(2)?,
                debug_retention_days: row.get(3)?,
                indent_size: row.get(4)?,
                concat_max_bytes: row.get(5)?,
                pivot_refresh_ms: row.get(6)?,
                remote_connect_timeout_seconds: row.get(7)?,
                remote_download_timeout_minutes: row.get(8)?,
                remote_max_mebibytes: row.get(9)?,
                remote_cache_days: row.get(10)?,
            })
        },
    )?;
    Ok(Settings {
        allowed_writes: allowed_writes(db, user_id)?,
        ..settings
    })
}

/// Writes allowed to user `user_id`, in the order of `WriteKind::ALL`.
fn allowed_writes(db: &Connection, user_id: i64) -> Result<Vec<WriteKind>, rusqlite::Error> {
    let mut statement = db.prepare("SELECT kind FROM user_allowed_writes WHERE user_id = ?1")?;
    let names = statement
        .query_map(params![user_id], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(WriteKind::ALL
        .into_iter()
        .filter(|kind| names.iter().any(|name| name == kind.as_str()))
        .collect())
}

/// Changes the settings of user `user_id` and returns the updated settings.
pub fn update(
    db: &Connection,
    user_id: i64,
    update: &SettingsUpdate,
) -> Result<Settings, SettingsError> {
    check_update(update)?;
    let transaction = db.unchecked_transaction()?;
    transaction.execute(
        "UPDATE users SET row_limit = ?1, read_only_session = ?2,
                history_retention_days = ?3, debug_retention_days = ?4, indent_size = ?5,
                concat_max_bytes = ?6, pivot_refresh_ms = ?7, remote_connect_timeout_seconds = ?8,
                remote_download_timeout_minutes = ?9, remote_max_mebibytes = ?10,
                remote_cache_days = ?11
         WHERE id = ?12",
        params![
            update.row_limit,
            update.read_only_session,
            update.history_retention_days,
            update.debug_retention_days,
            update.indent_size,
            update.concat_max_bytes,
            update.pivot_refresh_ms,
            update.remote_connect_timeout_seconds,
            update.remote_download_timeout_minutes,
            update.remote_max_mebibytes,
            update.remote_cache_days,
            user_id
        ],
    )?;
    transaction.execute(
        "DELETE FROM user_allowed_writes WHERE user_id = ?1",
        params![user_id],
    )?;
    for kind in &update.allowed_writes {
        transaction.execute(
            "INSERT OR IGNORE INTO user_allowed_writes (user_id, kind) VALUES (?1, ?2)",
            params![user_id, kind.as_str()],
        )?;
    }
    transaction.commit()?;
    load(db, user_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store;

    /// Update with the default values, except the row limit `row_limit`.
    fn update_of(row_limit: u32) -> SettingsUpdate {
        SettingsUpdate {
            row_limit,
            read_only_session: true,
            allowed_writes: Vec::new(),
            history_retention_days: 30,
            debug_retention_days: 30,
            indent_size: 4,
            concat_max_bytes: 1000,
            pivot_refresh_ms: 1000,
            remote_connect_timeout_seconds: 10,
            remote_download_timeout_minutes: 5,
            remote_max_mebibytes: 1024,
            remote_cache_days: 30,
        }
    }

    #[test]
    fn keeps_the_preferences_of_the_remote_locations_within_their_bounds() {
        let db = store::open_in_memory();
        let user_id = store::ensure_user(&db, "a@example.com").unwrap();
        let saved = load(&db, user_id).unwrap();
        // The defaults of decision 15 (docs/TODO-SQLITE.md).
        assert_eq!(
            (
                saved.remote_connect_timeout_seconds,
                saved.remote_download_timeout_minutes,
                saved.remote_max_mebibytes,
                saved.remote_cache_days
            ),
            (10, 5, 1024, 30)
        );

        let mut changed = update_of(1_000);
        changed.remote_connect_timeout_seconds = 3;
        changed.remote_download_timeout_minutes = 60;
        changed.remote_max_mebibytes = 10;
        changed.remote_cache_days = 7;
        let saved = update(&db, user_id, &changed).unwrap();
        assert_eq!(
            (
                saved.remote_connect_timeout_seconds,
                saved.remote_download_timeout_minutes,
                saved.remote_max_mebibytes,
                saved.remote_cache_days
            ),
            (3, 60, 10, 7)
        );

        changed.remote_cache_days = 0;
        assert!(matches!(
            update(&db, user_id, &changed),
            Err(SettingsError::RemoteOutOfRange {
                field: Field::RemoteCacheDays,
                ..
            })
        ));
        changed.remote_cache_days = 7;
        changed.remote_max_mebibytes = 102_401;
        assert!(matches!(
            check_update(&changed),
            Err(SettingsError::RemoteOutOfRange {
                field: Field::RemoteMaxMebibytes,
                ..
            })
        ));
    }

    #[test]
    fn allows_no_write_by_default_then_the_writes_chosen() {
        let db = store::open_in_memory();
        let user_id = store::ensure_user(&db, "a@example.com").unwrap();
        assert!(load(&db, user_id).unwrap().allowed_writes.is_empty());

        // Every kind is accepted by the table, and comes back in its order.
        let mut changed = update_of(1_000);
        changed.allowed_writes = WriteKind::ALL.into_iter().rev().collect();
        assert_eq!(
            update(&db, user_id, &changed).unwrap().allowed_writes,
            WriteKind::ALL.to_vec()
        );

        changed.allowed_writes = vec![WriteKind::Delete, WriteKind::Insert, WriteKind::Insert];
        update(&db, user_id, &changed).unwrap();
        assert_eq!(
            load(&db, user_id).unwrap().allowed_writes,
            vec![WriteKind::Insert, WriteKind::Delete]
        );
    }

    #[test]
    fn default_row_limit_is_1000() {
        let db = store::open_in_memory();
        let user_id = store::ensure_user(&db, "a@example.com").unwrap();
        assert_eq!(load(&db, user_id).unwrap().row_limit, 1_000);
    }

    #[test]
    fn update_accepts_only_the_choices() {
        let db = store::open_in_memory();
        let user_id = store::ensure_user(&db, "a@example.com").unwrap();

        let settings = update(&db, user_id, &update_of(5_000)).unwrap();
        assert_eq!(settings.row_limit, 5_000);

        assert!(matches!(
            update(&db, user_id, &update_of(1_234)),
            Err(SettingsError::InvalidRowLimit(1_234))
        ));
        assert_eq!(load(&db, user_id).unwrap().row_limit, 5_000);
    }

    #[test]
    fn read_only_sessions_are_on_by_default() {
        let db = store::open_in_memory();
        let user_id = store::ensure_user(&db, "a@example.com").unwrap();
        assert!(load(&db, user_id).unwrap().read_only_session);

        let mut changed = update_of(1_000);
        changed.read_only_session = false;
        assert!(!update(&db, user_id, &changed).unwrap().read_only_session);
        assert!(!load(&db, user_id).unwrap().read_only_session);
    }

    #[test]
    fn retentions_are_30_days_by_default_and_stay_in_their_bounds() {
        let db = store::open_in_memory();
        let user_id = store::ensure_user(&db, "a@example.com").unwrap();
        let settings = load(&db, user_id).unwrap();
        assert_eq!(
            (
                settings.history_retention_days,
                settings.debug_retention_days
            ),
            (30, 30)
        );

        let mut changed = update_of(1_000);
        changed.history_retention_days = 3650;
        changed.debug_retention_days = 1;
        let settings = update(&db, user_id, &changed).unwrap();
        assert_eq!(
            (
                settings.history_retention_days,
                settings.debug_retention_days
            ),
            (3650, 1)
        );

        changed.debug_retention_days = 0;
        assert!(matches!(
            update(&db, user_id, &changed),
            Err(SettingsError::RetentionOutOfRange {
                field: Field::DebugRetentionDays
            })
        ));
        changed.debug_retention_days = 1;
        changed.history_retention_days = 3651;
        assert!(update(&db, user_id, &changed).is_err());
        assert_eq!(load(&db, user_id).unwrap().history_retention_days, 3650);
    }

    #[test]
    fn indent_size_is_4_by_default_and_stays_in_its_bounds() {
        let db = store::open_in_memory();
        let user_id = store::ensure_user(&db, "a@example.com").unwrap();
        assert_eq!(load(&db, user_id).unwrap().indent_size, 4);

        let mut changed = update_of(1_000);
        for size in [1, 2, 8] {
            changed.indent_size = size;
            assert_eq!(update(&db, user_id, &changed).unwrap().indent_size, size);
        }

        for size in [0, 9] {
            changed.indent_size = size;
            assert!(matches!(
                update(&db, user_id, &changed),
                Err(SettingsError::IndentSizeOutOfRange)
            ));
        }
        assert_eq!(load(&db, user_id).unwrap().indent_size, 8);
    }

    #[test]
    fn concat_max_bytes_is_1000_by_default_and_stays_in_its_bounds() {
        let db = store::open_in_memory();
        let user_id = store::ensure_user(&db, "a@example.com").unwrap();
        assert_eq!(load(&db, user_id).unwrap().concat_max_bytes, 1000);

        let mut changed = update_of(1_000);
        for bytes in [100, 4000, 1_048_576] {
            changed.concat_max_bytes = bytes;
            assert_eq!(
                update(&db, user_id, &changed).unwrap().concat_max_bytes,
                bytes
            );
        }

        for bytes in [99, 1_048_577] {
            changed.concat_max_bytes = bytes;
            assert!(matches!(
                update(&db, user_id, &changed),
                Err(SettingsError::ConcatMaxBytesOutOfRange)
            ));
        }
        assert_eq!(load(&db, user_id).unwrap().concat_max_bytes, 1_048_576);

        // The database refuses the same bounds.
        let refused = db.execute("UPDATE users SET concat_max_bytes = 99", []);
        assert!(refused.is_err());
    }

    #[test]
    fn pivot_refresh_ms_is_1000_by_default_and_stays_in_its_bounds() {
        let db = store::open_in_memory();
        let user_id = store::ensure_user(&db, "a@example.com").unwrap();
        assert_eq!(load(&db, user_id).unwrap().pivot_refresh_ms, 1000);

        let mut changed = update_of(1_000);
        for delay in [200, 2500, 60_000] {
            changed.pivot_refresh_ms = delay;
            assert_eq!(
                update(&db, user_id, &changed).unwrap().pivot_refresh_ms,
                delay
            );
        }

        for delay in [199, 60_001] {
            changed.pivot_refresh_ms = delay;
            assert!(matches!(
                update(&db, user_id, &changed),
                Err(SettingsError::PivotRefreshMsOutOfRange)
            ));
        }
        assert_eq!(load(&db, user_id).unwrap().pivot_refresh_ms, 60_000);

        // The database refuses the same bounds.
        let refused = db.execute("UPDATE users SET pivot_refresh_ms = 199", []);
        assert!(refused.is_err());
    }
}
