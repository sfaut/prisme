//! Internal database of Prisme: a SQLite file in WAL mode in the data folder.
//!
//! It keeps the users and the connections, then the notebooks. The schema
//! evolves through numbered SQL migrations (`migrations/` folder), applied on
//! opening. `rusqlite` is synchronous: outside startup, accesses go through
//! dedicated blocking tasks (`spawn_blocking`).

use std::path::Path;
use std::sync::LazyLock;

use rusqlite::{Connection, params};
use rusqlite_migration::{M, Migrations};

/// Name of the internal database file in the data folder.
pub const FILE_NAME: &str = "prisme.db";

/// Migrations of the schema, in order. A published migration is never
/// modified again: any change goes through a new migration added at the end of
/// the list.
static MIGRATIONS: LazyLock<Migrations<'static>> = LazyLock::new(|| {
    Migrations::new(vec![
        M::up(include_str!("../migrations/0001_users.sql")),
        M::up(include_str!("../migrations/0002_connections.sql")),
        M::up(include_str!("../migrations/0003_users_row_limit.sql")),
        M::up(include_str!("../migrations/0004_connections_init_sql.sql")),
        M::up(include_str!("../migrations/0005_editors.sql")),
        M::up(include_str!(
            "../migrations/0006_connections_session_timeout.sql"
        )),
        M::up(include_str!(
            "../migrations/0007_connections_server_flavor.sql"
        )),
        M::up(include_str!(
            "../migrations/0008_connections_show_system_schemas.sql"
        )),
        M::up(include_str!("../migrations/0009_server_settings.sql")),
    ])
});

/// Errors accessing the internal database.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// SQLite error.
    #[error("internal database: {0}")]
    Sqlite(#[from] rusqlite::Error),

    /// Failure of a schema migration.
    #[error("internal database migration: {0}")]
    Migration(#[from] rusqlite_migration::Error),
}

/// Opens (or creates) the internal database at `path` and brings its schema up to date.
pub fn open(path: &Path) -> Result<Connection, StoreError> {
    let mut connection = Connection::open(path)?;
    // WAL: reads do not block writes, and conversely.
    connection.pragma_update(None, "journal_mode", "WAL")?;
    prepare(&mut connection)?;
    Ok(connection)
}

/// Enables foreign keys and applies the pending migrations.
fn prepare(connection: &mut Connection) -> Result<(), StoreError> {
    // SQLite ignores foreign keys until this setting is enabled, connection by connection.
    connection.pragma_update(None, "foreign_keys", "ON")?;
    MIGRATIONS.to_latest(connection)?;
    Ok(())
}

/// Creates the user `email` if it does not exist yet, and returns its id.
pub fn ensure_user(connection: &Connection, email: &str) -> Result<i64, StoreError> {
    connection.execute(
        "INSERT INTO users (email) VALUES (?1) ON CONFLICT (email) DO NOTHING",
        params![email],
    )?;
    let id = connection.query_row(
        "SELECT id FROM users WHERE email = ?1",
        params![email],
        |row| row.get(0),
    )?;
    Ok(id)
}

/// Opens an in-memory internal database, schema up to date, for the tests.
#[cfg(test)]
pub fn open_in_memory() -> Connection {
    let mut connection = Connection::open_in_memory().unwrap();
    prepare(&mut connection).unwrap();
    connection
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_are_valid() {
        MIGRATIONS.validate().unwrap();
    }

    #[test]
    fn ensure_user_is_idempotent() {
        let connection = open_in_memory();
        let first = ensure_user(&connection, "owner@example.com").unwrap();
        let second = ensure_user(&connection, "owner@example.com").unwrap();

        assert_eq!(first, second);
        let count: i64 = connection
            .query_row("SELECT COUNT(*) FROM users", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn rejects_too_long_email() {
        let connection = open_in_memory();
        let email = format!("{}@example.com", "a".repeat(250));
        assert!(ensure_user(&connection, &email).is_err());
    }

    #[test]
    fn file_store_uses_wal() {
        let dir = tempfile::tempdir().unwrap();
        let connection = open(&dir.path().join(FILE_NAME)).unwrap();
        let mode: String = connection
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .unwrap();
        assert_eq!(mode, "wal");
    }
}
