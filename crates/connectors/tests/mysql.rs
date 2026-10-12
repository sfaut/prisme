//! Tests d'intégration du connecteur MySQL / MariaDB, sur de vrais serveurs lancés dans Docker.
//!
//! Ignorés par défaut, car ils demandent Docker Desktop démarré et téléchargent
//! les images au premier lancement. Pour les exécuter :
//! `cargo test -p prisme-connectors -- --include-ignored`.

use prisme_connectors::mysql::{Flavor, MysqlTarget, probe};
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
