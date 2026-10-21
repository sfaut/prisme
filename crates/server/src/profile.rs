//! Profile of the user: first and last names, and the email address, which
//! identifies the user and is changed by `prisme --setup` or `--user-email`.
//! The preferences of the user are its settings (`settings.rs`).
//!
//! The names are stored in the `users` table (see `migrations/0010_users_names.sql`).
//! Synchronous functions (`rusqlite`), called from blocking tasks.

use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Longest first or last name, in characters: the database checks the same limit (`length()` counts characters).
pub const NAME_MAX_CHARS: usize = 100;

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
}

/// Change of the profile requested by the user.
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct ProfileUpdate {
    /// New first name, trimmed, at most `NAME_MAX_CHARS` characters.
    pub first_name: String,
    /// New last name, trimmed, at most `NAME_MAX_CHARS` characters.
    pub last_name: String,
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

    /// Error of the internal database.
    #[error("internal database: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

/// Checks the names of an update, and returns them trimmed.
///
/// # Errors
///
/// Returns `ProfileError::NameTooLong` if a name exceeds `NAME_MAX_CHARS`
/// characters.
pub fn check_update(update: &ProfileUpdate) -> Result<(String, String), ProfileError> {
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
        "SELECT email, first_name, last_name FROM users WHERE id = ?1",
        params![user_id],
        |row| {
            Ok(Profile {
                email: row.get(0)?,
                first_name: row.get(1)?,
                last_name: row.get(2)?,
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
        "UPDATE users SET first_name = ?1, last_name = ?2 WHERE id = ?3",
        params![first_name, last_name, user_id],
    )?;
    load(db, user_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store;

    /// Update with the given names.
    fn update_of(first_name: &str, last_name: &str) -> ProfileUpdate {
        ProfileUpdate {
            first_name: first_name.to_string(),
            last_name: last_name.to_string(),
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
    fn update_trims_and_stores_the_names() {
        let db = store::open_in_memory();
        let user_id = store::ensure_user(&db, "a@example.com").unwrap();

        let profile = update(&db, user_id, &update_of(" Sébastien ", "Faut")).unwrap();
        assert_eq!(profile.first_name, "Sébastien");
        assert_eq!(profile.last_name, "Faut");
    }

    #[test]
    fn update_refuses_names_over_the_limit() {
        let db = store::open_in_memory();
        let user_id = store::ensure_user(&db, "a@example.com").unwrap();

        let too_long = "é".repeat(NAME_MAX_CHARS + 1);
        assert!(matches!(
            update(&db, user_id, &update_of(&too_long, "")),
            Err(ProfileError::NameTooLong {
                field: "first_name"
            })
        ));
        assert_eq!(load(&db, user_id).unwrap().first_name, "");
    }
}
