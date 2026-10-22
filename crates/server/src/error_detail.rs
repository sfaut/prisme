//! Errors told to the user, as data: a code and its parameters.
//!
//! Every error that reaches the interface carries an `ErrorDetail`, next to
//! its English text (`Display`), which the logs and the history of the
//! queries keep. The interface writes its own message from the code, in the
//! user's language (`web/src/api/messages.ts`). The texts that Prisme does
//! not write (the target server's answer, the driver's or the system's
//! message, the SQL parser's) travel as they are, in their own field, for
//! the interface to introduce them.

use std::fmt;

use prisme_connectors::mysql::{ConnectFailure, FailureCause, Flavor, MysqlError};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Field of an input checked by the server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum Field {
    /// Name of a connection, an editor or a result tab.
    Name,
    /// Host of a connection.
    Host,
    /// Account of a connection.
    Username,
    /// Database chosen for a connection.
    DatabaseName,
    /// Port of a connection.
    Port,
    /// Queries run when a connection opens.
    InitSql,
    /// Minutes a session stays open after its page closes.
    SessionTimeoutMinutes,
    /// First name of the user.
    FirstName,
    /// Last name of the user.
    LastName,
    /// Days the history of the queries is kept.
    HistoryRetentionDays,
    /// Days the debug messages are kept.
    DebugRetentionDays,
    /// Spaces of an indentation level in the SQL editor.
    IndentSize,
}

impl fmt::Display for Field {
    /// Name of the field in the API, for example `database_name`.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = serde_json::to_value(self)
            .ok()
            .and_then(|value| value.as_str().map(str::to_owned))
            .unwrap_or_default();
        formatter.write_str(&name)
    }
}

/// What went wrong, for the user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, thiserror::Error)]
#[ts(export)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum ErrorDetail {
    /// Missing resource, or one belonging to another user.
    #[error("resource not found")]
    NotFound,
    /// Internal error, detailed in the server's log only.
    #[error("internal error, see the server log")]
    Internal,
    /// The editor of a session has no connection.
    #[error("no connection chosen for this editor")]
    NoConnection,
    /// Text field too short or too long, in characters.
    #[error("{field}: between {min} and {max} characters expected, {length} received")]
    FieldLength {
        /// Field checked.
        field: Field,
        /// Characters at least.
        min: u32,
        /// Characters at most.
        max: u32,
        /// Characters received.
        length: u32,
    },
    /// Number out of its bounds.
    #[error("{field}: must be between {min} and {max}")]
    FieldRange {
        /// Field checked.
        field: Field,
        /// Smallest value allowed.
        min: u32,
        /// Largest value allowed.
        max: u32,
    },
    /// Row limit outside the list of choices.
    #[error("row_limit: {value} is not allowed, possible choices: {choices:?}")]
    RowLimitNotAllowed {
        /// Limit received.
        value: u32,
        /// Limits allowed.
        choices: Vec<u32>,
    },
    /// The user already has a connection with this name.
    #[error("a connection named \"{name}\" already exists")]
    DuplicateConnectionName {
        /// Name taken.
        name: String,
    },
    /// A query of `init_sql` is not understood by Prisme.
    #[error("init_sql: \"{statement}\": query not understood by Prisme")]
    InitSqlNotUnderstood {
        /// Query refused.
        statement: String,
    },
    /// A query of `init_sql` would change the whole server.
    #[error(
        "init_sql: \"{statement}\": a global setting would change the whole server; \
         only session settings are allowed"
    )]
    InitSqlGlobalSetting {
        /// Query refused.
        statement: String,
    },
    /// A query of `init_sql` is not a setting.
    #[error("init_sql: \"{statement}\": only session settings (SET ...) are allowed")]
    InitSqlNotSetting {
        /// Query refused.
        statement: String,
    },
    /// The connection to the target server could not open.
    #[error("{failure}")]
    Connect {
        /// Why.
        failure: ConnectFailure,
    },
    /// The target server returned no version.
    #[error("the server returned no version")]
    MissingVersion,
    /// A query of `init_sql` failed when the connection opened.
    #[error("on-connect query \"{statement}\" failed: {cause}")]
    InitQueryFailed {
        /// Query that failed.
        statement: String,
        /// Answer of the server, or message of the driver.
        cause: FailureCause,
    },
    /// A query failed on the target server, or its connection broke.
    #[error("{cause}")]
    QueryFailed {
        /// Flavor of the server, when the connection told it.
        flavor: Option<Flavor>,
        /// Answer of the server, or message of the driver or of the system.
        cause: FailureCause,
    },
    /// A read of the catalog failed.
    #[error("cannot read the catalog ({}): {cause}", schema.as_deref().unwrap_or("databases"))]
    CatalogUnreadable {
        /// Database being read, absent for the list of the databases.
        schema: Option<String>,
        /// Flavor of the server.
        flavor: Flavor,
        /// Answer of the server, or message of the driver.
        cause: FailureCause,
    },
    /// The user stopped the query.
    #[error("query interrupted by the user")]
    QueryInterrupted,
    /// The session ended before the query.
    #[error("session closed before the end of the query")]
    SessionClosed,
    /// The text to run holds no query.
    #[error("no query to run")]
    NoQuery,
    /// The text to run holds several queries.
    #[error("{count} queries received: one query at a time")]
    MultipleQueries {
        /// Queries received.
        count: u32,
    },
    /// A write, refused until the write switches exist.
    #[error("write refused: writes are disabled until the write switches exist")]
    WriteRefused,
    /// A query that Prisme cannot parse, so cannot prove to be a read.
    #[error("query refused: Prisme cannot parse it, so it cannot check that it is a read")]
    UnparsedQueryRefused,
    /// The settings of the user could not be read.
    #[error("unreadable user settings, see the server log")]
    SettingsUnreadable,
    /// The interface sent a message the server does not understand.
    #[error("invalid message: {cause}")]
    InvalidMessage {
        /// Message of the JSON reader.
        cause: String,
    },
    /// The SQL parser does not understand the query.
    #[error("Query not understood by Prisme: {cause}")]
    QueryNotUnderstood {
        /// Message of the SQL parser (`sqlparser`).
        cause: String,
    },
}

impl ErrorDetail {
    /// Detail of a failure of the MySQL / MariaDB connector.
    ///
    /// # Arguments
    ///
    /// * `error` - Failure of the connector.
    /// * `flavor` - Flavor of the server, when the connection told it.
    pub fn of_mysql(error: &MysqlError, flavor: Option<Flavor>) -> Self {
        match error {
            MysqlError::Connect(failure) => Self::Connect {
                failure: failure.clone(),
            },
            MysqlError::MissingVersion => Self::MissingVersion,
            MysqlError::Init { statement, cause } => Self::InitQueryFailed {
                statement: statement.clone(),
                cause: cause.clone(),
            },
            MysqlError::Driver(_) => Self::QueryFailed {
                flavor,
                cause: error.cause(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn carries_its_code_and_parameters() {
        let detail = ErrorDetail::FieldLength {
            field: Field::DatabaseName,
            min: 0,
            max: 64,
            length: 70,
        };
        assert_eq!(
            serde_json::to_value(&detail).unwrap(),
            serde_json::json!({
                "code": "field_length",
                "field": "database_name",
                "min": 0,
                "max": 64,
                "length": 70,
            })
        );
        assert_eq!(
            detail.to_string(),
            "database_name: between 0 and 64 characters expected, 70 received"
        );
    }

    #[test]
    fn keeps_the_text_of_the_target_server_apart() {
        let error = MysqlError::Init {
            statement: "SET NAMES utf8mb4".to_string(),
            cause: FailureCause::Server {
                number: 1115,
                sqlstate: "42000".to_string(),
                message: "Unknown character set: 'utf8mb4'".to_string(),
            },
        };
        let detail = ErrorDetail::of_mysql(&error, Some(Flavor::MariaDb));
        assert_eq!(
            serde_json::to_value(&detail).unwrap(),
            serde_json::json!({
                "code": "init_query_failed",
                "statement": "SET NAMES utf8mb4",
                "cause": {
                    "source": "server",
                    "number": 1115,
                    "sqlstate": "42000",
                    "message": "Unknown character set: 'utf8mb4'",
                },
            })
        );
        assert_eq!(
            detail.to_string(),
            "on-connect query \"SET NAMES utf8mb4\" failed: ERROR 1115 (42000): Unknown character set: 'utf8mb4'"
        );
    }
}
