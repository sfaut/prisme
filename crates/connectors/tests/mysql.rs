//! Tests d'intégration du connecteur MySQL / MariaDB, sur de vrais serveurs lancés dans Docker.
//!
//! Ignorés par défaut, car ils demandent Docker Desktop démarré et téléchargent
//! les images au premier lancement. Pour les exécuter :
//! `cargo test -p prisme-connectors -- --include-ignored`.

use std::time::{Duration, Instant};

use prisme_connectors::mysql::{Flavor, MysqlTarget, connect, execute, kill_query, probe};
use secrecy::SecretString;
use testcontainers_modules::mariadb::Mariadb;
use testcontainers_modules::mysql::Mysql;
use testcontainers_modules::testcontainers::runners::AsyncRunner;

/// Paramètres de connexion des images de test : compte root sans mot de passe, base `test`.
fn target(host: String, port: u16) -> MysqlTarget {
    MysqlTarget {
        host,
        port,
        username: "root".to_string(),
        password: SecretString::from(""),
        database_name: "test".to_string(),
    }
}

#[tokio::test]
#[ignore = "nécessite Docker Desktop démarré"]
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
#[ignore = "nécessite Docker Desktop démarré"]
async fn probe_detects_mariadb() {
    let server = Mariadb::default().start().await.unwrap();
    let host = server.get_host().await.unwrap().to_string();
    let port = server.get_host_port_ipv4(3306).await.unwrap();

    let info = probe(&target(host, port)).await.unwrap();

    assert_eq!(info.flavor, Flavor::MariaDb);
}

#[tokio::test]
#[ignore = "nécessite Docker Desktop démarré"]
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
#[ignore = "nécessite Docker Desktop démarré"]
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
    // Interrompu, SLEEP() renvoie 1 ou une erreur selon le moment : seule la durée compte.
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
