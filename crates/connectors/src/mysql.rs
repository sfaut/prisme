//! Connecteur MySQL et MariaDB, avec le pilote `mysql_async`.
//!
//! Les deux moteurs partagent le même protocole et le même pilote, mais Prisme
//! les traite comme deux dialectes distincts (règle 1 de `CLAUDE.md`). La
//! variante est détectée à la connexion par `SELECT VERSION()`, dont le
//! résultat contient « MariaDB » pour ce dernier.
//!
//! Les résultats se lisent ligne à ligne (`execute`, puis `ResultStream`),
//! sans jamais charger tout le résultat en mémoire. Le catalogue (bases,
//! tables, colonnes) est dans le sous-module `catalog`, le type des colonnes
//! d'un résultat dans `column_type`.

use std::time::Duration;

use mysql_async::consts::ColumnType;
use mysql_async::prelude::Queryable;
use mysql_async::{Conn, OptsBuilder, QueryResult, TextProtocol, Value};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Connexion ouverte à un serveur MySQL ou MariaDB.
pub use mysql_async::Conn as Connection;

pub mod catalog;
mod column_type;
mod connect_error;

pub use column_type::{ColumnTypeInfo, TypeFamily};

/// Délai maximal pour ouvrir une connexion au serveur.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

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
    /// Requêtes exécutées à l'ouverture de chaque connexion, dans l'ordre
    /// (`SET NAMES utf8mb4`, `SET SESSION sql_mode = ...`).
    pub init_statements: Vec<String>,
}

/// Variante du serveur, détectée à la connexion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "lowercase")]
pub enum Flavor {
    /// Serveur MySQL (Oracle).
    MySql,
    /// Serveur MariaDB.
    MariaDb,
}

/// Identité d'un serveur, obtenue à la connexion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct ServerInfo {
    /// MySQL ou MariaDB.
    pub flavor: Flavor,
    /// Résultat brut de `SELECT VERSION()`, par exemple `8.1.0` ou `11.3.2-MariaDB-1:11.3.2+maria~ubu2204`.
    pub version: String,
    /// Le mode `ANSI_QUOTES` est-il actif sur la session (directement, ou par
    /// le mode `ANSI`) ? Les guillemets doubles délimitent alors des
    /// identifiants, et non plus des chaînes.
    pub ansi_quotes: bool,
}

/// Erreurs du connecteur MySQL / MariaDB.
#[derive(Debug, thiserror::Error)]
pub enum MysqlError {
    /// Connexion impossible, avec un message qui dit quoi vérifier.
    #[error("{0}")]
    Connect(String),

    /// Le serveur n'a pas répondu dans le délai `CONNECT_TIMEOUT`.
    #[error("{0}")]
    ConnectTimeout(String),

    /// Erreur du pilote sur une connexion ouverte : requête en échec, connexion perdue...
    #[error("{0}")]
    Driver(#[from] mysql_async::Error),

    /// Le serveur n'a renvoyé aucune version.
    #[error("le serveur n'a renvoyé aucune version")]
    MissingVersion,

    /// Une requête d'initialisation de la connexion a échoué.
    #[error("requête à la connexion « {statement} » en échec : {message}")]
    Init {
        /// La requête en échec.
        statement: String,
        /// Message du serveur.
        message: String,
    },
}

/// Déduit la variante du serveur du résultat de `SELECT VERSION()`.
pub fn detect_flavor(version: &str) -> Flavor {
    if version.to_ascii_lowercase().contains("mariadb") {
        Flavor::MariaDb
    } else {
        Flavor::MySql
    }
}

/// Ouvre une connexion au serveur, en `CONNECT_TIMEOUT` au plus, puis y
/// exécute les requêtes d'initialisation de la cible, dans l'ordre.
///
/// En cas d'échec, le message dit ce qui ne va pas (serveur injoignable, accès
/// refusé, base inconnue, requête d'initialisation refusée...) et quoi vérifier.
pub async fn connect(target: &MysqlTarget) -> Result<Conn, MysqlError> {
    let database_name = Some(target.database_name.as_str()).filter(|name| !name.is_empty());
    let options = OptsBuilder::default()
        .ip_or_hostname(target.host.as_str())
        .tcp_port(target.port)
        .user(Some(target.username.as_str()))
        .pass(Some(target.password.expose_secret()))
        .db_name(database_name);
    let mut connection = match tokio::time::timeout(CONNECT_TIMEOUT, Conn::new(options)).await {
        Ok(Ok(connection)) => connection,
        Ok(Err(error)) => return Err(MysqlError::Connect(connect_error::describe(target, &error))),
        Err(_) => {
            return Err(MysqlError::ConnectTimeout(connect_error::timeout_message(
                target,
                CONNECT_TIMEOUT,
            )));
        }
    };
    for statement in &target.init_statements {
        if let Err(error) = connection.query_drop(statement.as_str()).await {
            let _ = connection.disconnect().await;
            return Err(MysqlError::Init {
                statement: statement.clone(),
                message: error.to_string(),
            });
        }
    }
    Ok(connection)
}

/// Lit l'identité du serveur sur une connexion ouverte.
pub async fn server_info(connection: &mut Conn) -> Result<ServerInfo, MysqlError> {
    let row: Option<(String, String)> = connection
        .query_first("SELECT VERSION(), @@SESSION.sql_mode")
        .await?;
    let (version, sql_mode) = row.ok_or(MysqlError::MissingVersion)?;
    Ok(ServerInfo {
        flavor: detect_flavor(&version),
        ansi_quotes: has_ansi_quotes(&sql_mode),
        version,
    })
}

/// `sql_mode` contient-il `ANSI_QUOTES` ? Le serveur développe le mode
/// `ANSI` en ses composants dans `@@sql_mode`, `ANSI_QUOTES` compris.
fn has_ansi_quotes(sql_mode: &str) -> bool {
    sql_mode
        .split(',')
        .any(|mode| mode.trim().eq_ignore_ascii_case("ANSI_QUOTES"))
}

/// Base en cours sur une connexion ouverte (`SELECT DATABASE()`), `None` si aucune.
///
/// Elle change avec `USE`, ou avec la base choisie dans la connexion enregistrée.
pub async fn current_database(connection: &mut Conn) -> Result<Option<String>, MysqlError> {
    let database: Option<Option<String>> = connection.query_first("SELECT DATABASE()").await?;
    Ok(database.flatten())
}

/// Change la base en cours (`USE`), par exemple pour qu'une connexion
/// supplémentaire reprenne la base choisie dans la session.
pub async fn use_database(connection: &mut Conn, database: &str) -> Result<(), MysqlError> {
    let quoted = database.replace('`', "``");
    connection.query_drop(format!("USE `{quoted}`")).await?;
    Ok(())
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

    /// Pour chaque colonne, ses valeurs s'alignent-elles à droite ? Oui pour
    /// les nombres (entier, décimal, flottant, année), les dates et les heures,
    /// d'après le type annoncé par le serveur avec le résultat.
    pub fn right_aligned_columns(&self) -> Vec<bool> {
        self.result
            .columns_ref()
            .iter()
            .map(|column| is_right_aligned(column.column_type()))
            .collect()
    }

    /// Type de chaque colonne, d'après les métadonnées du résultat (voir
    /// le module `column_type`).
    pub fn column_types(&self) -> Vec<ColumnTypeInfo> {
        self.result
            .columns_ref()
            .iter()
            .map(|column| {
                column_type::describe(
                    column.column_type(),
                    column.flags(),
                    column.column_length(),
                    column.decimals(),
                    column.character_set(),
                )
            })
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

/// Un type dont les valeurs s'alignent à droite : nombre, date ou heure.
fn is_right_aligned(column_type: ColumnType) -> bool {
    column_type.is_numeric_type()
        || matches!(
            column_type,
            ColumnType::MYSQL_TYPE_DATE
                | ColumnType::MYSQL_TYPE_NEWDATE
                | ColumnType::MYSQL_TYPE_DATETIME
                | ColumnType::MYSQL_TYPE_DATETIME2
                | ColumnType::MYSQL_TYPE_TIMESTAMP
                | ColumnType::MYSQL_TYPE_TIMESTAMP2
                | ColumnType::MYSQL_TYPE_TIME
                | ColumnType::MYSQL_TYPE_TIME2
        )
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
    fn aligns_numbers_dates_and_times_to_the_right() {
        assert!(is_right_aligned(ColumnType::MYSQL_TYPE_LONGLONG));
        assert!(is_right_aligned(ColumnType::MYSQL_TYPE_NEWDECIMAL));
        assert!(is_right_aligned(ColumnType::MYSQL_TYPE_DATE));
        assert!(is_right_aligned(ColumnType::MYSQL_TYPE_DATETIME));
        assert!(is_right_aligned(ColumnType::MYSQL_TYPE_TIME));
        assert!(!is_right_aligned(ColumnType::MYSQL_TYPE_VAR_STRING));
        assert!(!is_right_aligned(ColumnType::MYSQL_TYPE_BLOB));
    }

    #[test]
    fn detects_ansi_quotes_in_sql_mode() {
        assert!(has_ansi_quotes(
            "REAL_AS_FLOAT,PIPES_AS_CONCAT,ANSI_QUOTES,IGNORE_SPACE,ANSI"
        ));
        assert!(has_ansi_quotes("ANSI_QUOTES"));
        assert!(!has_ansi_quotes(
            "STRICT_TRANS_TABLES,NO_ENGINE_SUBSTITUTION"
        ));
        assert!(!has_ansi_quotes(""));
    }

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
