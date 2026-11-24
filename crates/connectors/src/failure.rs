//! Failures of the connectors, as data: why a connection could not open
//! (`ConnectFailure`), and the text written outside Prisme that explains a
//! failure (`FailureCause`). A failure carries its data, not a sentence: the
//! interface writes its message in the user's language, and quotes as they
//! are the texts written elsewhere (the server's answer, the driver's or the
//! system's message). Their `Display` is the English text of the logs. Each
//! connector sorts the errors of its driver into these forms.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::location::LocationError;

/// Text written outside Prisme that explains a failure, as it was written.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, thiserror::Error)]
#[ts(export)]
#[serde(tag = "source", rename_all = "snake_case")]
pub enum FailureCause {
    /// Answer of the target server, in the language of its `lc_messages`.
    #[error("ERROR {number} ({sqlstate}): {message}")]
    Server {
        /// Error number of the server, for example 1146.
        number: u16,
        /// SQLSTATE of the error, for example `42S02`.
        sqlstate: String,
        /// Message of the server.
        message: String,
    },
    /// Answer of an engine that names its errors by a code (SQLite).
    #[error("{name} ({code}): {message}")]
    Engine {
        /// Code of the error in the engine, as text (`2067` for SQLite's
        /// `SQLITE_CONSTRAINT_UNIQUE`).
        code: String,
        /// Name of the code in the engine (`SQLITE_CONSTRAINT_UNIQUE`).
        name: String,
        /// Message of the engine, in its language.
        message: String,
    },
    /// Message of the driver (`mysql_async`, `rusqlite`), in English.
    #[error("{message}")]
    Driver {
        /// Message of the driver.
        message: String,
    },
    /// Message of the operating system (network), in its language.
    #[error("{message}")]
    System {
        /// Message of the system.
        message: String,
    },
}

/// Why a connection could not open. `address` is `host:port` for a server;
/// `location` is the location of a file, as typed (a path, a URL).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, thiserror::Error)]
#[ts(export)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum ConnectFailure {
    /// No server listens on the port, or it only accepts local connections.
    #[error(
        "{address} refuses the connection: no server listens on this port, \
         or it only accepts local connections (bind-address)"
    )]
    Refused {
        /// Address of the server.
        address: String,
    },
    /// The network does not reach the server.
    #[error("{address} is unreachable: server down, wrong address or firewall")]
    Unreachable {
        /// Address of the server.
        address: String,
    },
    /// The server did not answer within the connection delay.
    #[error(
        "{address} does not answer ({seconds} s timeout exceeded): server down, \
         wrong address or port, or firewall"
    )]
    Timeout {
        /// Address of the server.
        address: String,
        /// Connection delay, in seconds.
        seconds: u64,
    },
    /// Wrong password, or account not allowed from this machine.
    #[error(
        "access denied for the account \"{username}\": wrong password, or account not \
         allowed from this machine (the server answers: {cause})"
    )]
    AccessDenied {
        /// Connection account.
        username: String,
        /// Answer of the server.
        cause: FailureCause,
    },
    /// No account of this name is allowed from this machine.
    #[error(
        "the server refuses connections from this machine: no account \"{username}\" is \
         allowed from this address (the server answers: {cause})"
    )]
    HostNotAllowed {
        /// Connection account.
        username: String,
        /// Answer of the server.
        cause: FailureCause,
    },
    /// The database chosen for the connection does not exist.
    #[error("the database \"{database}\" does not exist on {address}")]
    UnknownDatabase {
        /// Database chosen for the connection.
        database: String,
        /// Address of the server.
        address: String,
    },
    /// The account has no access to the database chosen for the connection.
    #[error("the account \"{username}\" has no access to the database \"{database}\"")]
    DatabaseAccessDenied {
        /// Connection account.
        username: String,
        /// Database chosen for the connection.
        database: String,
    },
    /// The server refuses the connection for another reason.
    #[error("{address} refuses the connection: {cause}")]
    ServerRefused {
        /// Address of the server.
        address: String,
        /// Answer of the server.
        cause: FailureCause,
    },
    /// The account uses an authentication method the driver does not know.
    #[error("the account \"{username}\" uses an unsupported authentication method ({plugin})")]
    UnsupportedAuthentication {
        /// Connection account.
        username: String,
        /// Authentication plugin of the account.
        plugin: String,
    },
    /// The file of a database does not exist.
    #[error("the file \"{location}\" does not exist")]
    FileNotFound {
        /// Location of the file, as typed.
        location: String,
    },
    /// The file is not a database of the engine.
    #[error("\"{location}\" is not a database: {cause}")]
    NotADatabase {
        /// Location of the file, as typed.
        location: String,
        /// Message of the engine.
        cause: FailureCause,
    },
    /// The file could not be opened: access refused, folder...
    #[error("cannot open \"{location}\": {cause}")]
    FileUnreadable {
        /// Location of the file, as typed.
        location: String,
        /// Message of the engine or of the system.
        cause: FailureCause,
    },
    /// The location could not be reached: a remote file, its copy.
    #[error("{error}")]
    Location {
        /// Why the location could not be reached.
        error: LocationError,
    },
    /// Any other failure, told by the driver or the system.
    #[error("cannot connect to {address}: {cause}")]
    Other {
        /// Address of the server.
        address: String,
        /// Message of the driver or of the system.
        cause: FailureCause,
    },
}
