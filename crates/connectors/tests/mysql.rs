//! Integration tests of the MySQL / MariaDB connector, on real servers started in Docker.
//!
//! Ignored by default, as they require Docker Desktop running and download the
//! images on the first run. To run them:
//! `cargo test -p prisme-connectors -- --include-ignored`.

use std::time::{Duration, Instant};

use prisme_connectors::mysql::catalog::{self, RoutineKind, TableKind};
use prisme_connectors::mysql::{Flavor, MysqlTarget, connect, execute, kill_query, probe};
use prisme_types::{DataType, IntegerSize, IntegerType};
use secrecy::SecretString;
use testcontainers_modules::mariadb::Mariadb;
use testcontainers_modules::mysql::Mysql;
use testcontainers_modules::testcontainers::runners::AsyncRunner;

/// Connection settings of the test images: root account without a password, `test` database.
/// `localhost` is written `127.0.0.1`: `localhost` resolves to `::1` first, where the port of the
/// container does not answer, and Windows reports the refusal after 2 seconds (`docs/PERF.md`,
/// chapter 8).
fn target(host: String, port: u16) -> MysqlTarget {
    let host = if host == "localhost" {
        "127.0.0.1".to_string()
    } else {
        host
    };
    MysqlTarget {
        host,
        port,
        username: "root".to_string(),
        password: SecretString::from(""),
        database_name: "test".to_string(),
        init_statements: Vec::new(),
    }
}

#[tokio::test]
#[ignore = "requires Docker Desktop running"]
async fn probe_detects_mysql() {
    let server = Mysql::default().start().await.unwrap();
    let host = server.get_host().await.unwrap().to_string();
    let port = server.get_host_port_ipv4(3306).await.unwrap();

    let info = probe(&target(host, port)).await.unwrap();

    assert_eq!(info.flavor, Flavor::MySql);
    assert!(
        info.version.starts_with("8."),
        "version inattendue : {}",
        info.version
    );
}

#[tokio::test]
#[ignore = "requires Docker Desktop running"]
async fn probe_detects_mariadb() {
    let server = Mariadb::default().start().await.unwrap();
    let host = server.get_host().await.unwrap().to_string();
    let port = server.get_host_port_ipv4(3306).await.unwrap();

    let info = probe(&target(host, port)).await.unwrap();

    assert_eq!(info.flavor, Flavor::MariaDb);
}

#[tokio::test]
#[ignore = "requires Docker Desktop running"]
async fn streams_rows_and_reports_writes() {
    let server = Mariadb::default().start().await.unwrap();
    let host = server.get_host().await.unwrap().to_string();
    let port = server.get_host_port_ipv4(3306).await.unwrap();
    let mut connection = connect(&target(host, port)).await.unwrap();

    let mut stream = execute(
        &mut connection,
        "CREATE TABLE t (id INT, label VARCHAR(20), data VARBINARY(4))",
    )
    .await
    .unwrap();
    assert!(stream.column_names().is_empty());
    assert_eq!(stream.next_row().await.unwrap(), None);
    drop(stream);

    let stream = execute(
        &mut connection,
        "INSERT INTO t VALUES (1, 'clé €', 0xFF00), (2, NULL, NULL)",
    )
    .await
    .unwrap();
    assert_eq!(stream.affected_rows(), 2);
    drop(stream);

    let mut stream = execute(&mut connection, "SELECT id, label, data FROM t ORDER BY id")
        .await
        .unwrap();
    assert_eq!(stream.column_names(), ["id", "label", "data"]);
    assert_eq!(
        stream.next_row().await.unwrap(),
        Some(vec![
            Some("1".to_string()),
            Some("clé €".to_string()),
            Some("0xFF00".to_string()),
        ])
    );
    assert_eq!(
        stream.next_row().await.unwrap(),
        Some(vec![Some("2".to_string()), None, None])
    );
    assert_eq!(stream.next_row().await.unwrap(), None);
}

#[tokio::test]
#[ignore = "requires Docker Desktop running"]
async fn kill_query_interrupts_and_keeps_the_connection() {
    let server = Mysql::default().start().await.unwrap();
    let host = server.get_host().await.unwrap().to_string();
    let port = server.get_host_port_ipv4(3306).await.unwrap();
    let target = target(host, port);
    let mut connection = connect(&target).await.unwrap();
    let connection_id = connection.id();

    let killer = {
        let target = target.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(1)).await;
            kill_query(&target, connection_id).await.unwrap();
        })
    };
    let started = Instant::now();
    // Interrupted, SLEEP() returns 1 or an error depending on the moment: only the duration counts.
    if let Ok(stream) = execute(&mut connection, "SELECT SLEEP(30)").await {
        stream.discard().await;
    }
    killer.await.unwrap();
    assert!(started.elapsed() < Duration::from_secs(10));

    let mut stream = execute(&mut connection, "SELECT 1").await.unwrap();
    assert_eq!(
        stream.next_row().await.unwrap(),
        Some(vec![Some("1".to_string())])
    );
}

#[tokio::test]
#[ignore = "requires Docker Desktop running"]
async fn reads_the_catalog_schema_by_schema() {
    let server = Mariadb::default().start().await.unwrap();
    let host = server.get_host().await.unwrap().to_string();
    let port = server.get_host_port_ipv4(3306).await.unwrap();
    let mut connection = connect(&target(host, port)).await.unwrap();
    for sql in [
        "CREATE TABLE p (id INT PRIMARY KEY)",
        "CREATE TABLE t (id INT AUTO_INCREMENT PRIMARY KEY, label VARCHAR(20) NULL, p_id INT,
            INDEX idx_label (label),
            CONSTRAINT fk_p FOREIGN KEY (p_id) REFERENCES p (id))",
        "CREATE VIEW v AS SELECT id FROM t",
        "CREATE TRIGGER t_bi BEFORE INSERT ON t FOR EACH ROW SET NEW.label = UPPER(NEW.label)",
        "CREATE PROCEDURE do_nothing() BEGIN END",
        "CREATE FUNCTION one() RETURNS INT DETERMINISTIC RETURN 1",
        "CREATE EVENT e_daily ON SCHEDULE EVERY 1 DAY DO SELECT 1",
        "CREATE EVENT e_paused ON SCHEDULE EVERY 1 DAY DISABLE DO SELECT 1",
    ] {
        execute(&mut connection, sql).await.unwrap().discard().await;
    }

    let schemas = catalog::schemas(&mut connection).await.unwrap();
    assert!(schemas.contains(&"test".to_string()));

    let objects = catalog::objects(&mut connection, "test").await.unwrap();
    let kinds: Vec<_> = objects
        .tables
        .iter()
        .map(|table| (table.name.as_str(), table.kind))
        .collect();
    assert_eq!(
        kinds,
        [
            ("p", TableKind::Table),
            ("t", TableKind::Table),
            ("v", TableKind::View)
        ]
    );
    let routines: Vec<_> = objects
        .routines
        .iter()
        .map(|routine| (routine.name.as_str(), routine.kind))
        .collect();
    assert_eq!(
        routines,
        [
            ("do_nothing", RoutineKind::Procedure),
            ("one", RoutineKind::Function)
        ]
    );
    let events: Vec<_> = objects
        .events
        .iter()
        .map(|event| (event.name.as_str(), event.enabled))
        .collect();
    assert_eq!(events, [("e_daily", true), ("e_paused", false)]);

    let details = catalog::details(&mut connection, "test").await.unwrap();
    let t = details.iter().find(|table| table.table == "t").unwrap();
    assert_eq!(t.columns[0].name, "id");
    assert_eq!(t.columns[0].key, "PRI");
    assert!(t.columns[0].auto_increment);
    assert!(!t.columns[1].auto_increment);
    assert_eq!(t.columns[1].column_type, "varchar(20)");
    assert!(t.columns[1].nullable);
    let indexes: Vec<_> = t.indexes.iter().map(|index| index.name.as_str()).collect();
    assert_eq!(indexes, ["PRIMARY", "fk_p", "idx_label"]);
    assert_eq!(t.foreign_keys[0].name, "fk_p");
    assert_eq!(t.foreign_keys[0].referenced_table, "p");
    assert_eq!(t.foreign_keys[0].referenced_columns, ["id"]);
    assert_eq!(t.triggers[0].name, "t_bi");
    assert_eq!(
        (t.triggers[0].timing.as_str(), t.triggers[0].event.as_str()),
        ("BEFORE", "INSERT")
    );
}

#[tokio::test]
#[ignore = "requires Docker Desktop running"]
async fn reads_the_columns_of_a_table_without_running_it() {
    let server = Mysql::default().start().await.unwrap();
    let host = server.get_host().await.unwrap().to_string();
    let port = server.get_host_port_ipv4(3306).await.unwrap();
    let mut connection = connect(&target(host, port)).await.unwrap();
    for sql in [
        "CREATE TABLE t (id INT NOT NULL, label VARCHAR(20), doc JSON, body LONGTEXT)",
        "CREATE TEMPORARY TABLE `odd``name` (n BIGINT UNSIGNED NOT NULL, d JSON)",
    ] {
        execute(&mut connection, sql).await.unwrap().discard().await;
    }

    let columns = catalog::table_columns(&mut connection, Some("test"), "t")
        .await
        .unwrap()
        .unwrap();
    let names: Vec<&str> = columns.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(names, ["id", "label", "doc", "body"]);
    assert_eq!(
        columns[0].1.data_type,
        DataType::Integer(IntegerType::signed(IntegerSize::Int))
    );
    assert!(!columns[0].1.nullable);
    assert!(columns[1].1.nullable);
    // A `JSON` and a `LONGTEXT`, of the same metadata once prepared.
    assert_eq!(columns[2].1.data_type, DataType::Json);
    assert!(matches!(columns[3].1.data_type, DataType::Text(_)));

    // A temporary table of the connection, in the current database.
    let temporary = catalog::table_columns(&mut connection, None, "odd`name")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        temporary[0].1.data_type,
        DataType::Integer(IntegerType::unsigned(IntegerSize::Big))
    );
    assert_eq!(temporary[1].1.data_type, DataType::Json);
    assert!(
        catalog::table_columns(&mut connection, None, "missing")
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        catalog::table_columns(&mut connection, Some("nowhere"), "t")
            .await
            .unwrap()
            .is_none()
    );
}
