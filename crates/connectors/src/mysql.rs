//! MySQL and MariaDB connector, with the `mysql_async` driver.
//!
//! Both engines share the same protocol and the same driver, but Prisme
//! treats them as two distinct dialects (rule 1 of `CLAUDE.md`). The flavor
//! is detected on connection through `SELECT VERSION()`, whose result
//! contains "MariaDB" for the latter.
//!
//! Results are read row by row (`execute`, then `ResultStream`), never
//! loading the whole result in memory. The catalog (databases, tables,
//! columns) is in the `catalog` submodule, the type of a result's columns in
//! `column_type`. Each statement sent is traced for the history of the
//! queries (`sql_log`).

use std::time::Duration;

use mysql_async::consts::ColumnType;
use mysql_async::prelude::Queryable;
use mysql_async::{Conn, OptsBuilder, QueryResult, TextProtocol, Value};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Open connection to a MySQL or MariaDB server.
pub use mysql_async::Conn as Connection;

pub mod catalog;
mod column_type;
mod connect_error;
pub mod sql_log;

pub use column_type::{ColumnTypeInfo, TypeFamily};

/// Maximum delay to open a connection to the server.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Connection settings for a MySQL or MariaDB server.
#[derive(Clone)]
pub struct MysqlTarget {
    /// Host name or IP address of the server.
    pub host: String,
    /// TCP port of the server (usually 3306).
    pub port: u16,
    /// Connection account.
    pub username: String,
    /// Password of the account, never shown in a log.
    pub password: SecretString,
    /// Database selected on connection. Empty string: none.
    pub database_name: String,
    /// Queries run when each connection opens, in order
    /// (`SET NAMES utf8mb4`, `SET SESSION sql_mode = ...`).
    pub init_statements: Vec<String>,
}

/// Flavor of the server, detected on connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "lowercase")]
pub enum Flavor {
    /// MySQL server (Oracle).
    MySql,
    /// MariaDB server.
    MariaDb,
}

/// Identity of a server, obtained on connection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct ServerInfo {
    /// MySQL or MariaDB.
    pub flavor: Flavor,
    /// Raw result of `SELECT VERSION()`, for example `8.1.0` or `11.3.2-MariaDB-1:11.3.2+maria~ubu2204`.
    pub version: String,
    /// Is the `ANSI_QUOTES` mode active on the session (directly, or through
    /// the `ANSI` mode)? Double quotes then delimit identifiers, no longer
    /// strings.
    pub ansi_quotes: bool,
}

/// Errors of the MySQL / MariaDB connector.
#[derive(Debug, thiserror::Error)]
pub enum MysqlError {
    /// Connection impossible, with a message that says what to check.
    #[error("{0}")]
    Connect(String),

    /// The server did not answer within `CONNECT_TIMEOUT`.
    #[error("{0}")]
    ConnectTimeout(String),

    /// Driver error on an open connection: failed query, lost connection...
    #[error("{0}")]
    Driver(#[from] mysql_async::Error),

    /// The server returned no version.
    #[error("the server returned no version")]
    MissingVersion,

    /// A query initialising the connection failed.
    #[error("on-connect query \"{statement}\" failed: {message}")]
    Init {
        /// The failed query.
        statement: String,
        /// Message of the server.
        message: String,
    },
}

/// Deduces the server's flavor from the result of `SELECT VERSION()`.
pub fn detect_flavor(version: &str) -> Flavor {
    if version.to_ascii_lowercase().contains("mariadb") {
        Flavor::MariaDb
    } else {
        Flavor::MySql
    }
}

/// Opens a connection to the server, within `CONNECT_TIMEOUT` at most, then
/// runs the target's initialisation queries on it, in order.
///
/// On failure, the message says what is wrong (server unreachable, access
/// denied, unknown database, initialisation query refused...) and what to
/// check.
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
        let sent = connection.query_drop(statement.as_str());
        if let Err(error) = sql_log::traced(statement, sent, |_| None).await {
            let _ = connection.disconnect().await;
            return Err(MysqlError::Init {
                statement: statement.clone(),
                message: error.to_string(),
            });
        }
    }
    Ok(connection)
}

/// Reads the server's identity on an open connection.
pub async fn server_info(connection: &mut Conn) -> Result<ServerInfo, MysqlError> {
    let sql = "SELECT VERSION(), @@SESSION.sql_mode";
    let row: Option<(String, String)> =
        sql_log::traced(sql, connection.query_first(sql), first_row).await?;
    let (version, sql_mode) = row.ok_or(MysqlError::MissingVersion)?;
    Ok(ServerInfo {
        flavor: detect_flavor(&version),
        ansi_quotes: has_ansi_quotes(&sql_mode),
        version,
    })
}

/// Does `sql_mode` contain `ANSI_QUOTES`? The server expands the `ANSI` mode
/// into its components in `@@sql_mode`, `ANSI_QUOTES` included.
fn has_ansi_quotes(sql_mode: &str) -> bool {
    sql_mode
        .split(',')
        .any(|mode| mode.trim().eq_ignore_ascii_case("ANSI_QUOTES"))
}

/// Current database on an open connection (`SELECT DATABASE()`), `None` if none.
///
/// It changes with `USE`, or with the database chosen in the saved connection.
pub async fn current_database(connection: &mut Conn) -> Result<Option<String>, MysqlError> {
    let sql = "SELECT DATABASE()";
    let database: Option<Option<String>> =
        sql_log::traced(sql, connection.query_first(sql), first_row).await?;
    Ok(database.flatten())
}

/// Changes the current database (`USE`), for example so that an extra
/// connection takes the database chosen in the session.
pub async fn use_database(connection: &mut Conn, database: &str) -> Result<(), MysqlError> {
    let sql = format!("USE `{}`", database.replace('`', "``"));
    sql_log::traced(&sql, connection.query_drop(&sql), |_| None).await?;
    Ok(())
}

/// Connects to the server, reads its identity, then disconnects.
pub async fn probe(target: &MysqlTarget) -> Result<ServerInfo, MysqlError> {
    let mut connection = connect(target).await?;
    let info = server_info(&mut connection).await?;
    connection.disconnect().await?;
    Ok(info)
}

/// Interrupts the running query on connection `connection_id` (`Conn::id()`).
///
/// Goes through a second connection, as the first one is busy with the query.
/// The interrupted connection stays open and reusable.
pub async fn kill_query(target: &MysqlTarget, connection_id: u32) -> Result<(), MysqlError> {
    let mut connection = connect(target).await?;
    let sql = format!("KILL QUERY {connection_id}");
    sql_log::traced(&sql, connection.query_drop(&sql), |_| None).await?;
    connection.disconnect().await?;
    Ok(())
}

/// Number of rows of a `query_first` result: 1 or 0.
fn first_row<T>(row: &Option<T>) -> Option<u64> {
    Some(u64::from(row.is_some()))
}

/// Value of a result cell: `None` for NULL, otherwise text.
///
/// Everything is sent as text, numbers included: no loss of precision on a
/// `BIGINT UNSIGNED` or a `DECIMAL`. Fine typing will come with the `types`
/// crate.
pub type Cell = Option<String>;

/// Result of a query being read, row by row.
pub struct ResultStream<'a> {
    /// Driver result, which keeps the connection borrowed until the end of the reading.
    result: QueryResult<'a, 'static, TextProtocol>,
}

/// Runs the query `sql` and returns its result, to read row by row. The
/// reader traces the statement once read (`sql_log::record`): only it knows
/// the transfer time and the end of the result.
pub async fn execute<'a>(
    connection: &'a mut Conn,
    sql: &'a str,
) -> Result<ResultStream<'a>, MysqlError> {
    let result = connection.query_iter(sql).await?;
    Ok(ResultStream { result })
}

impl ResultStream<'_> {
    /// Column names. Empty for a query that returns no rows.
    pub fn column_names(&self) -> Vec<String> {
        self.result
            .columns_ref()
            .iter()
            .map(|column| column.name_str().into_owned())
            .collect()
    }

    /// For each column, are its values right-aligned? Yes for numbers
    /// (integer, decimal, floating, year), dates and times, from the type
    /// announced by the server with the result.
    pub fn right_aligned_columns(&self) -> Vec<bool> {
        self.result
            .columns_ref()
            .iter()
            .map(|column| is_right_aligned(column.column_type()))
            .collect()
    }

    /// Type of each column, from the result's metadata (see the
    /// `column_type` module).
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

    /// Reads the next row, or `None` at the end of the result.
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

    /// Number of affected rows, for a write.
    pub fn affected_rows(&self) -> u64 {
        self.result.affected_rows()
    }

    /// Gives up reading: the remaining rows are read and dropped by the
    /// driver, so that the connection becomes usable again. After a
    /// `kill_query`, the server sends no more and the interruption error is
    /// ignored.
    pub async fn discard(self) {
        // The error expected here is the interruption's: nothing to do with it.
        let _ = self.result.drop_result().await;
    }
}

/// A type whose values are right-aligned: number, date or time.
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

/// Converts a driver value to text, or `None` for NULL.
fn cell(value: Value) -> Cell {
    match value {
        Value::NULL => None,
        Value::Bytes(bytes) => Some(match String::from_utf8(bytes) {
            Ok(text) => text,
            // Binary data (BLOB, BINARY...): shown in hexadecimal, as MySQL does.
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

/// Writes bytes in uppercase hexadecimal.
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
