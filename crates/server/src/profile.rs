//! Profile of the user: first and last names, the email address, which
//! identifies the user and is changed by `prisme --setup` or `--user-email`,
//! the read-only mode of the query sessions (`docs/V1.md`, section 2.4) and
//! the retention of the console's history and debug log.
//!
//! The settings are stored in the `users` table (see `migrations/0010_users_names.sql`,
//! `migrations/0011_users_read_only_session.sql` and
//! `migrations/0014_users_console_retention.sql`).
//! Synchronous functions (`rusqlite`), called from blocking tasks.

use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Longest first or last name, in characters: the database checks the same limit (`length()` counts characters).
pub const NAME_MAX_CHARS: usize = 100;

/// Bounds of a retention of the console, in days: the database checks the same.
pub const RETENTION_DAYS: std::ops::RangeInclusive<u16> = 1..=3650;

/// Profile of a user, as returned by the API.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct Profile {
    /// Email address, read only here.
    pub email: String,
    /// First name, empty until the user fills it in.
    pub first_name: String,
    /// Last name, empty until the user fills it in.
    pub last_name: String,
    /// Are the query sessions in read-only mode? On by default: the database
    /// then refuses the writes too.
    pub read_only_session: bool,
    /// Days the queries of the console's history are kept, 30 by default.
    pub history_retention_days: u16,
    /// Days the lines of the console's debug log are kept, 30 by default.
    pub debug_retention_days: u16,
}

/// Change of the profile requested by the user.
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct ProfileUpdate {
    /// New first name, trimmed, at most `NAME_MAX_CHARS` characters.
    pub first_name: String,
    /// New last name, trimmed, at most `NAME_MAX_CHARS` characters.
    pub last_name: String,
    /// New read-only mode of the query sessions.
    pub read_only_session: bool,
    /// New retention of the history, in days (`RETENTION_DAYS`).
    pub history_retention_days: u16,
    /// New retention of the debug log, in days (`RETENTION_DAYS`).
    pub debug_retention_days: u16,
}

/// Errors of profile management.
#[derive(Debug, thiserror::Error)]
pub enum ProfileError {
    /// Name too long.
    #[error("{field}: at most {max} characters", max = NAME_MAX_CHARS)]
    NameTooLong {
        /// Field that is too long.
        field: &'static str,
    },

    /// Retention out of its bounds.
    #[error("{field}: between {min} and {max} days", min = RETENTION_DAYS.start(), max = RETENTION_DAYS.end())]
    RetentionOutOfRange {
        /// Field out of its bounds.
        field: &'static str,
    },

    /// Error of the internal database.
    #[error("internal database: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

/// Checks the names and the retentions of an update, and returns the names
/// trimmed.
///
/// # Errors
///
/// Returns `ProfileError::NameTooLong` if a name exceeds `NAME_MAX_CHARS`
/// characters, `ProfileError::RetentionOutOfRange` if a retention is out of
/// `RETENTION_DAYS`.
pub fn check_update(update: &ProfileUpdate) -> Result<(String, String), ProfileError> {
    for (field, days) in [
        ("history_retention_days", update.history_retention_days),
        ("debug_retention_days", update.debug_retention_days),
    ] {
        if !RETENTION_DAYS.contains(&days) {
            return Err(ProfileError::RetentionOutOfRange { field });
        }
    }
    let first_name = update.first_name.trim();
    let last_name = update.last_name.trim();
    if first_name.chars().count() > NAME_MAX_CHARS {
        return Err(ProfileError::NameTooLong {
            field: "first_name",
        });
    }
    if last_name.chars().count() > NAME_MAX_CHARS {
        return Err(ProfileError::NameTooLong { field: "last_name" });
    }
    Ok((first_name.to_string(), last_name.to_string()))
}

/// Reads the profile of user `user_id`.
pub fn load(db: &Connection, user_id: i64) -> Result<Profile, ProfileError> {
    let profile = db.query_row(
        "SELECT email, first_name, last_name, read_only_session,
                history_retention_days, debug_retention_days
         FROM users WHERE id = ?1",
        params![user_id],
        |row| {
            Ok(Profile {
                email: row.get(0)?,
                first_name: row.get(1)?,
                last_name: row.get(2)?,
                read_only_session: row.get(3)?,
                history_retention_days: row.get(4)?,
                debug_retention_days: row.get(5)?,
            })
        },
    )?;
    Ok(profile)
}

/// Changes the profile of user `user_id` and returns the updated profile.
pub fn update(
    db: &Connection,
    user_id: i64,
    update: &ProfileUpdate,
) -> Result<Profile, ProfileError> {
    let (first_name, last_name) = check_update(update)?;
    db.execute(
        "UPDATE users SET first_name = ?1, last_name = ?2, read_only_session = ?3,
                history_retention_days = ?4, debug_retention_days = ?5
         WHERE id = ?6",
        params![
            first_name,
            last_name,
            update.read_only_session,
            update.history_retention_days,
            update.debug_retention_days,
            user_id
        ],
    )?;
    load(db, user_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store;

    /// Update with the given names, read-only mode unchanged from `read_only_session`.
    fn update_of(first_name: &str, last_name: &str, read_only_session: bool) -> ProfileUpdate {
        ProfileUpdate {
            first_name: first_name.to_string(),
            last_name: last_name.to_string(),
            read_only_session,
            history_retention_days: 30,
            debug_retention_days: 30,
        }
    }

    #[test]
    fn names_are_empty_until_filled() {
        let db = store::open_in_memory();
        let user_id = store::ensure_user(&db, "a@example.com").unwrap();
        let profile = load(&db, user_id).unwrap();
        assert_eq!(profile.email, "a@example.com");
        assert_eq!(profile.first_name, "");
        assert_eq!(profile.last_name, "");
    }

    #[test]
    fn read_only_sessions_are_on_by_default() {
        let db = store::open_in_memory();
        let user_id = store::ensure_user(&db, "a@example.com").unwrap();
        assert!(load(&db, user_id).unwrap().read_only_session);
    }

    #[test]
    fn update_trims_and_stores_the_names_and_the_read_only_mode() {
        let db = store::open_in_memory();
        let user_id = store::ensure_user(&db, "a@example.com").unwrap();

        let profile = update(&db, user_id, &update_of(" Sébastien ", "Faut", false)).unwrap();
        assert_eq!(profile.first_name, "Sébastien");
        assert_eq!(profile.last_name, "Faut");
        assert!(!profile.read_only_session);
        assert!(!load(&db, user_id).unwrap().read_only_session);
    }

    #[test]
    fn retentions_are_30_days_by_default_and_stay_in_their_bounds() {
        let db = store::open_in_memory();
        let user_id = store::ensure_user(&db, "a@example.com").unwrap();
        let profile = load(&db, user_id).unwrap();
        assert_eq!(
            (profile.history_retention_days, profile.debug_retention_days),
            (30, 30)
        );

        let mut changed = update_of("", "", true);
        changed.history_retention_days = 3650;
        changed.debug_retention_days = 1;
        let profile = update(&db, user_id, &changed).unwrap();
        assert_eq!(
            (profile.history_retention_days, profile.debug_retention_days),
            (3650, 1)
        );

        changed.debug_retention_days = 0;
        assert!(matches!(
            update(&db, user_id, &changed),
            Err(ProfileError::RetentionOutOfRange {
                field: "debug_retention_days"
            })
        ));
        changed.debug_retention_days = 1;
        changed.history_retention_days = 3651;
        assert!(update(&db, user_id, &changed).is_err());
        assert_eq!(load(&db, user_id).unwrap().history_retention_days, 3650);
    }

    #[test]
    fn update_refuses_names_over_the_limit() {
        let db = store::open_in_memory();
        let user_id = store::ensure_user(&db, "a@example.com").unwrap();

        let too_long = "é".repeat(NAME_MAX_CHARS + 1);
        assert!(matches!(
            update(&db, user_id, &update_of(&too_long, "", true)),
            Err(ProfileError::NameTooLong {
                field: "first_name"
            })
        ));
        assert_eq!(load(&db, user_id).unwrap().first_name, "");
    }
}
