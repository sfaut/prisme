//! Connecteur MySQL et MariaDB, avec le pilote `mysql_async`.
//!
//! Les deux moteurs partagent le même protocole et le même pilote, mais Prisme
//! les traite comme deux dialectes distincts (règle 1 de `CLAUDE.md`). La
//! variante est détectée à la connexion par `SELECT VERSION()`, dont le
//! résultat contient « MariaDB » pour ce dernier.

use mysql_async::prelude::Queryable;
use mysql_async::{Conn, OptsBuilder};
use secrecy::{ExposeSecret, SecretString};
use serde::Serialize;

/// Paramètres de connexion à un serveur MySQL ou MariaDB.
pub struct MysqlTarget {
    /// Nom d'hôte ou adresse IP du serveur.
    pub host: String,
    /// Port TCP du serveur (3306 en général).
    pub port: u16,
    /// Compte de connexion.
    pub username: String,
    /// Mot de passe du compte, jamais affiché dans un journal.
    pub password: SecretString,
    /// Base sélectionnée à la connexion. Chaîne vide : aucune.
    pub database_name: String,
}

/// Variante du serveur, détectée à la connexion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Flavor {
    /// Serveur MySQL (Oracle).
    MySql,
    /// Serveur MariaDB.
    MariaDb,
}

/// Identité d'un serveur, obtenue à la connexion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ServerInfo {
    /// MySQL ou MariaDB.
    pub flavor: Flavor,
    /// Résultat brut de `SELECT VERSION()`, par exemple `8.1.0` ou `11.3.2-MariaDB-1:11.3.2+maria~ubu2204`.
    pub version: String,
}

/// Erreurs du connecteur MySQL / MariaDB.
#[derive(Debug, thiserror::Error)]
pub enum MysqlError {
    /// Erreur du pilote : serveur injoignable, authentification refusée, requête en échec...
    #[error("{0}")]
    Driver(#[from] mysql_async::Error),

    /// Le serveur n'a renvoyé aucune version.
    #[error("le serveur n'a renvoyé aucune version")]
    MissingVersion,
}

/// Déduit la variante du serveur du résultat de `SELECT VERSION()`.
pub fn detect_flavor(version: &str) -> Flavor {
    if version.to_ascii_lowercase().contains("mariadb") {
        Flavor::MariaDb
    } else {
        Flavor::MySql
    }
}

/// Ouvre une connexion au serveur.
pub async fn connect(target: &MysqlTarget) -> Result<Conn, MysqlError> {
    let database_name = Some(target.database_name.as_str()).filter(|name| !name.is_empty());
    let options = OptsBuilder::default()
        .ip_or_hostname(target.host.as_str())
        .tcp_port(target.port)
        .user(Some(target.username.as_str()))
        .pass(Some(target.password.expose_secret()))
        .db_name(database_name);
    Ok(Conn::new(options).await?)
}

/// Se connecte au serveur, lit son identité, puis se déconnecte.
pub async fn probe(target: &MysqlTarget) -> Result<ServerInfo, MysqlError> {
    let mut connection = connect(target).await?;
    let version: Option<String> = connection.query_first("SELECT VERSION()").await?;
    connection.disconnect().await?;

    let version = version.ok_or(MysqlError::MissingVersion)?;
    Ok(ServerInfo {
        flavor: detect_flavor(&version),
        version,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_mariadb() {
        assert_eq!(
            detect_flavor("11.3.2-MariaDB-1:11.3.2+maria~ubu2204"),
            Flavor::MariaDb
        );
        assert_eq!(detect_flavor("10.6.18-mariadb-log"), Flavor::MariaDb);
    }

    #[test]
    fn detects_mysql() {
        assert_eq!(detect_flavor("8.1.0"), Flavor::MySql);
        assert_eq!(detect_flavor("5.7.44-log"), Flavor::MySql);
    }
}
