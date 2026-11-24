//! Contracts of the connectors: what the server asks of any engine, whatever
//! its driver (`docs/TODO-SQLITE.md`, section "Contracts").
//!
//! An engine is reached through its `Driver`, which the registry gives
//! (`drivers::of`): the driver tests a source (`probe`) and opens sessions on
//! it (`open`). A `Session` holds one connection to the source: it runs
//! statements, whose rows are read from a `Rows`, follows the transaction and
//! the current namespace, and gives a `CatalogReader`, which reads the
//! namespaces, the objects and the details of the tables of the source. A
//! `Canceller`, taken from the session, stops its running statement from
//! another task. Every engine implements these five traits, and the server
//! uses nothing else of it. What an engine can
//! do is told by its `Capabilities`, sent to the interface, which shows only
//! what the engine offers; an operation it cannot do fails with
//! `ConnectorError::Unsupported`.
//!
//! The methods return boxed futures: a trait with `async fn` cannot be used
//! behind `dyn` yet, and the server only knows the engines through `dyn`.
//! The server names no engine: it gets a driver from the registry, then
//! only uses these traits.

use std::sync::Arc;

use futures_util::future::BoxFuture;
pub use prisme_dialects::DialectOptions;
use prisme_dialects::classify::WriteKind;
use prisme_dialects::verb::StatementVerb;
use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::catalog::{NamespaceInfo, ObjectInfo, ObjectKind, TableDetails};
use crate::columns::{Cell, ColumnTypeInfo};
use crate::failure::{ConnectFailure, FailureCause};
use crate::flavor::Flavor;
use crate::location::{Location, LocationError, Remote};
use crate::outcome::{Notice, StatementOutcome};

/// Name of the schema of the main database of a source held in files.
pub const MAIN_SCHEMA: &str = "main";

/// Family of connector: one per native driver. An engine may serve several
/// flavors (MySQL and MariaDB through `mysql_async`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum Engine {
    /// MySQL and MariaDB, through `mysql_async`.
    Mysql,
    /// SQLite, embedded in Prisme, through `rusqlite`.
    Sqlite,
}

impl Engine {
    /// Every engine, as the reference table `engines` of the internal
    /// database lists them.
    pub const ALL: [Self; 2] = [Self::Mysql, Self::Sqlite];

    /// Key of the engine, as the internal database stores it
    /// (`connections.engine`).
    pub fn key(self) -> &'static str {
        match self {
            Self::Mysql => "mysql",
            Self::Sqlite => "sqlite",
        }
    }

    /// Family of the settings of its sources.
    pub fn family(self) -> Family {
        match self {
            Self::Mysql => Family::Server,
            Self::Sqlite => Family::Files,
        }
    }

    /// Engine of the key `key`, as the internal database stores it.
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|engine| engine.key() == key)
    }
}

/// Family of the settings of a source: what reaching it takes. Each family
/// has its table of settings in the internal database, and its form in the
/// interface; several engines can share one (SQLite and DuckDB, files).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Family {
    /// A server reached over the network: host, port, account.
    Server,
    /// Databases held in files, each at a location.
    Files,
}

impl Family {
    /// Every family, as the reference table `setting_families` of the
    /// internal database lists them.
    pub const ALL: [Self; 2] = [Self::Server, Self::Files];

    /// Key of the family, as the internal database stores it.
    pub fn key(self) -> &'static str {
        match self {
            Self::Server => "server",
            Self::Files => "files",
        }
    }
}

/// A source to reach: its engine, its settings, and the statements that
/// initialise each of its connections.
#[derive(Clone)]
pub struct Target {
    /// Engine of the source.
    pub engine: Engine,
    /// Settings of the source, by family.
    pub settings: Settings,
    /// Statements run when each connection opens, in order (`SET NAMES
    /// utf8mb4`, `SET SESSION sql_mode = ...`).
    pub init_statements: Vec<String>,
}

/// Settings of a source, by family of engines: the engines of a family share
/// their settings, and their table in the internal database.
#[derive(Clone)]
pub enum Settings {
    /// A database server reached over the network.
    Server(ServerSettings),
    /// Databases held in files.
    Files(FilesSettings),
}

/// Settings of a database server reached over the network.
#[derive(Clone)]
pub struct ServerSettings {
    /// Host name or IP address of the server.
    pub host: String,
    /// TCP port of the server.
    pub port: u16,
    /// Connection account.
    pub username: String,
    /// Password of the account, never shown in a log.
    pub password: SecretString,
    /// Database selected on connection. Empty string: none.
    pub database_name: String,
}

/// Settings of databases held in files: a main database, and the databases
/// attached to it in each session.
#[derive(Clone)]
pub struct FilesSettings {
    /// Databases, the main one first (schema `MAIN_SCHEMA`), then those
    /// attached, in the order of their attachment.
    pub databases: Vec<FileDatabase>,
    /// Where the remote databases are copied, and the limits of a download.
    pub remote: Remote,
}

/// Database of a source held in files: its schema in the sessions, and where
/// it is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct FileDatabase {
    /// Name of its schema in the sessions: `main` for the main database, the
    /// name of its `ATTACH` for another one.
    pub schema: String,
    /// Where the database is.
    pub location: Location,
}

impl Target {
    /// Namespace the settings open a session on, `None` if none: the
    /// database chosen for a server, the main database for files.
    pub fn namespace(&self) -> Option<Namespace> {
        match &self.settings {
            Settings::Server(server) => {
                (!server.database_name.is_empty()).then(|| Namespace::single(&server.database_name))
            }
            Settings::Files(_) => Some(Namespace::single(MAIN_SCHEMA)),
        }
    }
}

/// How a session opens.
#[derive(Debug, Clone, Default)]
pub struct OpenOptions {
    /// Read-only session: the connector makes the source refuse the writes
    /// (a statement of the session, a flag of the file...), after the
    /// statements of the target, so that they cannot lift it.
    pub read_only: bool,
    /// Namespace to open on, instead of the one of the settings: the current
    /// database of a session, for the connections of its parallel queries.
    pub namespace: Option<Namespace>,
    /// Name of the resources that the connections of one session share (a
    /// database in memory), so that its parallel queries see them.
    pub session_key: Option<String>,
}

/// Path of a namespace of a source, from the outermost level to the
/// innermost one: a database of MySQL (`["shop"]`), a schema of SQLite
/// (`["main"]`), a catalog and a schema of DuckDB (`["db", "main"]`). The
/// levels are those of `Capabilities::namespace_levels`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Namespace(pub Vec<String>);

impl Namespace {
    /// Namespace of a single level.
    pub fn single(name: impl Into<String>) -> Self {
        Self(vec![name.into()])
    }

    /// Name of the innermost level, the one statements see unqualified.
    pub fn name(&self) -> &str {
        self.0.last().map_or("", String::as_str)
    }
}

/// Level of the namespaces of an engine, for the labels of the interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum NamespaceLevel {
    /// Database of a server (MySQL, MariaDB).
    Database,
    /// Schema (SQLite, PostgreSQL, DuckDB).
    Schema,
    /// Catalog holding schemas (DuckDB, the standard).
    Catalog,
}

/// What an engine can do. Sent to the interface with the identity of the
/// source, so that it offers only what the engine supports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct Capabilities {
    /// Explicit transactions on the connection of a session.
    pub transactions: bool,
    /// A running statement can be stopped (`Session::canceller`).
    pub cancellation: bool,
    /// Several connections at once on the same source: the queries of a
    /// batch that only read run in parallel, each on its own connection.
    pub parallel_sessions: bool,
    /// The current namespace of a session can change (`USE`,
    /// `Session::switch_namespace`).
    pub switch_namespace: bool,
    /// Databases can be attached to a session and detached from it.
    pub attachments: bool,
    /// Levels of the namespaces, from the outermost one.
    pub namespace_levels: Vec<NamespaceLevel>,
    /// Kinds of objects the catalog lists, in the order of the interface.
    pub object_kinds: Vec<ObjectKind>,
    /// Kinds of write the engine knows: the settings name the engines of each
    /// switch.
    pub write_kinds: Vec<WriteKind>,
    /// Settings of a session that the editor and the SQL run at connection
    /// may change, when the engine keeps a closed list of them (the pragmas
    /// of the connection, SQLite); empty when any setting of the session is
    /// allowed (`SET SESSION`, MySQL).
    pub session_settings: Vec<String>,
    /// The source tells the notices of a statement (warnings, notes), read
    /// by `Session::notices`.
    pub notices: bool,
    /// The catalog gives an estimate of the rows of the tables.
    pub rows_estimate: bool,
}

/// Identity of a source, read when a session opens or a source is tested.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct SourceInfo {
    /// Product of the source.
    pub flavor: Flavor,
    /// Version, as the source writes it (`8.1.0`,
    /// `11.3.2-MariaDB-1:11.3.2+maria~ubu2204`).
    pub version: String,
    /// Lexical settings of the session, read on the source.
    #[serde(flatten)]
    pub dialect_options: DialectOptions,
    /// Oldest version of the flavor that Prisme supports
    /// (`Flavor::oldest_supported`), only when the source is older: it then
    /// gets a warning. `None` for a supported source.
    pub oldest_supported: Option<String>,
    /// What the engine of the source can do.
    pub capabilities: Capabilities,
    /// State of each database of a source held in files, in the order of
    /// its settings: the ones that could not be opened are left out of the
    /// session, which opens without them. Empty for a server.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[ts(as = "Option<Vec<DatabaseState>>", optional)]
    pub databases: Vec<DatabaseState>,
}

/// Database attached to the connection of a session, as the engine lists it
/// (`Session::databases`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct AttachedDatabase {
    /// Name of its schema.
    pub schema: String,
    /// Where it is, for another connection to attach it too; `None` for a
    /// database private to this connection (an `ATTACH ':memory:'` written
    /// in SQL).
    pub location: Option<Location>,
}

/// State of a database of a source held in files, when a session opens or
/// the source is tested.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct DatabaseState {
    /// Name of its schema in the session.
    pub schema: String,
    /// Where the database is.
    pub location: Location,
    /// Is it open, or why not.
    #[serde(flatten)]
    pub status: DatabaseStatus,
}

/// Is a database of a source held in files open, or why not.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum DatabaseStatus {
    /// Open in the session.
    Open {
        /// Size of the database, in bytes (its pages).
        #[ts(type = "number")]
        bytes: u64,
        /// Mode of its journal (`delete`, `wal`, `memory`...).
        journal_mode: String,
        /// Can it only be read: a file the process cannot write, a copy of a
        /// remote file, or a read-only session.
        read_only: bool,
    },
    /// Left out of the session.
    Unavailable {
        /// Why it could not be opened.
        failure: ConnectFailure,
    },
}

/// Errors of the connectors.
#[derive(Debug, Clone, thiserror::Error)]
pub enum ConnectorError {
    /// Connection impossible (refused, unreachable, no answer in time,
    /// access denied...): what is wrong and what to check.
    #[error("{0}")]
    Connect(ConnectFailure),

    /// A statement initialising the connection failed.
    #[error("on-connect query \"{statement}\" failed: {cause}")]
    Init {
        /// The failed statement.
        statement: String,
        /// Answer of the source, or message of the driver.
        cause: FailureCause,
    },

    /// The source gave no version.
    #[error("the server returned no version")]
    MissingVersion,

    /// Failure on an open connection: failed statement, lost connection...
    #[error("{0}")]
    Query(FailureCause),

    /// An operation the engine cannot do (outside its `Capabilities`).
    #[error("{0} is not supported by this engine")]
    Unsupported(&'static str),

    /// A location refused or out of reach, such as the file of a copy that
    /// exists already.
    #[error("{0}")]
    Location(LocationError),

    /// An `ATTACH` naming its file by its path alone, on a connection whose
    /// main database is in memory (SQLite): the file system of that database
    /// would give an empty database in memory instead of the file. `uri`
    /// names the file in the file system of the disk, which opens it.
    #[error("ATTACH of {path} refused: the main database is in memory, name the file {uri}")]
    AttachPathInMemory {
        /// Path written in the `ATTACH`.
        path: String,
        /// URI that opens the file on the disk.
        uri: String,
    },
}

impl ConnectorError {
    /// Text written outside Prisme that explains the failure: the source's
    /// answer, or the driver's or the system's message. A failure that Prisme
    /// itself describes gives its English text, as the driver's.
    pub fn cause(&self) -> FailureCause {
        match self {
            Self::Query(cause) | Self::Init { cause, .. } => cause.clone(),
            Self::Connect(_)
            | Self::MissingVersion
            | Self::Unsupported(_)
            | Self::Location(_)
            | Self::AttachPathInMemory { .. } => FailureCause::Driver {
                message: self.to_string(),
            },
        }
    }
}

/// Column of a result: its name, its type, and the side its values align to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnMeta {
    /// Name of the column.
    pub name: String,
    /// Type and origin of the column.
    pub type_info: ColumnTypeInfo,
    /// Are its values right-aligned (numbers, dates and times)?
    pub right_aligned: bool,
}

/// Access to an engine: tests a source and opens sessions on it.
pub trait Driver: Send + Sync {
    /// Engine of the driver.
    fn engine(&self) -> Engine;

    /// What the engine can do.
    fn capabilities(&self) -> Capabilities;

    /// Connects to the source, reads its identity, then disconnects. For a
    /// source held in files, the identity tells the state of each database,
    /// the main one included, even when a session could not open.
    fn probe<'a>(&'a self, target: &'a Target)
    -> BoxFuture<'a, Result<SourceInfo, ConnectorError>>;

    /// Opens a session on the source: one connection, initialised by the
    /// statements of the target, then set by `options`.
    fn open<'a>(
        &'a self,
        target: &'a Target,
        options: OpenOptions,
    ) -> BoxFuture<'a, Result<Box<dyn Session>, ConnectorError>>;
}

/// Connection open on a source.
pub trait Session: Send {
    /// Identity of the source, read when the session opened.
    fn info(&self) -> &SourceInfo;

    /// Runs the statement `sql` and returns its result, to read row by row.
    /// The reader traces the statement once read (`sql_log::record`): only
    /// it knows the transfer time and the end of the result.
    fn execute<'a>(
        &'a mut self,
        sql: &'a str,
    ) -> BoxFuture<'a, Result<Box<dyn Rows<'a> + 'a>, ConnectorError>>;

    /// Current namespace of the session, `None` if none.
    fn current_namespace(&mut self) -> BoxFuture<'_, Result<Option<Namespace>, ConnectorError>>;

    /// Changes the current namespace of the session.
    fn switch_namespace<'a>(
        &'a mut self,
        namespace: &'a Namespace,
    ) -> BoxFuture<'a, Result<(), ConnectorError>>;

    /// Is a transaction open on the session? With `check`, the connector
    /// makes sure the connection is still open (after a `COMMIT` or a
    /// `ROLLBACK`, which can close it).
    ///
    /// # Errors
    ///
    /// Fails if the connection is lost.
    fn transaction_state(&mut self, check: bool) -> BoxFuture<'_, Result<bool, ConnectorError>>;

    /// Notices (warnings, notes) of the last statement, which its outcome
    /// only counts: to read before the next statement, which clears them.
    fn notices(&mut self) -> BoxFuture<'_, Result<Vec<Notice>, ConnectorError>>;

    /// Attaches `database` to the session, for the session only, and tells
    /// its state: open, or why not. Only for an engine with
    /// `Capabilities::attachments`.
    fn attach<'a>(
        &'a mut self,
        _database: &'a FileDatabase,
    ) -> BoxFuture<'a, Result<DatabaseState, ConnectorError>> {
        Box::pin(async { Err(ConnectorError::Unsupported("attaching a database")) })
    }

    /// Detaches the database of the schema `schema` from the session. Only
    /// for an engine with `Capabilities::attachments`.
    fn detach<'a>(&'a mut self, _schema: &'a str) -> BoxFuture<'a, Result<(), ConnectorError>> {
        Box::pin(async { Err(ConnectorError::Unsupported("detaching a database")) })
    }

    /// Databases attached to the connection of the session now, the main
    /// one first, in the order of their attachment, `temp` left out: those
    /// of its opening and of `attach`, and those of the `ATTACH` written in
    /// SQL. Only for an engine with `Capabilities::attachments`.
    fn databases(&mut self) -> BoxFuture<'_, Result<Vec<AttachedDatabase>, ConnectorError>> {
        Box::pin(async {
            Err(ConnectorError::Unsupported(
                "listing the attached databases",
            ))
        })
    }

    /// Copies the database of the schema `schema` into a new file at
    /// `location`, which must not exist (a database in memory saved). Only
    /// for an engine with `Capabilities::attachments`.
    fn backup<'a>(
        &'a mut self,
        _schema: &'a str,
        _location: &'a Location,
    ) -> BoxFuture<'a, Result<(), ConnectorError>> {
        Box::pin(async { Err(ConnectorError::Unsupported("copying a database")) })
    }

    /// Stops the running statement of the session, from another task.
    fn canceller(&self) -> Arc<dyn Canceller>;

    /// Reader of the catalog of the source, through the connection of the
    /// session, which it borrows until it is dropped.
    fn catalog(&mut self) -> Box<dyn CatalogReader + '_>;

    /// Closes the session.
    fn close(self: Box<Self>) -> BoxFuture<'static, ()>;
}

/// Catalog of a source, read part by part for the navigator: the namespaces,
/// then the objects of each one, then the details of its tables. Each engine
/// reads it from its own tables (`information_schema` for MySQL), and gives
/// it in the neutral form of `crate::catalog`.
pub trait CatalogReader: Send {
    /// Namespaces of the source, by path.
    fn namespaces(&mut self) -> BoxFuture<'_, Result<Vec<NamespaceInfo>, ConnectorError>>;

    /// Objects of the namespace `namespace`: tables, views, routines...
    fn objects<'a>(
        &'a mut self,
        namespace: &'a Namespace,
    ) -> BoxFuture<'a, Result<Vec<ObjectInfo>, ConnectorError>>;

    /// Details of the tables of the namespace `namespace`: columns, indexes,
    /// foreign keys, triggers.
    fn details<'a>(
        &'a mut self,
        namespace: &'a Namespace,
    ) -> BoxFuture<'a, Result<Vec<TableDetails>, ConnectorError>>;
}

/// Result of a statement being read, row by row. Borrows its session until
/// it is dropped or discarded.
pub trait Rows<'a>: Send {
    /// Columns of the result. Empty for a statement that returns no rows.
    fn columns(&self) -> &[ColumnMeta];

    /// Reads the next row, or `None` at the end of the result. A read must
    /// not be dropped before it ends: it may hold a part of a message of
    /// the protocol.
    fn next_row(&mut self) -> BoxFuture<'_, Result<Option<Vec<Cell>>, ConnectorError>>;

    /// Outcome of the statement of verb `verb`, in its neutral form: to read
    /// once the rows are read.
    fn outcome(&self, verb: StatementVerb) -> StatementOutcome;

    /// Gives up reading: the rest of the result is read and dropped, so that
    /// the session becomes usable again. After a stop, the error of the stop
    /// is ignored.
    fn discard(self: Box<Self>) -> BoxFuture<'a, ()>;
}

/// Stops the running statement of a session, from another task than the one
/// that reads its rows.
pub trait Canceller: Send + Sync {
    /// Interrupts the running statement; the session stays open and usable.
    fn interrupt(&self) -> BoxFuture<'_, Result<(), ConnectorError>>;

    /// Ends the session on the source, which stops its running statement,
    /// even when the order arrives before the statement.
    fn terminate(&self) -> BoxFuture<'_, Result<(), ConnectorError>>;
}
