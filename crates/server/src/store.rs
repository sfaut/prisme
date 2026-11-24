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
        M::up(include_str!("../migrations/0010_users_names.sql")),
        M::up(include_str!(
            "../migrations/0011_users_read_only_session.sql"
        )),
        M::up(include_str!("../migrations/0012_query_history.sql")),
        M::up(include_str!("../migrations/0013_debug_log.sql")),
        M::up(include_str!(
            "../migrations/0014_users_console_retention.sql"
        )),
        M::up(include_str!(
            "../migrations/0015_connections_server_version.sql"
        )),
        M::up(include_str!("../migrations/0016_users_indent_size.sql")),
        M::up(include_str!("../migrations/0017_user_allowed_writes.sql")),
        M::up(include_str!(
            "../migrations/0018_query_history_run_exact.sql"
        )),
        M::up(include_str!(
            "../migrations/0019_user_allowed_writes_temporary.sql"
        )),
        M::up(include_str!(
            "../migrations/0020_query_history_navigator.sql"
        )),
        M::up(include_str!("../migrations/0021_exports.sql")),
        M::up(include_str!("../migrations/0022_query_history_export.sql")),
        M::up(include_str!("../migrations/0023_result_tabs_pivot.sql")),
        M::up(include_str!("../migrations/0024_result_chunks.sql")),
        M::up(include_str!(
            "../migrations/0025_users_concat_max_bytes.sql"
        )),
        M::up(include_str!(
            "../migrations/0026_users_pivot_refresh_ms.sql"
        )),
        M::up(include_str!(
            "../migrations/0027_query_history_interrupted.sql"
        )),
        M::up(include_str!("../migrations/0028_console_editor.sql")),
        M::up(include_str!("../migrations/0029_result_tabs_outcome.sql")),
        M::up(include_str!(
            "../migrations/0030_query_history_diagnostics.sql"
        )),
        M::up(include_str!("../migrations/0031_editors_navigator.sql")),
        M::up(include_str!("../migrations/0032_editors_display.sql")),
        M::up(include_str!("../migrations/0033_users_display.sql")),
        M::up(include_str!("../migrations/0034_result_tabs_grid.sql")),
        M::up(include_str!("../migrations/0035_editors_database.sql")),
        M::up(include_str!(
            "../migrations/0036_connections_connected_at.sql"
        )),
        M::up(include_str!("../migrations/0037_connections_by_family.sql")).foreign_key_check(),
        M::up(include_str!("../migrations/0038_write_kinds.sql")).foreign_key_check(),
        M::up(include_str!(
            "../migrations/0039_users_remote_locations.sql"
        )),
        M::up(include_str!("../migrations/0040_write_kinds_sqlite.sql")),
        M::up(include_str!("../migrations/0041_engine_sqlite.sql")),
        M::up(include_str!(
            "../migrations/0042_query_history_databases.sql"
        )),
        M::up(include_str!(
            "../migrations/0043_connection_file_options.sql"
        )),
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

/// Applies the pending migrations, then enables foreign keys.
fn prepare(connection: &mut Connection) -> Result<(), StoreError> {
    // A migration that rebuilds a table needs the foreign keys off
    // (https://sqlite.org/lang_altertable.html#otheralter), and the setting
    // does nothing inside the transaction of the migrations: it is set
    // before. Such a migration checks the foreign keys before its commit
    // (`M::foreign_key_check`).
    connection.pragma_update(None, "foreign_keys", "OFF")?;
    MIGRATIONS.to_latest(connection)?;
    // SQLite ignores foreign keys until this setting is enabled, connection by connection.
    connection.pragma_update(None, "foreign_keys", "ON")?;
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
    use prisme_connectors::api::{Engine, Family};
    use prisme_connectors::flavor::Flavor;
    use prisme_connectors::location::Scheme;
    use prisme_dialects::classify::WriteKind;

    use super::*;

    /// In-memory internal database with the schema of migration `version`,
    /// opened as `prepare` opens it (foreign keys off during the migrations).
    fn at_version(version: usize) -> Connection {
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .pragma_update(None, "foreign_keys", "OFF")
            .unwrap();
        MIGRATIONS.to_version(&mut connection, version).unwrap();
        connection
    }

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
    fn gives_the_rights_on_the_tables_to_the_temporary_tables() {
        // Schema before the switches of the temporary tables (migration 0019).
        let mut connection = at_version(18);
        let user = ensure_user(&connection, "owner@example.com").unwrap();
        connection
            .execute(
                "INSERT INTO user_allowed_writes (user_id, kind) \
                 VALUES (?1, 'delete'), (?1, 'drop_table'), (?1, 'create_view')",
                params![user],
            )
            .unwrap();

        prepare(&mut connection).unwrap();
        let mut statement = connection
            .prepare("SELECT kind FROM user_allowed_writes WHERE user_id = ?1 ORDER BY kind")
            .unwrap();
        let kinds: Vec<String> = statement
            .query_map(params![user], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(
            kinds,
            [
                "create_view",
                "delete",
                "drop_table",
                "drop_temporary_table"
            ]
        );
    }

    #[test]
    fn renames_the_explorer_action_to_navigator() {
        // Schema before the object navigator (migration 0020).
        let mut connection = at_version(19);
        let user = ensure_user(&connection, "owner@example.com").unwrap();
        connection
            .execute(
                "INSERT INTO query_history (user_id, started_at, action, sql) \
                 VALUES (?1, '2026-10-23T10:00:00Z', 'explorer', 'SHOW DATABASES'), \
                        (?1, '2026-10-23T10:00:01Z', 'run', 'SELECT 1')",
                params![user],
            )
            .unwrap();

        prepare(&mut connection).unwrap();
        let mut statement = connection
            .prepare("SELECT action, sql FROM query_history ORDER BY id")
            .unwrap();
        let rows: Vec<(String, String)> = statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(
            rows,
            [
                ("navigator".to_owned(), "SHOW DATABASES".to_owned()),
                ("run".to_owned(), "SELECT 1".to_owned())
            ]
        );
    }

    #[test]
    fn cuts_the_saved_rows_into_chunks() {
        // Schema before the chunks of the rows (migration 0024).
        let mut connection = at_version(23);
        let user = ensure_user(&connection, "owner@example.com").unwrap();
        connection
            .execute("INSERT INTO editors (user_id) VALUES (?1)", params![user])
            .unwrap();
        let editor = connection.last_insert_rowid();
        let rows: Vec<Vec<Option<String>>> = (0..25_001)
            .map(|row| vec![Some(row.to_string()), None])
            .collect();
        connection
            .execute(
                "INSERT INTO result_tabs (editor_id, query, sql, rows) VALUES (?1, 0, 'SELECT 1', ?2)",
                params![editor, serde_json::to_string(&rows).unwrap()],
            )
            .unwrap();
        let tab = connection.last_insert_rowid();

        prepare(&mut connection).unwrap();
        let mut statement = connection
            .prepare(
                "SELECT run, position, rows FROM result_chunks WHERE tab_id = ?1 ORDER BY position",
            )
            .unwrap();
        let chunks: Vec<(i64, i64, String)> = statement
            .query_map(params![tab], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(
            chunks
                .iter()
                .map(|(run, position, _)| (*run, *position))
                .collect::<Vec<_>>(),
            [(0, 0), (0, 1), (0, 2)]
        );
        let read: Vec<Vec<Option<String>>> = chunks
            .iter()
            .flat_map(|(_, _, text)| {
                serde_json::from_str::<Vec<Vec<Option<String>>>>(text).unwrap()
            })
            .collect();
        assert_eq!(read, rows);
    }

    #[test]
    fn marks_the_queries_stopped_before_as_interrupted() {
        // Schema before the stops of the history (migration 0027).
        let mut connection = at_version(26);
        let user = ensure_user(&connection, "owner@example.com").unwrap();
        connection
            .execute(
                "INSERT INTO query_history (user_id, started_at, action, sql, error) VALUES                  (?1, '2026-11-16T22:44:54Z', 'run_all', 'SELECT SLEEP(20)', 'query interrupted by the user'),                  (?1, '2026-11-16T22:44:55Z', 'run', 'SELECT * FROM missing', 'Unknown table'),                  (?1, '2026-11-16T22:44:56Z', 'run', 'SELECT 1', '')",
                params![user],
            )
            .unwrap();

        prepare(&mut connection).unwrap();
        let mut statement = connection
            .prepare("SELECT interrupted FROM query_history ORDER BY id")
            .unwrap();
        let interrupted: Vec<bool> = statement
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(interrupted, [true, false, false]);
    }

    #[test]
    fn moves_the_affected_rows_into_the_outcome() {
        // Schema before the outcome of the saved results (migration 0029).
        let mut connection = at_version(28);
        let user = ensure_user(&connection, "owner@example.com").unwrap();
        connection
            .execute("INSERT INTO editors (user_id) VALUES (?1)", params![user])
            .unwrap();
        let editor = connection.last_insert_rowid();
        connection
            .execute(
                "INSERT INTO result_tabs (editor_id, query, sql, result) VALUES \
                 (?1, 0, 'DELETE FROM t', '{\"done\":true,\"affected_rows\":3}'), \
                 (?1, 1, 'SELECT 1', '{}')",
                params![editor],
            )
            .unwrap();

        prepare(&mut connection).unwrap();
        let mut statement = connection
            .prepare("SELECT result FROM result_tabs ORDER BY query")
            .unwrap();
        let results: Vec<String> = statement
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(results, [r#"{"done":true,"outcome":{"affected":3}}"#, "{}"]);
    }

    #[test]
    fn keeps_the_history_when_adding_the_diagnostics() {
        // Schema before the diagnostics action (migration 0030).
        let mut connection = at_version(29);
        let user = ensure_user(&connection, "owner@example.com").unwrap();
        connection
            .execute(
                "INSERT INTO query_history (user_id, started_at, action, sql, affected_rows, \
                 interrupted, editor_id) \
                 VALUES (?1, '2026-11-21T10:00:00Z', 'run', 'DELETE FROM t', 3, 1, 7)",
                params![user],
            )
            .unwrap();

        prepare(&mut connection).unwrap();
        let row: (String, i64, bool, i64) = connection
            .query_row(
                "SELECT action, affected_rows, interrupted, editor_id FROM query_history",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(row, ("run".to_owned(), 3, true, 7));
        connection
            .execute(
                "INSERT INTO query_history (user_id, started_at, action, sql) \
                 VALUES (?1, '2026-11-21T10:00:01Z', 'diagnostics', 'SHOW WARNINGS')",
                params![user],
            )
            .unwrap();
    }

    #[test]
    fn moves_the_navigator_of_the_editors_into_their_display() {
        // Schema with the navigator of the editors (migration 0031).
        let mut connection = at_version(31);
        let user = ensure_user(&connection, "owner@example.com").unwrap();
        connection
            .execute(
                "INSERT INTO editors (user_id, navigator) VALUES                  (?1, '{\"open\":false,\"width\":320.5,\"expanded\":[\"sources\",\"source:1\"],                 \"scroll_top\":48,\"tables_by_rows\":[\"folder:tables:1/shop\"]}'),                  (?1, '{\"open\":true,\"width\":null,\"expanded\":[],\"scroll_top\":0}'),                  (?1, '')",
                params![user],
            )
            .unwrap();

        prepare(&mut connection).unwrap();
        let mut statement = connection
            .prepare("SELECT display FROM editors ORDER BY id")
            .unwrap();
        let displays: Vec<String> = statement
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        let read = |json: &str| -> crate::editor_display::EditorDisplay {
            serde_json::from_str(json).unwrap()
        };
        let first = read(&displays[0]);
        assert!(!first.panels.navigator.open);
        assert_eq!(first.panels.navigator.size, Some(320.5));
        assert!(first.panels.editor.open && first.panels.console.open);
        assert!(!first.panels.explorer.open && !first.panels.results.open);
        assert_eq!(first.navigator.expanded, ["sources", "source:1"]);
        assert_eq!(first.navigator.scroll_top, 48.0);
        assert_eq!(first.navigator.tables_by_rows, ["folder:tables:1/shop"]);
        let second = read(&displays[1]);
        assert!(second.panels.navigator.open);
        assert_eq!(second.panels.navigator.size, None);
        assert!(second.navigator.tables_by_rows.is_empty());
        assert_eq!(displays[2], "");
    }

    #[test]
    fn splits_the_connections_by_family() {
        // Schema before the connections by family (migration 0037).
        let mut connection = at_version(36);
        let user = ensure_user(&connection, "owner@example.com").unwrap();
        connection
            .execute(
                "INSERT INTO connections (id, user_id, name, engine, host, port, username, \
                 password_secret, database_name, init_sql, server_flavor, server_version, \
                 connected_at) VALUES \
                 (4, ?1, 'prod', 'mysql', 'db.local', 3307, 'app', x'0102', 'shop', \
                  'SET NAMES utf8mb4', 'mariadb', '10.11.11-MariaDB', '2026-11-20T10:00:00.000Z'), \
                 (9, ?1, 'old', 'mysql', 'localhost', 3306, 'root', x'03', '', '', \
                  'mysql', '8.0.40', '')",
                params![user],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO editors (user_id, connection_id) VALUES (?1, 4), (?1, 9)",
                params![user],
            )
            .unwrap();

        prepare(&mut connection).unwrap();
        let servers: Vec<(i64, String, i64, String, Vec<u8>, String)> = connection
            .prepare(
                "SELECT connection_id, host, port, username, password_secret, database_name \
                 FROM connection_servers ORDER BY connection_id",
            )
            .unwrap()
            .query_map([], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            })
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(
            servers,
            [
                (
                    4,
                    "db.local".to_owned(),
                    3307,
                    "app".to_owned(),
                    vec![1, 2],
                    "shop".to_owned()
                ),
                (
                    9,
                    "localhost".to_owned(),
                    3306,
                    "root".to_owned(),
                    vec![3],
                    String::new()
                )
            ]
        );
        let init_sql: String = connection
            .query_row("SELECT init_sql FROM connections WHERE id = 4", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(init_sql, "SET NAMES utf8mb4");
        // Only the connection whose probe was complete keeps it.
        let probes: Vec<(i64, String, String, String)> = connection
            .prepare("SELECT connection_id, flavor, version, connected_at FROM connection_probes")
            .unwrap()
            .query_map([], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(
            probes,
            [(
                4,
                "mariadb".to_owned(),
                "10.11.11-MariaDB".to_owned(),
                "2026-11-20T10:00:00.000Z".to_owned()
            )]
        );
        let editors: Vec<i64> = connection
            .prepare("SELECT connection_id FROM editors ORDER BY id")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(editors, [4, 9]);
        // The foreign keys hold the editors after the rebuild.
        connection
            .execute("DELETE FROM connections WHERE id = 9", [])
            .unwrap();
        let orphan: Option<i64> = connection
            .query_row(
                "SELECT connection_id FROM editors WHERE id = 2",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(orphan, None);
    }

    #[test]
    fn reference_tables_list_the_keys_of_the_connectors() {
        let connection = open_in_memory();
        let keys = |sql: &str| -> Vec<String> {
            connection
                .prepare(sql)
                .unwrap()
                .query_map([], |row| row.get(0))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap()
        };
        let mut families: Vec<String> = Family::ALL.map(|f| f.key().to_owned()).to_vec();
        families.sort();
        assert_eq!(
            keys("SELECT key FROM setting_families ORDER BY key"),
            families
        );
        let mut engines: Vec<String> = Engine::ALL
            .map(|engine| format!("{}:{}", engine.key(), engine.family().key()))
            .to_vec();
        engines.sort();
        assert_eq!(
            keys("SELECT key || ':' || family FROM engines ORDER BY key"),
            engines
        );
        let mut flavors: Vec<String> = Flavor::ALL
            .map(|flavor| format!("{}:{}", flavor.key(), flavor.engine().key()))
            .to_vec();
        flavors.sort();
        assert_eq!(
            keys("SELECT key || ':' || engine FROM flavors ORDER BY key"),
            flavors
        );
        let mut schemes: Vec<String> = Scheme::ALL.map(|scheme| scheme.key().to_owned()).to_vec();
        schemes.sort();
        assert_eq!(keys("SELECT key FROM schemes ORDER BY key"), schemes);
    }

    #[test]
    fn write_kinds_list_the_kinds_of_the_dialects() {
        let connection = open_in_memory();
        let mut kinds: Vec<String> = connection
            .prepare("SELECT key FROM write_kinds ORDER BY key")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        let mut expected: Vec<&str> = WriteKind::ALL.map(WriteKind::as_str).to_vec();
        expected.sort_unstable();
        kinds.sort();
        assert_eq!(kinds, expected);
    }

    #[test]
    fn keeps_the_switches_when_the_kinds_become_a_table() {
        // Schema before the reference table of the kinds (migration 0038).
        let mut connection = at_version(37);
        let user = ensure_user(&connection, "owner@example.com").unwrap();
        connection
            .execute(
                "INSERT INTO user_allowed_writes (user_id, kind) VALUES (?1, 'delete'), (?1, 'drop_event')",
                params![user],
            )
            .unwrap();

        prepare(&mut connection).unwrap();
        let kinds: Vec<String> = connection
            .prepare("SELECT kind FROM user_allowed_writes ORDER BY kind")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(kinds, ["delete", "drop_event"]);
        assert!(
            connection
                .execute(
                    "INSERT INTO user_allowed_writes (user_id, kind) VALUES (?1, 'unknown')",
                    params![user],
                )
                .is_err()
        );
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
