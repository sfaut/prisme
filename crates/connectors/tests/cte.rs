//! The CTE export of the interface (`web/src/components/result/cte.ts`) run on real
//! servers: each reference file of `tests/fixtures/export/cte/` must give back
//! the names and the cells of its input, with the types expected for its
//! dialect, on each development server of that dialect (`compose.yaml`). The
//! files written for the `ANSI_QUOTES` and `NO_BACKSLASH_ESCAPES` modes run on
//! a session in these modes. SQLite has a reference of its own
//! (`input-sqlite.json`), loaded into a table of its declared types, then read
//! back from the table and from its CTE.
//!
//! The tests of MySQL and MariaDB are ignored by default, as they require
//! these servers started (`docker compose up --detach --wait`). To run them:
//! `cargo test -p prisme-connectors --test cte -- --include-ignored`.

use std::path::PathBuf;

use prisme_connectors::api::{
    Engine, FileDatabase, FilesSettings, MAIN_SCHEMA, OpenOptions, Session, Settings, Target,
};
use prisme_connectors::columns::{Cell, TypeFamily};
use prisme_connectors::drivers;
use prisme_connectors::flavor::Flavor;
use prisme_connectors::location::{Location, Remote, RemoteLimits, Scheme};
use prisme_connectors::mysql::{Connection, connect, execute};
use prisme_connectors::test_servers;
use serde::Deserialize;

/// Input of the reference files: names and rows of a result, and the types
/// each dialect reads back.
#[derive(Deserialize)]
struct Reference {
    columns: Vec<String>,
    rows: Vec<Vec<Cell>>,
    read_back: ReadBack,
}

/// Families of the columns read back from the CTE, per dialect.
#[derive(Deserialize)]
struct ReadBack {
    mysql: Vec<TypeFamily>,
    mariadb: Vec<TypeFamily>,
}

/// Reference of SQLite: names, declared types and rows of a table, as the
/// connector gives them, and the families read back from the table and from
/// its CTE.
#[derive(Deserialize)]
struct SqliteReference {
    columns: Vec<String>,
    declared: Vec<String>,
    families: Vec<TypeFamily>,
    read_back: Vec<TypeFamily>,
    rows: Vec<Vec<Cell>>,
}

/// Folder of the reference files, shared with the interface's tests.
fn references() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/export/cte")
}

/// Input of the reference files.
fn reference() -> Reference {
    let input = std::fs::read_to_string(references().join("input.json")).unwrap();
    serde_json::from_str(&input).unwrap()
}

/// Query of a reference file, its final separator left out.
fn query(file: &str) -> String {
    let sql = std::fs::read_to_string(references().join(file)).unwrap();
    sql.trim_end().strip_suffix(';').unwrap().to_string()
}

/// Runs a reference file and checks what it gives back.
async fn reads_back(
    connection: &mut Connection,
    file: &str,
    reference: &Reference,
    families: &[TypeFamily],
) {
    let sql = query(file);
    let mut stream = execute(connection, &sql).await.unwrap();
    assert_eq!(stream.column_names(), reference.columns, "{file}");
    let read: Vec<TypeFamily> = stream
        .column_types()
        .into_iter()
        .map(|column| column.family)
        .collect();
    assert_eq!(read, families, "{file}");
    let mut rows = Vec::new();
    while let Some(row) = stream.next_row().await.unwrap() {
        rows.push(row);
    }
    assert_eq!(rows, reference.rows, "{file}");
}

/// Adds the `ANSI_QUOTES` and `NO_BACKSLASH_ESCAPES` modes to the session.
async fn set_modes(connection: &mut Connection) {
    let sql =
        "SET SESSION sql_mode = CONCAT(@@SESSION.sql_mode, ',ANSI_QUOTES,NO_BACKSLASH_ESCAPES')";
    let mut stream = execute(connection, sql).await.unwrap();
    assert_eq!(stream.next_row().await.unwrap(), None);
}

#[tokio::test]
#[ignore = "requires the development servers (docker compose up --detach --wait)"]
async fn mysql_reads_back_the_cte() {
    let reference = reference();
    for server in test_servers::of(Flavor::MySql) {
        server
            .with_database("cte", async |target| {
                let mut connection = connect(&target).await.unwrap();

                // MySQL reads `VALUES ROW` from 8.0.19 only (README): the oldest
                // server, 8.0.11, refuses the CTE.
                if server.name == "mysql-oldest" {
                    let sql = query("expected-mysql.sql");
                    assert!(execute(&mut connection, &sql).await.is_err());
                    return;
                }
                let families = &reference.read_back.mysql;
                reads_back(&mut connection, "expected-mysql.sql", &reference, families).await;
                set_modes(&mut connection).await;
                reads_back(
                    &mut connection,
                    "expected-mysql-modes.sql",
                    &reference,
                    families,
                )
                .await;
            })
            .await;
    }
}

#[tokio::test]
#[ignore = "requires the development servers (docker compose up --detach --wait)"]
async fn mariadb_reads_back_the_cte() {
    let reference = reference();
    for server in test_servers::of(Flavor::MariaDb) {
        server
            .with_database("cte", async |target| {
                let mut connection = connect(&target).await.unwrap();

                let families = &reference.read_back.mariadb;
                reads_back(
                    &mut connection,
                    "expected-mariadb.sql",
                    &reference,
                    families,
                )
                .await;
                set_modes(&mut connection).await;
                reads_back(
                    &mut connection,
                    "expected-mariadb-modes.sql",
                    &reference,
                    families,
                )
                .await;
            })
            .await;
    }
}

/// Runs `sql` on a SQLite session: names, families and rows of its result.
async fn sqlite_result(
    session: &mut dyn Session,
    sql: &str,
) -> (Vec<String>, Vec<TypeFamily>, Vec<Vec<Cell>>) {
    let mut stream = session.execute(sql).await.unwrap();
    let names = stream
        .columns()
        .iter()
        .map(|column| column.name.clone())
        .collect();
    let families = stream
        .columns()
        .iter()
        .map(|column| column.type_info.family)
        .collect();
    let mut rows = Vec::new();
    while let Some(row) = stream.next_row().await.unwrap() {
        rows.push(row);
    }
    stream.discard().await;
    (names, families, rows)
}

#[tokio::test]
async fn sqlite_reads_back_the_cte() {
    let input = std::fs::read_to_string(references().join("input-sqlite.json")).unwrap();
    let reference: SqliteReference = serde_json::from_str(&input).unwrap();
    let folder = tempfile::tempdir().unwrap();
    let main = folder.path().join("main.db");
    std::fs::File::create(&main).unwrap();
    let target = Target {
        engine: Engine::Sqlite,
        settings: Settings::Files(FilesSettings {
            optimize: false,
            databases: vec![FileDatabase {
                schema: MAIN_SCHEMA.to_string(),
                location: Location {
                    scheme: Scheme::File,
                    address: main.to_string_lossy().into_owned(),
                },
            }],
            remote: Remote {
                cache_dir: folder.path().join("cache"),
                limits: RemoteLimits::default(),
            },
        }),
        init_statements: Vec::new(),
    };
    let mut session = drivers::of(Engine::Sqlite)
        .open(&target, OpenOptions::default())
        .await
        .unwrap();
    let cte = query("expected-sqlite.sql");

    // The rows of the reference are those of a table of the declared types.
    let columns: Vec<String> = reference
        .columns
        .iter()
        .zip(&reference.declared)
        .map(|(name, declared)| format!("\"{name}\" {declared}"))
        .collect();
    let create = format!("CREATE TABLE source ({})", columns.join(", "));
    sqlite_result(session.as_mut(), &create).await;
    let insert = format!("INSERT INTO source SELECT * FROM ({cte})");
    sqlite_result(session.as_mut(), &insert).await;
    let table = sqlite_result(session.as_mut(), "SELECT * FROM source ORDER BY rowid").await;
    assert_eq!(
        table,
        (
            reference.columns.clone(),
            reference.families,
            reference.rows.clone()
        )
    );

    // Its CTE gives them back.
    let read = sqlite_result(session.as_mut(), &cte).await;
    assert_eq!(
        read,
        (reference.columns, reference.read_back, reference.rows)
    );
    session.close().await;
}
