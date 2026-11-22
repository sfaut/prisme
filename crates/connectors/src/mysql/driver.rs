//! The MySQL / MariaDB connector behind the contracts of `crate::api`: the
//! driver (`DRIVER`), its sessions, the rows of their results and the
//! stopping of their statements. The work itself stays in the functions of
//! the `mysql` module; this module only puts them behind the traits, and
//! applies the read-only mode of a session.

use std::sync::Arc;

use futures_util::future::BoxFuture;
use mysql_async::Conn;
use prisme_dialects::classify::WriteKind;
use prisme_dialects::verb::StatementVerb;

use super::catalog;
use super::{
    MysqlError, MysqlTarget, ResultStream, ServerInfo, connect, current_database, execute,
    in_transaction, kill_connection, kill_query, read_warnings, server_info, use_database,
};
use crate::api::{
    Canceller, Capabilities, ColumnMeta, ConnectorError, DialectOptions, Driver, Engine, Namespace,
    NamespaceLevel, OpenOptions, Rows, Session, Settings, SourceInfo, Target,
};
use crate::catalog::{NamespaceInfo, ObjectInfo, ObjectKind, TableDetails};
use crate::columns::Cell;
use crate::failure::FailureCause;
use crate::outcome::{Notice, StatementOutcome};

/// Driver of MySQL and MariaDB, for the registry (`drivers::of`).
pub static DRIVER: MysqlDriver = MysqlDriver;

/// Statement that puts a connection in read-only mode: the server then
/// refuses the writes of its transactions (`docs/V1.md`, section 2.4).
///
/// The standard statement, known by every version of MySQL and MariaDB. The
/// variable it sets is named `transaction_read_only` in MySQL and from
/// MariaDB 11.1 only, `tx_read_only` before: `SET SESSION
/// transaction_read_only = ON` fails on MariaDB 10.x and 11.0 (error 1193).
const READ_ONLY_STATEMENT: &str = "SET SESSION TRANSACTION READ ONLY";

/// Driver of MySQL and MariaDB, through `mysql_async`.
pub struct MysqlDriver;

impl Driver for MysqlDriver {
    fn engine(&self) -> Engine {
        Engine::Mysql
    }

    fn capabilities(&self) -> Capabilities {
        capabilities()
    }

    fn probe<'a>(
        &'a self,
        target: &'a Target,
    ) -> BoxFuture<'a, Result<SourceInfo, ConnectorError>> {
        Box::pin(async move {
            let target = mysql_target(target, &OpenOptions::default());
            let mut connection = connect(&target).await?;
            let info = server_info(&mut connection).await;
            // The identity is read: a failure to disconnect changes nothing.
            let _ = connection.disconnect().await;
            Ok(source_info(info?))
        })
    }

    fn open<'a>(
        &'a self,
        target: &'a Target,
        options: OpenOptions,
    ) -> BoxFuture<'a, Result<Box<dyn Session>, ConnectorError>> {
        Box::pin(async move {
            let target = mysql_target(target, &options);
            let mut connection = connect(&target).await?;
            let info = match server_info(&mut connection).await {
                Ok(info) => source_info(info),
                Err(error) => {
                    let _ = connection.disconnect().await;
                    return Err(error.into());
                }
            };
            let session = MysqlSession {
                connection,
                target,
                info,
            };
            Ok(Box::new(session) as Box<dyn Session>)
        })
    }
}

/// What MySQL and MariaDB can do.
pub fn capabilities() -> Capabilities {
    Capabilities {
        transactions: true,
        cancellation: true,
        parallel_sessions: true,
        switch_namespace: true,
        attachments: false,
        namespace_levels: vec![NamespaceLevel::Database],
        object_kinds: vec![
            ObjectKind::Table,
            ObjectKind::View,
            ObjectKind::SystemTable,
            ObjectKind::Procedure,
            ObjectKind::Function,
            ObjectKind::Event,
        ],
        write_kinds: WriteKind::ALL.to_vec(),
        notices: true,
        rows_estimate: true,
    }
}

/// Identity of a server in its neutral form.
pub fn source_info(info: ServerInfo) -> SourceInfo {
    SourceInfo {
        flavor: info.flavor,
        version: info.version,
        dialect_options: DialectOptions {
            ansi_quotes: info.ansi_quotes,
            no_backslash_escapes: info.no_backslash_escapes,
        },
        oldest_supported: info.oldest_supported,
        capabilities: capabilities(),
    }
}

/// Settings of a connection to the server of `target`, set by `options`:
/// the namespace asked instead of the database of the settings, and the
/// read-only mode after the statements of the target, so that they cannot
/// switch it off.
fn mysql_target(target: &Target, options: &OpenOptions) -> MysqlTarget {
    let Settings::Server(server) = &target.settings;
    let mut init_statements = target.init_statements.clone();
    if options.read_only {
        init_statements.push(READ_ONLY_STATEMENT.to_string());
    }
    MysqlTarget {
        host: server.host.clone(),
        port: server.port,
        username: server.username.clone(),
        password: server.password.clone(),
        database_name: options.namespace.as_ref().map_or_else(
            || server.database_name.clone(),
            |namespace| namespace.name().to_string(),
        ),
        init_statements,
    }
}

impl From<MysqlError> for ConnectorError {
    /// Sorts a failure of the connector into its neutral form.
    fn from(error: MysqlError) -> Self {
        match error {
            MysqlError::Connect(failure) => Self::Connect(failure),
            MysqlError::Driver(error) => Self::Query(FailureCause::from(&error)),
            MysqlError::MissingVersion => Self::MissingVersion,
            MysqlError::Init { statement, cause } => Self::Init { statement, cause },
        }
    }
}

/// Session on a MySQL or MariaDB server: one connection.
struct MysqlSession {
    /// Connection to the server.
    connection: Conn,
    /// Settings the connection opened with, for the connection of a stop.
    target: MysqlTarget,
    /// Identity of the server, read when the session opened.
    info: SourceInfo,
}

impl Session for MysqlSession {
    fn info(&self) -> &SourceInfo {
        &self.info
    }

    fn execute<'a>(
        &'a mut self,
        sql: &'a str,
    ) -> BoxFuture<'a, Result<Box<dyn Rows<'a> + 'a>, ConnectorError>> {
        Box::pin(async move {
            let stream = execute(&mut self.connection, sql).await?;
            let columns = columns_of(&stream);
            Ok(Box::new(MysqlRows { stream, columns }) as Box<dyn Rows<'a> + 'a>)
        })
    }

    fn current_namespace(&mut self) -> BoxFuture<'_, Result<Option<Namespace>, ConnectorError>> {
        Box::pin(async move {
            let database = current_database(&mut self.connection).await?;
            Ok(database.map(Namespace::single))
        })
    }

    fn switch_namespace<'a>(
        &'a mut self,
        namespace: &'a Namespace,
    ) -> BoxFuture<'a, Result<(), ConnectorError>> {
        Box::pin(async move { Ok(use_database(&mut self.connection, namespace.name()).await?) })
    }

    fn transaction_state(&mut self, check: bool) -> BoxFuture<'_, Result<bool, ConnectorError>> {
        Box::pin(async move { Ok(in_transaction(&mut self.connection, check).await?) })
    }

    fn notices(&mut self) -> BoxFuture<'_, Result<Vec<Notice>, ConnectorError>> {
        Box::pin(async move { Ok(read_warnings(&mut self.connection).await?) })
    }

    fn canceller(&self) -> Arc<dyn Canceller> {
        Arc::new(MysqlCanceller {
            target: self.target.clone(),
            connection_id: self.connection.id(),
        })
    }

    fn namespaces(&mut self) -> BoxFuture<'_, Result<Vec<NamespaceInfo>, ConnectorError>> {
        Box::pin(async move {
            let schemas = catalog::schemas(&mut self.connection).await?;
            Ok(schemas
                .into_iter()
                .map(|schema| NamespaceInfo {
                    system: catalog::SYSTEM_SCHEMAS.contains(&schema.as_str()),
                    namespace: Namespace::single(schema),
                })
                .collect())
        })
    }

    fn objects<'a>(
        &'a mut self,
        namespace: &'a Namespace,
    ) -> BoxFuture<'a, Result<Vec<ObjectInfo>, ConnectorError>> {
        Box::pin(async move { Ok(catalog::objects(&mut self.connection, namespace.name()).await?) })
    }

    fn details<'a>(
        &'a mut self,
        namespace: &'a Namespace,
    ) -> BoxFuture<'a, Result<Vec<TableDetails>, ConnectorError>> {
        Box::pin(async move { Ok(catalog::details(&mut self.connection, namespace.name()).await?) })
    }

    fn close(self: Box<Self>) -> BoxFuture<'static, ()> {
        Box::pin(async move {
            // The session ends: a failure to disconnect changes nothing.
            let _ = self.connection.disconnect().await;
        })
    }
}

/// Columns of a result, from the metadata the server sent with it.
fn columns_of(stream: &ResultStream<'_>) -> Vec<ColumnMeta> {
    let names = stream.column_names();
    let aligned = stream.right_aligned_columns();
    let types = stream.column_types();
    names
        .into_iter()
        .zip(aligned)
        .zip(types)
        .map(|((name, right_aligned), type_info)| ColumnMeta {
            name,
            type_info,
            right_aligned,
        })
        .collect()
}

/// Result of a statement being read.
struct MysqlRows<'a> {
    /// Result of the driver.
    stream: ResultStream<'a>,
    /// Its columns, read once.
    columns: Vec<ColumnMeta>,
}

impl<'a> Rows<'a> for MysqlRows<'a> {
    fn columns(&self) -> &[ColumnMeta] {
        &self.columns
    }

    fn next_row(&mut self) -> BoxFuture<'_, Result<Option<Vec<Cell>>, ConnectorError>> {
        Box::pin(async move { Ok(self.stream.next_row().await?) })
    }

    fn outcome(&self, verb: StatementVerb) -> StatementOutcome {
        self.stream.outcome(verb)
    }

    fn discard(self: Box<Self>) -> BoxFuture<'a, ()> {
        Box::pin(self.stream.discard())
    }
}

/// Stop of the statement of a session, through a second connection: the
/// first one is busy reading.
struct MysqlCanceller {
    /// Settings of the session's connection.
    target: MysqlTarget,
    /// Id of the session's connection on the server (`Conn::id()`).
    connection_id: u32,
}

impl Canceller for MysqlCanceller {
    fn interrupt(&self) -> BoxFuture<'_, Result<(), ConnectorError>> {
        Box::pin(async move { Ok(kill_query(&self.target, self.connection_id).await?) })
    }

    fn terminate(&self) -> BoxFuture<'_, Result<(), ConnectorError>> {
        Box::pin(async move { Ok(kill_connection(&self.target, self.connection_id).await?) })
    }
}

#[cfg(test)]
mod tests {
    use secrecy::SecretString;

    use super::*;
    use crate::api::ServerSettings;

    fn target() -> Target {
        Target {
            engine: Engine::Mysql,
            settings: Settings::Server(ServerSettings {
                host: "localhost".to_string(),
                port: 3306,
                username: "app".to_string(),
                password: SecretString::from(""),
                database_name: "shop".to_string(),
            }),
            init_statements: vec!["SET NAMES utf8mb4".to_string()],
        }
    }

    #[test]
    fn read_only_comes_last_and_only_when_asked() {
        let read_only = OpenOptions {
            read_only: true,
            ..OpenOptions::default()
        };
        assert_eq!(
            mysql_target(&target(), &read_only).init_statements,
            ["SET NAMES utf8mb4", READ_ONLY_STATEMENT]
        );
        assert_eq!(
            mysql_target(&target(), &OpenOptions::default()).init_statements,
            ["SET NAMES utf8mb4"]
        );
    }

    #[test]
    fn opens_on_the_namespace_asked_or_on_the_database_of_the_settings() {
        let elsewhere = OpenOptions {
            namespace: Some(Namespace::single("xlist")),
            ..OpenOptions::default()
        };
        assert_eq!(mysql_target(&target(), &elsewhere).database_name, "xlist");
        assert_eq!(
            mysql_target(&target(), &OpenOptions::default()).database_name,
            "shop"
        );
    }
}
