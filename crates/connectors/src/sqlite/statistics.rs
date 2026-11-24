//! Statistics of the query planner of the files of a source, refreshed when
//! a query session opens, when its connection asks for it
//! (`FilesSettings::optimize`): `PRAGMA optimize=0x10002`, the form the
//! documentation of SQLite recommends at the opening of a connection that
//! lasts. It analyzes the tables that need it (never analyzed, or whose size
//! changed much), within a limit of rows it sets itself, and writes their
//! statistics into `sqlite_stat1`; the catalog reads the row counts there
//! (`docs/SQL-COMPARISON.md`, section 6).
//!
//! `PRAGMA optimize` writes, and fails on a connection opened read-only, as
//! the sessions are by default: each local file is refreshed on a
//! connection of its own, read-write, opened for that alone, beside the
//! session. Only the statistics are written there, never the data. The
//! copies of the remote databases and the databases in memory are left out.
//!
//! The refresh runs on a thread of its own, without delaying the session; a
//! failure (file read-only on the disk, locked) only shows in the history
//! of the queries, as the statement is traced there.

use std::time::Duration;

use prisme_dialects::sqlite_files::file_uri;
use rusqlite::{Connection, OpenFlags};

use super::statement::traced;
use crate::api::FileDatabase;
use crate::location::Scheme;

/// Statement that refreshes the statistics of a file: analyzes every table
/// that needs it (`0x10000`: all the tables, as a new connection has run no
/// query yet), within the limit of rows SQLite sets.
pub(super) const OPTIMIZE: &str = "PRAGMA optimize=0x10002";

/// Wait for a lock held by another connection on the file (a write of the
/// session) before giving up.
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

/// Refreshes the statistics of the local files of `databases` on a thread of
/// its own, in the context of the caller's traces (user, connection), and
/// returns at once.
pub(super) fn refresh(databases: Vec<FileDatabase>) {
    let span = tracing::Span::current();
    let spawned = std::thread::Builder::new()
        .name("prisme-sqlite-statistics".to_string())
        .spawn(move || span.in_scope(|| refresh_now(&databases)));
    if let Err(error) = spawned {
        tracing::warn!("the statistics of the files were not refreshed: {error}");
    }
}

/// Refreshes the statistics of each local file of `databases`, one after the
/// other, and returns how many were refreshed.
pub(super) fn refresh_now(databases: &[FileDatabase]) -> usize {
    databases
        .iter()
        .filter(|database| database.location.scheme == Scheme::File)
        .filter(|database| optimize(&database.location.address).is_ok())
        .count()
}

/// Refreshes the statistics of the file at the absolute path `path`, on a
/// read-write connection that never creates it.
///
/// # Errors
///
/// Fails if the file cannot be opened for writing, or if the statement
/// fails.
fn optimize(path: &str) -> rusqlite::Result<()> {
    let connection = Connection::open_with_flags(
        file_uri(path, false),
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_URI
            | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    connection.busy_timeout(BUSY_TIMEOUT)?;
    traced(OPTIMIZE, || connection.execute_batch(OPTIMIZE))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::location::Location;

    /// Database of schema `schema` at `location`.
    fn database(schema: &str, scheme: Scheme, address: &str) -> FileDatabase {
        FileDatabase {
            schema: schema.to_string(),
            location: Location {
                scheme,
                address: address.to_string(),
            },
        }
    }

    /// Rows of `sqlite_stat1` of the file at `path`, none without the table.
    fn stats(path: &str) -> Vec<(String, String)> {
        let connection = Connection::open(path).unwrap();
        let exists: bool = connection
            .query_row(
                "SELECT count(*) > 0 FROM sqlite_schema WHERE name = 'sqlite_stat1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        if !exists {
            return Vec::new();
        }
        connection
            .prepare("SELECT tbl, stat FROM sqlite_stat1 ORDER BY tbl")
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    }

    #[test]
    fn refreshes_the_statistics_of_the_local_files_only() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("shop.db");
        let path = path.to_string_lossy().into_owned();
        Connection::open(&path)
            .unwrap()
            .execute_batch(
                "CREATE TABLE t (a INTEGER PRIMARY KEY, b TEXT);
                 CREATE INDEX t_b ON t (b);
                 WITH RECURSIVE n (i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 2000)
                 INSERT INTO t (b) SELECT i % 10 FROM n;",
            )
            .unwrap();

        // A connection opened read-only, as a session is, cannot do it.
        let read_only = Connection::open_with_flags(
            file_uri(&path, true),
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
        )
        .unwrap();
        let error = read_only.execute_batch(OPTIMIZE).unwrap_err();
        assert!(error.to_string().contains("readonly"), "{error}");
        drop(read_only);
        assert!(stats(&path).is_empty());

        let missing = folder.path().join("missing.db");
        let databases = [
            database("main", Scheme::File, &path),
            database("scratch", Scheme::Memory, ""),
            database("remote", Scheme::Https, "https://example.com/shop.db"),
            database("gone", Scheme::File, &missing.to_string_lossy()),
        ];
        // The file only: neither the memory, nor the remote copy, nor a
        // missing file, which is not created.
        assert_eq!(refresh_now(&databases), 1);
        assert_eq!(stats(&path), [("t".to_string(), "2000 200".to_string())]);
        assert!(!missing.exists());
    }
}
