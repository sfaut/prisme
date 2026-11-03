//! Integration tests of the MySQL / MariaDB connector, on the development
//! servers of `compose.yaml`: each test runs on every server of the engines
//! it concerns, in a database of its own.
//!
//! Ignored by default, as they require these servers started
//! (`docker compose up --detach --wait`). To run them:
//! `cargo test -p prisme-connectors -- --include-ignored`.

use std::time::{Duration, Instant};

use prisme_connectors::mysql::catalog::{self, RoutineKind, TableKind};
use prisme_connectors::mysql::{
    Connection, Flavor, connect, execute, in_transaction, kill_query, probe,
};
use prisme_connectors::test_servers::SERVERS;

#[tokio::test]
#[ignore = "requires the development servers (docker compose up --detach --wait)"]
async fn probe_detects_the_flavor_of_supported_servers() {
    for server in SERVERS {
        let info = probe(&server.target("")).await.unwrap();

        assert_eq!(info.flavor, server.flavor, "{}", server.name);
        assert_eq!(info.oldest_supported, None, "{}", server.name);
        // The oldest server is the oldest version Prisme supports.
        if server.name.ends_with("-oldest") {
            assert_eq!(
                info.version.split('-').next(),
                Some(server.flavor.oldest_supported()),
                "{}",
                server.name
            );
        }
    }
}

#[tokio::test]
#[ignore = "requires the development servers (docker compose up --detach --wait)"]
async fn streams_rows_and_reports_writes() {
    for server in SERVERS {
        server
            .with_database("streams_rows", async |target| {
                let mut connection = connect(&target).await.unwrap();

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

                let mut stream =
                    execute(&mut connection, "SELECT id, label, data FROM t ORDER BY id")
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
            })
            .await;
    }
}

#[tokio::test]
#[ignore = "requires the development servers (docker compose up --detach --wait)"]
async fn kill_query_interrupts_and_keeps_the_connection() {
    for server in SERVERS {
        server
            .with_database("kill_query", async |target| {
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
            })
            .await;
    }
}

#[tokio::test]
#[ignore = "requires the development servers (docker compose up --detach --wait)"]
async fn reads_the_catalog_schema_by_schema() {
    for server in SERVERS {
        server
            .with_database("catalog", async |target| {
                let mut connection = connect(&target).await.unwrap();
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
                assert!(schemas.contains(&target.database_name));

                let objects = catalog::objects(&mut connection, &target.database_name)
                    .await
                    .unwrap();
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

                let details = catalog::details(&mut connection, &target.database_name)
                    .await
                    .unwrap();
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
                // The navigator finds the references of a table by this database.
                assert_eq!(t.foreign_keys[0].referenced_schema, target.database_name);
                assert_eq!(t.foreign_keys[0].referenced_table, "p");
                assert_eq!(t.foreign_keys[0].referenced_columns, ["id"]);
                assert_eq!(t.triggers[0].name, "t_bi");
                assert_eq!(
                    (t.triggers[0].timing.as_str(), t.triggers[0].event.as_str()),
                    ("BEFORE", "INSERT")
                );
            })
            .await;
    }
}

#[tokio::test]
#[ignore = "requires the development servers (docker compose up --detach --wait)"]
async fn tells_an_open_transaction() {
    for server in SERVERS {
        server
            .with_database("open_transaction", async |target| {
                let mut connection = connect(&target).await.unwrap();
                let mut run = async |sql: &str| {
                    if let Ok(stream) = execute(&mut connection, sql).await {
                        stream.discard().await;
                    }
                    in_transaction(&mut connection, false).await.unwrap()
                };

                assert!(!run("CREATE TABLE t (a INT)").await);
                assert!(run("START TRANSACTION").await);
                assert!(run("INSERT INTO t VALUES (1)").await);
                // An error leaves the transaction open.
                assert!(run("SELECT * FROM missing").await);
                assert!(!run("COMMIT").await);
                assert!(run("BEGIN").await);
                // A statement that commits implicitly ends it, even when it fails.
                assert!(!run("CREATE TABLE t (a INT)").await);
                assert!(!run("SET autocommit = 0").await);
                assert!(run("SELECT * FROM t").await);
                assert!(!run("ROLLBACK").await);
            })
            .await;
    }
}

/// Table, column and primary key of the origin of each column of a query.
async fn origins(connection: &mut Connection, sql: &str) -> Vec<Option<(String, String, bool)>> {
    let stream = execute(connection, sql).await.unwrap();
    let origins = stream
        .column_types()
        .into_iter()
        .map(|column| column.origin.map(|o| (o.table, o.column, o.primary_key)))
        .collect();
    stream.discard().await;
    origins
}

#[tokio::test]
#[ignore = "requires the development servers (docker compose up --detach --wait)"]
async fn tells_the_origin_of_the_columns_of_a_result() {
    let origin = |table: &str, column: &str, primary_key| {
        Some((table.to_string(), column.to_string(), primary_key))
    };
    for server in SERVERS {
        server
            .with_database("column_origins", async |target| {
                let mut connection = connect(&target).await.unwrap();
                for sql in [
                    "CREATE TABLE p (id INT PRIMARY KEY)",
                    "CREATE TABLE f (id INT PRIMARY KEY, p_id INT, amount INT, KEY (amount),
                     FOREIGN KEY (p_id) REFERENCES p (id))",
                    "CREATE TABLE c (a INT, b INT, PRIMARY KEY (a, b))",
                    "CREATE VIEW w AS SELECT id, p_id FROM f",
                ] {
                    execute(&mut connection, sql).await.unwrap().discard().await;
                }

                assert_eq!(
                    origins(&mut connection, "SELECT id, p_id, amount FROM f").await,
                    [
                        origin("f", "id", true),
                        origin("f", "p_id", false),
                        origin("f", "amount", false),
                    ],
                    "{}",
                    server.name
                );
                assert_eq!(
                    origins(&mut connection, "SELECT id AS x FROM f").await,
                    [origin("f", "id", true)],
                    "{}",
                    server.name
                );
                assert_eq!(
                    origins(&mut connection, "SELECT a, b FROM c").await,
                    [origin("c", "a", true), origin("c", "b", true)],
                    "{}",
                    server.name
                );
                assert_eq!(
                    origins(
                        &mut connection,
                        "SELECT f.id, p.id FROM f JOIN p ON p.id = f.p_id"
                    )
                    .await,
                    [origin("f", "id", true), origin("p", "id", true)],
                    "{}",
                    server.name
                );
                assert_eq!(
                    origins(&mut connection, "SELECT id FROM w").await,
                    [origin("w", "id", true)],
                    "{}",
                    server.name
                );
                let derived = match server.flavor {
                    Flavor::MySql => "f",
                    Flavor::MariaDb => "d",
                };
                assert_eq!(
                    origins(&mut connection, "SELECT id FROM (SELECT id FROM f) AS d").await,
                    [origin(derived, "id", true)],
                    "{}",
                    server.name
                );
                for sql in [
                    "SELECT id + 1 AS x FROM f",
                    "SELECT id FROM f UNION ALL SELECT id FROM f",
                ] {
                    assert_eq!(
                        origins(&mut connection, sql).await,
                        [None],
                        "{}",
                        server.name
                    );
                }

                let stream = execute(&mut connection, "SELECT id FROM f").await.unwrap();
                let schema = stream.column_types()[0].origin.clone().unwrap().schema;
                stream.discard().await;
                assert_eq!(schema, target.database_name, "{}", server.name);
            })
            .await;
    }
}

#[tokio::test]
#[ignore = "requires the development servers (docker compose up --detach --wait)"]
async fn tells_a_connection_the_server_closed() {
    for server in SERVERS {
        server
            .with_database("closed_connection", async |target| {
                let mut connection = connect(&target).await.unwrap();
                for sql in ["SET SESSION completion_type = 2", "START TRANSACTION"] {
                    execute(&mut connection, sql).await.unwrap().discard().await;
                }
                assert!(in_transaction(&mut connection, true).await.unwrap());
                // The server closes the connection after the COMMIT.
                execute(&mut connection, "COMMIT")
                    .await
                    .unwrap()
                    .discard()
                    .await;
                assert!(in_transaction(&mut connection, true).await.is_err());
            })
            .await;
    }
}
