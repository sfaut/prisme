//! Connecteur MySQL et MariaDB, avec le pilote `mysql_async`.
//!
//! Les deux moteurs partagent le même protocole et le même pilote, mais Prisme
//! les traite comme deux dialectes distincts (règle 1 de `CLAUDE.md`). La
//! variante est détectée à la connexion par `SELECT VERSION()`, dont le
//! résultat contient « MariaDB » pour ce dernier.
//!
//! Les résultats se lisent ligne à ligne (`execute`, puis `ResultStream`),
//! sans jamais charger tout le résultat en mémoire.

use mysql_async::prelude::Queryable;
use mysql_async::{Conn, OptsBuilder, QueryResult, TextProtocol, Value};
use secrecy::{ExposeSecret, SecretString};
use serde::Serialize;

/// Paramètres de connexion à un serveur MySQL ou MariaDB.
#[derive(Clone)]
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

/// Lit l'identité du serveur sur une connexion ouverte.
pub async fn server_info(connection: &mut Conn) -> Result<ServerInfo, MysqlError> {
    let version: Option<String> = connection.query_first("SELECT VERSION()").await?;
    let version = version.ok_or(MysqlError::MissingVersion)?;
    Ok(ServerInfo {
        flavor: detect_flavor(&version),
        version,
    })
}

/// Se connecte au serveur, lit son identité, puis se déconnecte.
pub async fn probe(target: &MysqlTarget) -> Result<ServerInfo, MysqlError> {
    let mut connection = connect(target).await?;
    let info = server_info(&mut connection).await?;
    connection.disconnect().await?;
    Ok(info)
}

/// Interrompt la requête en cours sur la connexion `connection_id` (`Conn::id()`).
///
/// Passe par une seconde connexion, car la première est occupée par la
/// requête. La connexion interrompue reste ouverte et réutilisable.
pub async fn kill_query(target: &MysqlTarget, connection_id: u32) -> Result<(), MysqlError> {
    let mut connection = connect(target).await?;
    connection
        .query_drop(format!("KILL QUERY {connection_id}"))
        .await?;
    connection.disconnect().await?;
    Ok(())
}

/// Valeur d'une cellule de résultat : `None` pour NULL, sinon du texte.
///
/// Tout est transmis en texte, nombres compris : aucune perte de précision
/// sur un `BIGINT UNSIGNED` ou un `DECIMAL`. Le typage fin viendra avec le
/// crate `types`.
pub type Cell = Option<String>;

/// Résultat d'une requête en cours de lecture, ligne à ligne.
pub struct ResultStream<'a> {
    /// Résultat du pilote, qui garde la connexion empruntée jusqu'à la fin de la lecture.
    result: QueryResult<'a, 'static, TextProtocol>,
}

/// Lance la requête `sql` et renvoie son résultat, à lire ligne à ligne.
pub async fn execute<'a>(
    connection: &'a mut Conn,
    sql: &'a str,
) -> Result<ResultStream<'a>, MysqlError> {
    let result = connection.query_iter(sql).await?;
    Ok(ResultStream { result })
}

impl ResultStream<'_> {
    /// Noms des colonnes. Vide pour une requête qui ne renvoie pas de lignes.
    pub fn column_names(&self) -> Vec<String> {
        self.result
            .columns_ref()
            .iter()
            .map(|column| column.name_str().into_owned())
            .collect()
    }

    /// Lit la ligne suivante, ou `None` à la fin du résultat.
    pub async fn next_row(&mut self) -> Result<Option<Vec<Cell>>, MysqlError> {
        let Some(row) = self.result.next().await? else {
            return Ok(None);
        };
        let cells = row
            .unwrap_raw()
            .into_iter()
            .map(|value| value.and_then(cell))
            .collect();
        Ok(Some(cells))
    }

    /// Nombre de lignes modifiées, pour une écriture.
    pub fn affected_rows(&self) -> u64 {
        self.result.affected_rows()
    }

    /// Abandonne la lecture : les lignes restantes sont lues et jetées par le
    /// pilote, pour que la connexion redevienne utilisable. Après un
    /// `kill_query`, le serveur n'en envoie plus et l'erreur d'interruption est
    /// ignorée.
    pub async fn discard(self) {
        // L'erreur attendue ici est celle de l'interruption : rien à en faire.
        let _ = self.result.drop_result().await;
    }
}

/// Convertit une valeur du pilote en texte, ou `None` pour NULL.
fn cell(value: Value) -> Cell {
    match value {
        Value::NULL => None,
        Value::Bytes(bytes) => Some(match String::from_utf8(bytes) {
            Ok(text) => text,
            // Données binaires (BLOB, BINARY...) : affichées en hexadécimal, comme le fait MySQL.
            Err(error) => format!("0x{}", hex(error.as_bytes())),
        }),
        Value::Int(number) => Some(number.to_string()),
        Value::UInt(number) => Some(number.to_string()),
        Value::Float(number) => Some(number.to_string()),
        Value::Double(number) => Some(number.to_string()),
        Value::Date(year, month, day, hour, minute, second, micro) => Some(format!(
            "{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02}.{micro:06}"
        )),
        Value::Time(negative, days, hours, minutes, seconds, micro) => {
            let sign = if negative { "-" } else { "" };
            let hours = u32::from(hours) + days * 24;
            Some(format!(
                "{sign}{hours:02}:{minutes:02}:{seconds:02}.{micro:06}"
            ))
        }
    }
}

/// Écrit des octets en hexadécimal majuscule.
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02X}")).collect()
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
    fn converts_cells_to_text() {
        assert_eq!(cell(Value::NULL), None);
        assert_eq!(
            cell(Value::Bytes("clé €".into())),
            Some("clé €".to_string())
        );
        assert_eq!(
            cell(Value::Bytes(vec![0xFF, 0x00, 0x1A])),
            Some("0xFF001A".to_string())
        );
        assert_eq!(
            cell(Value::UInt(u64::MAX)),
            Some("18446744073709551615".to_string())
        );
        assert_eq!(
            cell(Value::Date(2026, 10, 12, 9, 5, 0, 120)),
            Some("2026-10-12 09:05:00.000120".to_string())
        );
        assert_eq!(
            cell(Value::Time(true, 1, 2, 3, 4, 0)),
            Some("-26:03:04.000000".to_string())
        );
    }

    #[test]
    fn detects_mysql() {
        assert_eq!(detect_flavor("8.1.0"), Flavor::MySql);
        assert_eq!(detect_flavor("5.7.44-log"), Flavor::MySql);
    }
}
