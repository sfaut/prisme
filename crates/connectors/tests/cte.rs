//! The CTE export of the interface (`web/src/components/cte.ts`) run on real
//! servers: each reference file of `tests/fixtures/export/cte/` must give back
//! the names and the cells of its input, with the types expected for its
//! dialect. The files written for the `ANSI_QUOTES` and `NO_BACKSLASH_ESCAPES`
//! modes run on a session in these modes.
//!
//! Ignored by default, as they require Docker Desktop running and download the
//! images on the first run. To run them:
//! `cargo test -p prisme-connectors --test cte -- --include-ignored`.

use std::path::PathBuf;

use prisme_connectors::mysql::{Cell, Connection, MysqlTarget, TypeFamily, connect, execute};
use secrecy::SecretString;
use serde::Deserialize;
use testcontainers_modules::mariadb::Mariadb;
use testcontainers_modules::mysql::Mysql;
use testcontainers_modules::testcontainers::runners::AsyncRunner;

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

/// Connection settings of the test images: root account without a password, `test` database.
fn target(host: String, port: u16) -> MysqlTarget {
    MysqlTarget {
        host,
        port,
        username: "root".to_string(),
        password: SecretString::from(""),
        database_name: "test".to_string(),
        init_statements: Vec::new(),
    }
}

/// Runs a reference file, its final separator left out, and checks what it
/// gives back.
async fn reads_back(
    connection: &mut Connection,
    file: &str,
    reference: &Reference,
    families: &[TypeFamily],
) {
    let sql = std::fs::read_to_string(references().join(file)).unwrap();
    let query = sql.trim_end().strip_suffix(';').unwrap();
    let mut stream = execute(connection, query).await.unwrap();
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
#[ignore = "requires Docker Desktop running"]
async fn mysql_reads_back_the_cte() {
    let server = Mysql::default().start().await.unwrap();
    let host = server.get_host().await.unwrap().to_string();
    let port = server.get_host_port_ipv4(3306).await.unwrap();
    let mut connection = connect(&target(host, port)).await.unwrap();
    let reference = reference();

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
}

#[tokio::test]
#[ignore = "requires Docker Desktop running"]
async fn mariadb_reads_back_the_cte() {
    let server = Mariadb::default().start().await.unwrap();
    let host = server.get_host().await.unwrap().to_string();
    let port = server.get_host_port_ipv4(3306).await.unwrap();
    let mut connection = connect(&target(host, port)).await.unwrap();
    let reference = reference();

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
}
