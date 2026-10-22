//! Settings of the user: the default row limit, added to the queries that do
//! not write one (see `docs/V1.md`, section 2.3), the read-only mode of the
//! query sessions (`docs/V1.md`, section 2.4), the retention of the
//! console's history and debug log, and the indentation of the SQL editor.
//!
//! The settings are stored in the `users` table (see `migrations/0003_users_row_limit.sql`,
//! `migrations/0011_users_read_only_session.sql`,
//! `migrations/0014_users_console_retention.sql` and
//! `migrations/0016_users_indent_size.sql`).
//! Synchronous functions (`rusqlite`), called from blocking tasks.

use crate::error_detail::{ErrorDetail, Field};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Row limits offered to the user. The only place to change to modify the list.
pub const ROW_LIMIT_CHOICES: [u32; 5] = [1_000, 2_000, 5_000, 10_000, 50_000];

/// Bounds of a retention of the console, in days: the database checks the same.
pub const RETENTION_DAYS: std::ops::RangeInclusive<u16> = 1..=3650;

/// Bounds of the indentation of the SQL editor, in spaces: the database checks the same.
pub const INDENT_SIZE: std::ops::RangeInclusive<u8> = 1..=8;

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
    /// Days the queries of the console's history are kept, 30 by default.
    pub history_retention_days: u16,
    /// Days the lines of the console's debug log are kept, 30 by default.
    pub debug_retention_days: u16,
    /// Spaces of an indentation level in the SQL editor, 4 by default.
    pub indent_size: u8,
}

/// Change of settings requested by the user.
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct SettingsUpdate {
    /// New default row limit, among `ROW_LIMIT_CHOICES`.
    pub row_limit: u32,
    /// New read-only mode of the query sessions.
    pub read_only_session: bool,
    /// New retention of the history, in days (`RETENTION_DAYS`).
    pub history_retention_days: u16,
    /// New retention of the debug log, in days (`RETENTION_DAYS`).
    pub debug_retention_days: u16,
    /// New indentation of the SQL editor, in spaces (`INDENT_SIZE`).
    pub indent_size: u8,
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

/// Checks the row limit, the retentions and the indentation of an update.
///
/// # Errors
///
/// Returns `SettingsError::InvalidRowLimit` if the limit is not among
/// `ROW_LIMIT_CHOICES`, `SettingsError::RetentionOutOfRange` if a retention
/// is out of `RETENTION_DAYS`, `SettingsError::IndentSizeOutOfRange` if the
/// indentation is out of `INDENT_SIZE`.
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
    Ok(())
}

/// Reads the settings of user `user_id`.
pub fn load(db: &Connection, user_id: i64) -> Result<Settings, SettingsError> {
    let settings = db.query_row(
        "SELECT row_limit, read_only_session, history_retention_days, debug_retention_days,
                indent_size
         FROM users WHERE id = ?1",
        params![user_id],
        |row| {
            Ok(Settings {
                row_limit: row.get(0)?,
                row_limit_choices: &ROW_LIMIT_CHOICES,
                read_only_session: row.get(1)?,
                history_retention_days: row.get(2)?,
                debug_retention_days: row.get(3)?,
                indent_size: row.get(4)?,
            })
        },
    )?;
    Ok(settings)
}

/// Changes the settings of user `user_id` and returns the updated settings.
pub fn update(
    db: &Connection,
    user_id: i64,
    update: &SettingsUpdate,
) -> Result<Settings, SettingsError> {
    check_update(update)?;
    db.execute(
        "UPDATE users SET row_limit = ?1, read_only_session = ?2,
                history_retention_days = ?3, debug_retention_days = ?4, indent_size = ?5
         WHERE id = ?6",
        params![
            update.row_limit,
            update.read_only_session,
            update.history_retention_days,
            update.debug_retention_days,
            update.indent_size,
            user_id
        ],
    )?;
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
            history_retention_days: 30,
            debug_retention_days: 30,
            indent_size: 4,
        }
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
}
