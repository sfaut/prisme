//! Settings of the user.
//!
//! For now: the default row limit, added to the queries that do not write one
//! (see `docs/V1.md`, section 2.3). Synchronous functions (`rusqlite`), called
//! from blocking tasks.

use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Row limits offered to the user. The only place to change to modify the list.
pub const ROW_LIMIT_CHOICES: [u32; 5] = [1_000, 2_000, 5_000, 10_000, 50_000];

/// Settings of a user, as returned by the API.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct Settings {
    /// Default row limit.
    pub row_limit: u32,
    /// Allowed limits, to build the interface's list of choices.
    #[ts(type = "Array<number>")]
    pub row_limit_choices: &'static [u32],
}

/// Change of settings requested by the user.
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct SettingsUpdate {
    /// New default row limit, among `ROW_LIMIT_CHOICES`.
    pub row_limit: u32,
}

/// Errors of settings management.
#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    /// Limit outside the list of choices.
    #[error("row_limit: {0} is not allowed, possible choices: {choices:?}", choices = ROW_LIMIT_CHOICES)]
    InvalidRowLimit(u32),

    /// Error of the internal database.
    #[error("internal database: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

/// Checks that a row limit is among the allowed choices.
pub fn check_row_limit(row_limit: u32) -> Result<u32, SettingsError> {
    if ROW_LIMIT_CHOICES.contains(&row_limit) {
        Ok(row_limit)
    } else {
        Err(SettingsError::InvalidRowLimit(row_limit))
    }
}

/// Reads the settings of user `user_id`.
pub fn load(db: &Connection, user_id: i64) -> Result<Settings, SettingsError> {
    let row_limit = db.query_row(
        "SELECT row_limit FROM users WHERE id = ?1",
        params![user_id],
        |row| row.get(0),
    )?;
    Ok(Settings {
        row_limit,
        row_limit_choices: &ROW_LIMIT_CHOICES,
    })
}

/// Changes the settings of user `user_id` and returns the updated settings.
pub fn update(
    db: &Connection,
    user_id: i64,
    update: &SettingsUpdate,
) -> Result<Settings, SettingsError> {
    let row_limit = check_row_limit(update.row_limit)?;
    db.execute(
        "UPDATE users SET row_limit = ?1 WHERE id = ?2",
        params![row_limit, user_id],
    )?;
    load(db, user_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store;

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

        let settings = update(&db, user_id, &SettingsUpdate { row_limit: 5_000 }).unwrap();
        assert_eq!(settings.row_limit, 5_000);

        assert!(matches!(
            update(&db, user_id, &SettingsUpdate { row_limit: 1_234 }),
            Err(SettingsError::InvalidRowLimit(1_234))
        ));
        assert_eq!(load(&db, user_id).unwrap().row_limit, 5_000);
    }
}
