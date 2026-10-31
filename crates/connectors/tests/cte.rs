//! The CTE export of the interface (`web/src/components/cte.ts`) run on real
//! servers: each reference file of `tests/fixtures/export/cte/` must give back
//! the names and the cells of its input, with the types expected for its
//! dialect, on each development server of that dialect (`compose.yaml`). The
//! files written for the `ANSI_QUOTES` and `NO_BACKSLASH_ESCAPES` modes run on
//! a session in these modes.
//!
//! Ignored by default, as they require these servers started
//! (`docker compose up --detach --wait`). To run them:
//! `cargo test -p prisme-connectors --test cte -- --include-ignored`.

use std::path::PathBuf;

use prisme_connectors::mysql::{Cell, Connection, Flavor, TypeFamily, connect, execute};
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
