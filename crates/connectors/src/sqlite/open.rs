//! Opening of a SQLite connection: the local form of each database of the
//! settings (`plan`, on the async side, as a remote file is copied into the
//! cache first), then, on the thread of the connection, the main database
//! (`connect`), the attached ones and the statements of the target
//! (`prepare`).
//!
//! The connection never creates a file: it opens without
//! `SQLITE_OPEN_CREATE`, which its attachments inherit
//! (`docs/SQL-COMPARISON.md`, section 6). It names every database by a URI:
//! a file in the default file system of SQLite, which the URI names, as an
//! attachment would otherwise open in that of the main database (`memdb`
//! for a main database in memory); a remote copy read-only (`mode=ro`); a
//! database in memory in `memdb`. A database that cannot be attached is left out, its state
//! telling why; the main database is the only one whose failure fails the
//! opening.

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use prisme_dialects::sqlite_files::file_uri;
use rusqlite::{Connection, InterruptHandle, OpenFlags};

use super::failure::{cause, opening};
use super::statement::traced;
use crate::api::{
    ConnectorError, DatabaseState, DatabaseStatus, FileDatabase, FilesSettings, MAIN_SCHEMA,
};
use crate::failure::ConnectFailure;
use crate::location::{Local, Location, Remote, locations};
use crate::sql_log;

/// A database of the settings, in the form the library opens.
#[derive(Debug, Clone)]
pub(super) struct Planned {
    /// Schema and location of the database.
    pub(super) database: FileDatabase,
    /// Its local form, or why it cannot be reached.
    pub(super) local: Result<Local, ConnectFailure>,
}

/// Local forms of the databases of `files`: a file as it is (it must
/// exist), a remote file copied into the cache, a database in memory.
pub(super) async fn plan(files: &FilesSettings) -> Vec<Planned> {
    let mut planned = Vec::with_capacity(files.databases.len());
    for database in &files.databases {
        planned.push(plan_one(database, &files.remote).await);
    }
    planned
}

/// Local form of `database`, its remote file read within `remote`.
pub(super) async fn plan_one(database: &FileDatabase, remote: &Remote) -> Planned {
    Planned {
        database: database.clone(),
        local: local_form(&database.location, remote).await,
    }
}

/// Local form of the database at `location`.
async fn local_form(location: &Location, remote: &Remote) -> Result<Local, ConnectFailure> {
    let provider = locations::of(location.scheme);
    let unreachable = |error| ConnectFailure::Location { error };
    // A local file is checked first: the library would not tell a missing
    // file from one it cannot open.
    if provider.capabilities().local {
        let report = provider
            .probe(location, remote)
            .await
            .map_err(unreachable)?;
        if !report.exists {
            return Err(ConnectFailure::FileNotFound {
                location: location.input().to_string(),
            });
        }
    }
    provider
        .materialize(location, remote)
        .await
        .map_err(unreachable)
}

/// Flags of a connection: read-write, or read-only for a read-only session;
/// never `SQLITE_OPEN_CREATE`; URIs read; one thread at a time.
fn flags(read_only: bool) -> OpenFlags {
    let mode = if read_only {
        OpenFlags::SQLITE_OPEN_READ_ONLY
    } else {
        OpenFlags::SQLITE_OPEN_READ_WRITE
    };
    mode | OpenFlags::SQLITE_OPEN_URI | OpenFlags::SQLITE_OPEN_NO_MUTEX
}

/// Name of the resources of a connection outside any session: its own
/// databases in memory.
pub(super) fn private_key() -> String {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    format!("connection-{}", NEXT.fetch_add(1, Ordering::Relaxed))
}

/// URI the library opens for the database `schema` in the local form
/// `local`: a file in the default file system, read-only for a copy; a
/// database of the `memdb` file system for a database in memory, named by
/// `key` (the key of the session) and by its schema, whatever its case.
pub(super) fn file_name(local: &Local, schema: &str, key: &str) -> String {
    match local {
        Local::File { path, read_only } => file_uri(&path.to_string_lossy(), *read_only),
        Local::Memory => format!(
            "file:/prisme/{}/{}?vfs=memdb",
            encoded(key),
            encoded(&schema.to_lowercase())
        ),
    }
}

/// Text written into a path of a URI, every reserved character encoded.
fn encoded(text: &str) -> String {
    url::form_urlencoded::byte_serialize(text.as_bytes()).collect()
}

/// Opens the main database at the file name `name`.
///
/// # Errors
///
/// Fails if the library cannot open it.
pub(super) fn connect(name: &str, read_only: bool) -> rusqlite::Result<Connection> {
    Connection::open_with_flags(name, flags(read_only))
}

/// What the opening of a connection gives: the way to interrupt it, and
/// the state of each of its databases.
pub(super) struct Prepared {
    /// Interruption of the running statement, from another thread.
    pub(super) handle: InterruptHandle,
    /// State of each database, in the order of the settings.
    pub(super) databases: Vec<DatabaseState>,
}

/// Prepares the connection whose main database is `main`: attaches the
/// other databases of `planned`, then runs the statements of the target.
///
/// # Errors
///
/// Fails if the main database is not a database, or if a statement of the
/// target fails.
pub(super) fn prepare(
    connection: &Connection,
    main: &FileDatabase,
    planned: &[Planned],
    key: &str,
    init_statements: &[String],
) -> Result<Prepared, ConnectorError> {
    let main_state = state(connection, MAIN_SCHEMA)
        .map_err(|error| ConnectorError::Connect(opening(&main.location, &error)))?;
    let mut databases = vec![DatabaseState {
        schema: main.schema.clone(),
        location: main.location.clone(),
        status: main_state,
    }];
    for database in planned {
        databases.push(DatabaseState {
            schema: database.database.schema.clone(),
            location: database.database.location.clone(),
            status: attach(connection, database, key),
        });
    }
    for statement in init_statements {
        run_to_end(connection, statement).map_err(|error| ConnectorError::Init {
            statement: statement.clone(),
            cause: cause(&error),
        })?;
    }
    Ok(Prepared {
        handle: connection.get_interrupt_handle(),
        databases,
    })
}

/// Attaches `database` to `connection`, and tells its state: open, or why
/// not. A file that is not a database is detached again.
pub(super) fn attach(connection: &Connection, database: &Planned, key: &str) -> DatabaseStatus {
    let local = match &database.local {
        Ok(local) => local,
        Err(failure) => {
            return DatabaseStatus::Unavailable {
                failure: failure.clone(),
            };
        }
    };
    let schema = database.database.schema.as_str();
    let location = &database.database.location;
    let name = file_name(local, schema, key);
    let sql = "ATTACH ?1 AS ?2";
    let attached = traced(&sql_log::bound(sql, &[&name, schema]), || {
        connection.execute(sql, [name.as_str(), schema])
    });
    if let Err(error) = attached {
        return DatabaseStatus::Unavailable {
            failure: opening(location, &error),
        };
    }
    match state(connection, schema) {
        Ok(status) => status,
        Err(error) => {
            let sql = "DETACH ?1";
            // Detached as it came: a failure leaves it attached, unreadable.
            let _ = traced(&sql_log::bound(sql, &[schema]), || {
                connection.execute(sql, [schema])
            });
            DatabaseStatus::Unavailable {
                failure: opening(location, &error),
            }
        }
    }
}

/// Databases attached to `connection`, the main one first, in the order of
/// their attachment, `temp` left out, each with the file SQLite lists for
/// it: empty for a database in memory, and for any database of a connection
/// whose main database is in `memdb`, unless its `ATTACH` named a file
/// system (`docs/SQL-COMPARISON.md`, section 6).
pub(super) fn listed(connection: &Connection) -> rusqlite::Result<Vec<(String, String)>> {
    let sql = "SELECT name, file FROM pragma_database_list WHERE name <> 'temp' ORDER BY seq";
    traced(sql, || {
        let mut statement = connection.prepare(sql)?;
        let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        rows.collect()
    })
}

/// Detaches the database of the schema `schema`.
pub(super) fn detach(connection: &Connection, schema: &str) -> rusqlite::Result<()> {
    let sql = "DETACH ?1";
    traced(&sql_log::bound(sql, &[schema]), || {
        connection.execute(sql, [schema])
    })?;
    Ok(())
}

/// Copies the database of the schema `schema` into the file at `path`, which
/// exists, empty (`LocationProvider::create`): the file is removed again if
/// the copy fails.
pub(super) fn backup(connection: &Connection, schema: &str, path: &Path) -> rusqlite::Result<()> {
    let sql = format!("-- backup of {schema} into {}", path.display());
    let copied = traced(&sql, || {
        connection.backup(schema, path, None::<fn(rusqlite::backup::Progress)>)
    });
    if copied.is_err() {
        let _ = std::fs::remove_file(path);
    }
    copied
}

/// State of the open database `schema`: its size, its journal, and whether
/// it is read-only. Reading it reads the header of the file: a file that is
/// not a database fails here.
fn state(connection: &Connection, schema: &str) -> rusqlite::Result<DatabaseStatus> {
    let sql = "SELECT (SELECT page_count FROM pragma_page_count(?1)) \
               * (SELECT page_size FROM pragma_page_size(?1)), \
               (SELECT journal_mode FROM pragma_journal_mode(?1))";
    let (bytes, journal_mode) = traced(&sql_log::bound(sql, &[schema]), || {
        connection.query_row(sql, [schema], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })
    })?;
    Ok(DatabaseStatus::Open {
        bytes: u64::try_from(bytes).unwrap_or(0),
        journal_mode,
        read_only: connection.is_readonly(schema)?,
    })
}

/// Runs a statement of the target to its end, its rows dropped, and traces
/// it.
fn run_to_end(connection: &Connection, sql: &str) -> rusqlite::Result<()> {
    traced(sql, || {
        let mut statement = connection.prepare(sql)?;
        let mut rows = statement.raw_query();
        while rows.next()?.is_some() {}
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn names_the_file_of_each_local_form() {
        let file = |path: &str, read_only| {
            let local = Local::File {
                path: PathBuf::from(path),
                read_only,
            };
            file_name(&local, "main", "k")
        };
        if cfg!(windows) {
            assert_eq!(
                file(r"D:\data\a b#1%.db", false),
                "file:///D:/data/a%20b%231%25.db?vfs=win32"
            );
            assert_eq!(
                file(r"\\server\share\v.db", true),
                "file:////server/share/v.db?vfs=win32&mode=ro"
            );
        } else {
            assert_eq!(
                file("/data/a b#1%.db", false),
                "file:///data/a%20b%231%25.db?vfs=unix"
            );
            assert_eq!(file("/v.db", true), "file:///v.db?vfs=unix&mode=ro");
        }
        assert_eq!(
            file_name(&Local::Memory, "Other db", "s/1"),
            "file:/prisme/s%2F1/other+db?vfs=memdb"
        );
    }

    #[test]
    fn never_creates_a_file() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("missing.db");
        assert!(connect(&path.to_string_lossy(), false).is_err());
        assert!(!path.exists());
    }

    #[test]
    fn shares_a_database_in_memory_by_its_key() {
        let name = file_name(&Local::Memory, "main", &private_key());
        let first = connect(&name, false).unwrap();
        first.execute_batch("CREATE TABLE m (x)").unwrap();
        let second = connect(&name, false).unwrap();
        assert!(second.prepare("SELECT x FROM m").is_ok());
        let other = connect(&file_name(&Local::Memory, "main", &private_key()), false).unwrap();
        assert!(other.prepare("SELECT x FROM m").is_err());
    }
}
