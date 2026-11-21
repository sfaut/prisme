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
use prisme_connectors::outcome::{OutcomeCounts, StatementOutcome};
use prisme_connectors::test_servers::SERVERS;
use prisme_dialects::Dialect;
use prisme_dialects::classify::classify;

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

/// Runs the statement `sql` and returns its outcome, its verb taken from its
/// classification as Prisme does.
async fn outcome_of(connection: &mut Connection, sql: &str) -> StatementOutcome {
    let verb = classify(sql, Dialect::MySql).verb();
    let stream = execute(connection, sql).await.unwrap();
    let outcome = stream.outcome(verb);
    stream.discard().await;
    outcome
}

#[tokio::test]
#[ignore = "requires the development servers (docker compose up --detach --wait)"]
async fn reports_the_outcome_of_writes() {
    for server in SERVERS {
        server
            .with_database("outcome", async |target| {
                let mut connection = connect(&target).await.unwrap();
                let name = server.name;
                execute(
                    &mut connection,
                    "CREATE TABLE t (id INT AUTO_INCREMENT PRIMARY KEY, a INT, s VARCHAR(3))",
                )
                .await
                .unwrap()
                .discard()
                .await;

                let insert =
                    outcome_of(&mut connection, "INSERT INTO t (a) VALUES (1), (2), (3)").await;
                assert_eq!(insert.affected, Some(3), "{name}");
                assert_eq!(insert.counts.inserted, Some(3), "{name}");
                // The first id of the rows inserted.
                assert_eq!(insert.generated_id.as_deref(), Some("1"), "{name}");

                let ignore = outcome_of(
                    &mut connection,
                    "INSERT IGNORE INTO t (id, a) VALUES (1, 9), (10, 10)",
                )
                .await;
                assert_eq!(
                    (ignore.counts.inserted, ignore.counts.ignored),
                    (Some(1), Some(1)),
                    "{name}"
                );
                assert_eq!(ignore.warnings, Some(1), "{name}");

                // (1, 1) already holds a = 1, (2, 7) changes it, (20, 20) is new.
                let upsert = outcome_of(
                    &mut connection,
                    concat!(
                        "INSERT INTO t (id, a) VALUES (1, 1), (2, 7), (20, 20) ",
                        "ON DUPLICATE KEY UPDATE a = VALUES(a)",
                    ),
                )
                .await;
                assert_eq!(
                    (
                        upsert.counts.inserted,
                        upsert.counts.changed,
                        upsert.counts.unchanged
                    ),
                    (Some(1), Some(1), Some(1)),
                    "{name}"
                );
                let single = outcome_of(
                    &mut connection,
                    "INSERT INTO t (id, a) VALUES (3, 8) ON DUPLICATE KEY UPDATE a = 8",
                )
                .await;
                assert_eq!(
                    (
                        single.counts.inserted,
                        single.counts.changed,
                        single.counts.unchanged
                    ),
                    (Some(0), Some(1), Some(0)),
                    "{name}"
                );
                let same = outcome_of(
                    &mut connection,
                    "INSERT INTO t (id, a) VALUES (3, 8) ON DUPLICATE KEY UPDATE a = 8",
                )
                .await;
                assert_eq!(same.affected, Some(0), "{name}");
                assert_eq!(same.counts.unchanged, Some(1), "{name}");
                let new = outcome_of(
                    &mut connection,
                    "INSERT INTO t (id, a) VALUES (40, 1) ON DUPLICATE KEY UPDATE a = 1",
                )
                .await;
                assert_eq!(new.affected, Some(1), "{name}");
                assert_eq!(new.counts.inserted, Some(1), "{name}");

                let replace = outcome_of(
                    &mut connection,
                    "REPLACE INTO t (id, a) VALUES (3, 30), (30, 30)",
                )
                .await;
                assert_eq!(replace.affected, Some(3), "{name}");
                assert_eq!(
                    (replace.counts.inserted, replace.counts.replaced),
                    (Some(1), Some(1)),
                    "{name}"
                );

                let update = outcome_of(&mut connection, "UPDATE t SET a = 1 WHERE id <= 3").await;
                assert_eq!(
                    (
                        update.counts.matched,
                        update.counts.changed,
                        update.counts.unchanged
                    ),
                    (Some(3), Some(2), Some(1)),
                    "{name}"
                );
                assert_eq!(update.generated_id, None, "{name}");

                let delete = outcome_of(&mut connection, "DELETE FROM t WHERE id >= 20").await;
                assert_eq!(
                    delete.counts,
                    OutcomeCounts {
                        deleted: Some(3),
                        ..OutcomeCounts::default()
                    },
                    "{name}"
                );

                let truncated = outcome_of(
                    &mut connection,
                    "INSERT IGNORE INTO t (s) VALUES ('abcdef')",
                )
                .await;
                assert_eq!(truncated.warnings, Some(1), "{name}");

                let note = outcome_of(&mut connection, "DROP TABLE IF EXISTS missing").await;
                assert_eq!(note.warnings, Some(1), "{name}");
                assert_eq!(note.counts, OutcomeCounts::default(), "{name}");
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
