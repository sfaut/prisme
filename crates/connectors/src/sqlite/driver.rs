//! The SQLite connector behind the contracts of `api`: its driver, its
//! sessions, the rows of their statements and their canceller.
//!
//! A session sends its orders to the thread of its connection (`blocking`)
//! and awaits their answers. Its canceller interrupts the running statement
//! (`sqlite3_interrupt`, callable from any thread); ending the session
//! (`terminate`) also marks it ended, so that the orders still waiting fail
//! instead of running, even one that arrives after the interruption.

use std::marker::PhantomData;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use futures_util::future::BoxFuture;
use prisme_dialects::classify::WriteKind;
use prisme_dialects::verb::StatementVerb;
use rusqlite::{Connection, InterruptHandle};
use tokio::sync::{mpsc, oneshot};

use super::catalog;
use super::failure::opening;
use super::open::{Planned, connect, file_name, plan, prepare, private_key};
use super::statement::{self, Ending, Message, ROWS_AHEAD};
use crate::api::{
    Canceller, Capabilities, CatalogReader, ColumnMeta, ConnectorError, DialectOptions, Driver,
    Engine, MAIN_SCHEMA, Namespace, NamespaceLevel, OpenOptions, Rows, Session, Settings,
    SourceInfo, Target,
};
use crate::blocking::{Stopped, Worker};
use crate::catalog::{NamespaceInfo, ObjectInfo, ObjectKind, TableDetails};
use crate::columns::Cell;
use crate::failure::FailureCause;
use crate::flavor::Flavor;
use crate::outcome::{Notice, StatementOutcome};

/// Driver of SQLite.
pub static DRIVER: SqliteDriver = SqliteDriver;

/// Driver of SQLite, embedded in Prisme.
pub struct SqliteDriver;

impl Driver for SqliteDriver {
    fn engine(&self) -> Engine {
        Engine::Sqlite
    }

    fn capabilities(&self) -> Capabilities {
        capabilities()
    }

    fn probe<'a>(
        &'a self,
        target: &'a Target,
    ) -> BoxFuture<'a, Result<SourceInfo, ConnectorError>> {
        Box::pin(async move {
            let session = open(target, OpenOptions::default()).await?;
            Ok(session.info)
        })
    }

    fn open<'a>(
        &'a self,
        target: &'a Target,
        options: OpenOptions,
    ) -> BoxFuture<'a, Result<Box<dyn Session>, ConnectorError>> {
        Box::pin(async move {
            let session: Box<dyn Session> = Box::new(open(target, options).await?);
            Ok(session)
        })
    }
}

/// What SQLite can do. A session sees only the schemas of its connection,
/// without changing its current one: `main` stays first for the names it
/// does not qualify, after `temp`.
pub fn capabilities() -> Capabilities {
    use WriteKind::*;
    Capabilities {
        transactions: true,
        cancellation: true,
        parallel_sessions: true,
        switch_namespace: false,
        attachments: true,
        namespace_levels: vec![NamespaceLevel::Schema],
        object_kinds: vec![ObjectKind::Table, ObjectKind::View, ObjectKind::SystemTable],
        write_kinds: vec![
            Insert,
            Update,
            Delete,
            CreateTable,
            AlterTable,
            DropTable,
            CreateTemporaryTable,
            AlterTemporaryTable,
            DropTemporaryTable,
            CreateView,
            DropView,
            Attach,
            Maintenance,
        ],
        notices: false,
        rows_estimate: false,
    }
}

/// Opens a session on the databases of `target`.
async fn open(target: &Target, options: OpenOptions) -> Result<SqliteSession, ConnectorError> {
    let Settings::Files(files) = &target.settings else {
        return Err(ConnectorError::Unsupported("a source on a server"));
    };
    let main_namespace = Namespace::single(MAIN_SCHEMA);
    if options
        .namespace
        .as_ref()
        .is_some_and(|namespace| *namespace != main_namespace)
    {
        return Err(ConnectorError::Unsupported(
            "a session opened on another schema than main",
        ));
    }
    let mut planned: Vec<Planned> = plan(files).await;
    if planned.is_empty() {
        return Err(ConnectorError::Unsupported(
            "a source without a main database",
        ));
    }
    let main = planned.remove(0);
    let local = main.local.map_err(ConnectorError::Connect)?;
    let key = options.session_key.unwrap_or_else(private_key);
    let name = file_name(&local, MAIN_SCHEMA, &key);
    let read_only = options.read_only;
    let location = main.database.location.clone();
    let worker = Worker::spawn("prisme-sqlite", move || {
        connect(&name, read_only)
            .map_err(|error| ConnectorError::Connect(opening(&location, &error)))
    })
    .await??;
    let init_statements = target.init_statements.clone();
    let database = main.database;
    let prepared = worker
        .run(move |connection: &mut Connection| {
            prepare(connection, &database, &planned, &key, &init_statements)
        })
        .await??;
    let info = SourceInfo {
        flavor: Flavor::Sqlite,
        version: rusqlite::version().to_string(),
        dialect_options: DialectOptions::default(),
        oldest_supported: None,
        capabilities: capabilities(),
        databases: prepared.databases,
    };
    Ok(SqliteSession {
        worker,
        info,
        canceller: Arc::new(SqliteCanceller {
            handle: prepared.handle,
            ended: Arc::new(AtomicBool::new(false)),
        }),
    })
}

/// Error of an order that arrives once the session was ended.
fn ended() -> ConnectorError {
    ConnectorError::Query(FailureCause::Driver {
        message: "the session was ended".to_string(),
    })
}

/// Session of SQLite: a connection held by a thread of its own.
pub struct SqliteSession {
    /// Thread of the connection.
    worker: Worker<Connection>,
    /// Identity of the source and state of its databases, read at the
    /// opening.
    info: SourceInfo,
    /// Interruption of the running statement.
    canceller: Arc<SqliteCanceller>,
}

impl SqliteSession {
    /// Runs `order` on the connection, unless the session was ended.
    async fn run<R: Send + 'static>(
        &self,
        order: impl FnOnce(&mut Connection) -> Result<R, ConnectorError> + Send + 'static,
    ) -> Result<R, ConnectorError> {
        let ended_flag = Arc::clone(&self.canceller.ended);
        self.worker
            .run(move |connection| {
                if ended_flag.load(Ordering::SeqCst) {
                    return Err(ended());
                }
                order(connection)
            })
            .await?
    }
}

impl Session for SqliteSession {
    fn info(&self) -> &SourceInfo {
        &self.info
    }

    fn execute<'a>(
        &'a mut self,
        sql: &'a str,
    ) -> BoxFuture<'a, Result<Box<dyn Rows<'a> + 'a>, ConnectorError>> {
        Box::pin(async move {
            let (head_sender, head) = oneshot::channel();
            let (rows_sender, rows) = mpsc::channel(ROWS_AHEAD);
            let sql = sql.to_string();
            let ended_flag = Arc::clone(&self.canceller.ended);
            self.worker.send(move |connection: &mut Connection| {
                if ended_flag.load(Ordering::SeqCst) {
                    let _ = head_sender.send(Err(ended()));
                    return;
                }
                statement::run(connection, &sql, head_sender, rows_sender);
            })?;
            let head = head.await.map_err(|_| Stopped)??;
            let rows: Box<dyn Rows<'a> + 'a> = Box::new(SqliteRows {
                columns: head.columns,
                first: head.first,
                ending: head.ending,
                failed: false,
                rows,
                session: PhantomData,
            });
            Ok(rows)
        })
    }

    fn current_namespace(&mut self) -> BoxFuture<'_, Result<Option<Namespace>, ConnectorError>> {
        Box::pin(async { Ok(Some(Namespace::single(MAIN_SCHEMA))) })
    }

    fn switch_namespace<'a>(
        &'a mut self,
        _namespace: &'a Namespace,
    ) -> BoxFuture<'a, Result<(), ConnectorError>> {
        Box::pin(async {
            Err(ConnectorError::Unsupported(
                "changing the schema of a session",
            ))
        })
    }

    fn transaction_state(&mut self, _check: bool) -> BoxFuture<'_, Result<bool, ConnectorError>> {
        Box::pin(self.run(|connection| Ok(!connection.is_autocommit())))
    }

    fn notices(&mut self) -> BoxFuture<'_, Result<Vec<Notice>, ConnectorError>> {
        // SQLite keeps no notices of a statement.
        Box::pin(async { Ok(Vec::new()) })
    }

    fn canceller(&self) -> Arc<dyn Canceller> {
        self.canceller.clone()
    }

    fn catalog(&mut self) -> Box<dyn CatalogReader + '_> {
        Box::new(SqliteCatalog { session: self })
    }

    fn close(self: Box<Self>) -> BoxFuture<'static, ()> {
        // The thread closes the connection once its last order ran.
        Box::pin(async move { drop(self) })
    }
}

/// Result of a statement being read: its head, then the rows the thread of
/// the connection sends.
struct SqliteRows<'a> {
    /// Columns of the result.
    columns: Vec<ColumnMeta>,
    /// First row, not given yet.
    first: Option<Vec<Cell>>,
    /// What the statement wrote, once it ended.
    ending: Option<Ending>,
    /// Did the statement fail after its first row?
    failed: bool,
    /// Rows sent by the thread of the connection.
    rows: mpsc::Receiver<Message>,
    /// The session, borrowed until the result is dropped.
    session: PhantomData<&'a mut SqliteSession>,
}

impl<'a> Rows<'a> for SqliteRows<'a> {
    fn columns(&self) -> &[ColumnMeta] {
        &self.columns
    }

    fn next_row(&mut self) -> BoxFuture<'_, Result<Option<Vec<Cell>>, ConnectorError>> {
        Box::pin(async move {
            if let Some(first) = self.first.take() {
                return Ok(Some(first));
            }
            if self.ending.is_some() || self.failed {
                return Ok(None);
            }
            match self.rows.recv().await {
                Some(Message::Row(row)) => Ok(Some(row)),
                Some(Message::End(ending)) => {
                    self.ending = Some(ending);
                    Ok(None)
                }
                Some(Message::Failed(error)) => {
                    self.failed = true;
                    Err(error)
                }
                None => Err(Stopped.into()),
            }
        })
    }

    fn outcome(&self, verb: StatementVerb) -> StatementOutcome {
        statement::outcome(verb, self.ending.unwrap_or_default())
    }

    fn discard(self: Box<Self>) -> BoxFuture<'a, ()> {
        // Without its reader, the thread stops at the next row and resets the
        // statement: the session takes its next order after it.
        drop(self);
        Box::pin(async {})
    }
}

/// Interruption of the statements of a session, from another task.
struct SqliteCanceller {
    /// Interruption of the running statement of the connection.
    handle: InterruptHandle,
    /// Was the session ended? Its waiting orders fail then.
    ended: Arc<AtomicBool>,
}

impl Canceller for SqliteCanceller {
    fn interrupt(&self) -> BoxFuture<'_, Result<(), ConnectorError>> {
        self.handle.interrupt();
        Box::pin(async { Ok(()) })
    }

    fn terminate(&self) -> BoxFuture<'_, Result<(), ConnectorError>> {
        self.ended.store(true, Ordering::SeqCst);
        self.handle.interrupt();
        Box::pin(async { Ok(()) })
    }
}

/// Catalog of a SQLite session, read on its connection (`catalog`).
struct SqliteCatalog<'a> {
    /// Session whose connection reads the catalog.
    session: &'a mut SqliteSession,
}

impl CatalogReader for SqliteCatalog<'_> {
    fn namespaces(&mut self) -> BoxFuture<'_, Result<Vec<NamespaceInfo>, ConnectorError>> {
        Box::pin(
            self.session
                .run(|connection| Ok(catalog::namespaces(connection)?)),
        )
    }

    fn objects<'a>(
        &'a mut self,
        namespace: &'a Namespace,
    ) -> BoxFuture<'a, Result<Vec<ObjectInfo>, ConnectorError>> {
        let schema = namespace.name().to_string();
        Box::pin(
            self.session
                .run(move |connection| Ok(catalog::objects(connection, &schema)?)),
        )
    }

    fn details<'a>(
        &'a mut self,
        namespace: &'a Namespace,
    ) -> BoxFuture<'a, Result<Vec<TableDetails>, ConnectorError>> {
        let schema = namespace.name().to_string();
        Box::pin(
            self.session
                .run(move |connection| Ok(catalog::details(connection, &schema)?)),
        )
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::api::{DatabaseStatus, FileDatabase, FilesSettings};
    use crate::failure::ConnectFailure;
    use crate::location::{Location, Remote, RemoteLimits, Scheme};

    /// Database `schema` of the file at `path`.
    fn file(schema: &str, path: &Path) -> FileDatabase {
        FileDatabase {
            schema: schema.to_string(),
            location: Location {
                scheme: Scheme::File,
                address: path.to_string_lossy().into_owned(),
            },
        }
    }

    /// Database `schema` in memory.
    fn memory(schema: &str) -> FileDatabase {
        FileDatabase {
            schema: schema.to_string(),
            location: Location {
                scheme: Scheme::Memory,
                address: String::new(),
            },
        }
    }

    /// Source of the databases `databases`, initialised by `init_statements`.
    fn target(databases: Vec<FileDatabase>, init_statements: &[&str]) -> Target {
        Target {
            engine: Engine::Sqlite,
            settings: Settings::Files(FilesSettings {
                databases,
                remote: Remote {
                    cache_dir: std::env::temp_dir(),
                    limits: RemoteLimits::default(),
                },
            }),
            init_statements: init_statements.iter().map(ToString::to_string).collect(),
        }
    }

    /// Creates a database at `path` holding a table `t` of one row.
    fn create(path: &Path) {
        Connection::open(path)
            .unwrap()
            .execute_batch("CREATE TABLE t (a); INSERT INTO t VALUES (1)")
            .unwrap();
    }

    /// Rows of `sql` on `session`.
    async fn rows_of(
        session: &mut dyn Session,
        sql: &str,
    ) -> Result<Vec<Vec<Cell>>, ConnectorError> {
        let mut rows = session.execute(sql).await?;
        let mut read = Vec::new();
        while let Some(row) = rows.next_row().await? {
            read.push(row);
        }
        Ok(read)
    }

    /// One cell of text.
    fn cell(text: &str) -> Cell {
        Some(text.to_string())
    }

    #[tokio::test]
    async fn attaches_the_databases_it_can_open_and_tells_the_others() {
        let folder = tempfile::tempdir().unwrap();
        let main = folder.path().join("main.db");
        std::fs::File::create(&main).unwrap();
        let other = folder.path().join("other.db");
        create(&other);
        let text = folder.path().join("text.db");
        std::fs::write(&text, "x".repeat(200)).unwrap();
        let missing = folder.path().join("missing.db");
        let target = target(
            vec![
                file("main", &main),
                file("other", &other),
                file("gone", &missing),
                file("text", &text),
                memory("scratch"),
            ],
            &["PRAGMA foreign_keys = 1"],
        );
        let mut session = DRIVER.open(&target, OpenOptions::default()).await.unwrap();
        let info = session.info().clone();
        assert_eq!(info.flavor, Flavor::Sqlite);
        assert_eq!(info.version, rusqlite::version());
        let open = |bytes, journal: &str| DatabaseStatus::Open {
            bytes,
            journal_mode: journal.to_string(),
            read_only: false,
        };
        let statuses: Vec<(&str, &DatabaseStatus)> = info
            .databases
            .iter()
            .map(|state| (state.schema.as_str(), &state.status))
            .collect();
        assert_eq!(statuses[0], ("main", &open(0, "delete")));
        assert_eq!(statuses[1], ("other", &open(8192, "delete")));
        let not_found = DatabaseStatus::Unavailable {
            failure: ConnectFailure::FileNotFound {
                location: missing.to_string_lossy().into_owned(),
            },
        };
        assert_eq!(statuses[2], ("gone", &not_found));
        assert!(matches!(
            statuses[3].1,
            DatabaseStatus::Unavailable {
                failure: ConnectFailure::NotADatabase { .. }
            }
        ));
        assert_eq!(statuses[4], ("scratch", &open(0, "memory")));
        assert!(!missing.exists());

        let joined = "SELECT (SELECT count(*) FROM other.t), \
                      (SELECT count(*) FROM pragma_database_list WHERE name = 'text')";
        assert_eq!(
            rows_of(session.as_mut(), joined).await.unwrap(),
            [[cell("1"), cell("0")]]
        );
        assert_eq!(
            rows_of(session.as_mut(), "PRAGMA foreign_keys")
                .await
                .unwrap(),
            [[cell("1")]]
        );
        session.close().await;
    }

    #[tokio::test]
    async fn fails_to_open_when_the_main_database_cannot_be_read() {
        let folder = tempfile::tempdir().unwrap();
        let missing = folder.path().join("missing.db");
        let open = async |databases, init: &[&str]| {
            DRIVER
                .open(&target(databases, init), OpenOptions::default())
                .await
                .err()
        };
        assert!(matches!(
            open(vec![file("main", &missing)], &[]).await,
            Some(ConnectorError::Connect(ConnectFailure::FileNotFound { .. }))
        ));
        let text = folder.path().join("text.db");
        std::fs::write(&text, "x".repeat(200)).unwrap();
        assert!(matches!(
            open(vec![file("main", &text)], &[]).await,
            Some(ConnectorError::Connect(ConnectFailure::NotADatabase { .. }))
        ));
        assert!(matches!(
            open(vec![memory("main")], &["SELECT * FROM missing"]).await,
            Some(ConnectorError::Init { .. })
        ));
    }

    #[tokio::test]
    async fn a_read_only_session_writes_in_none_of_its_databases() {
        let folder = tempfile::tempdir().unwrap();
        let main = folder.path().join("main.db");
        create(&main);
        let other = folder.path().join("other.db");
        create(&other);
        let target = target(vec![file("main", &main), file("other", &other)], &[]);
        let read_only = OpenOptions {
            read_only: true,
            ..OpenOptions::default()
        };
        let mut session = DRIVER.open(&target, read_only).await.unwrap();
        assert!(session.info().databases.iter().all(|state| matches!(
            state.status,
            DatabaseStatus::Open {
                read_only: true,
                ..
            }
        )));
        for sql in ["INSERT INTO t VALUES (2)", "INSERT INTO other.t VALUES (2)"] {
            assert!(session.execute(sql).await.is_err(), "{sql}");
        }
        session.close().await;
    }

    #[tokio::test]
    async fn opens_a_remote_database_read_only_from_its_copy() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("served.db");
        create(&path);
        let bytes = std::fs::read(&path).unwrap();
        let app = axum::Router::new().route(
            "/r.db",
            axum::routing::get(move || {
                let bytes = bytes.clone();
                async move { bytes }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let remote = FileDatabase {
            schema: "remote".to_string(),
            location: Location::parse(&format!("http://{address}/r.db")).unwrap(),
        };
        // A file attached to a main database in memory is read from the disk.
        let mut target = target(
            vec![memory("main"), remote.clone(), file("local", &path)],
            &[],
        );
        if let Settings::Files(files) = &mut target.settings {
            files.remote.cache_dir = folder.path().join("cache");
        }
        let mut session = DRIVER.open(&target, OpenOptions::default()).await.unwrap();
        assert!(matches!(
            session.info().databases[1].status,
            DatabaseStatus::Open {
                read_only: true,
                ..
            }
        ));
        assert_eq!(
            rows_of(session.as_mut(), "SELECT count(*) FROM remote.t")
                .await
                .unwrap(),
            [[cell("1")]]
        );
        assert!(
            session
                .execute("INSERT INTO remote.t VALUES (2)")
                .await
                .is_err()
        );
        assert_eq!(
            rows_of(session.as_mut(), "SELECT count(*) FROM local.t")
                .await
                .unwrap(),
            [[cell("1")]]
        );
        session.close().await;

        // The main database itself can be remote.
        if let Settings::Files(files) = &mut target.settings {
            files.databases = vec![FileDatabase {
                schema: "main".to_string(),
                ..remote
            }];
        }
        let mut session = DRIVER.open(&target, OpenOptions::default()).await.unwrap();
        assert!(session.execute("INSERT INTO t VALUES (2)").await.is_err());
        session.close().await;
    }

    #[tokio::test]
    async fn the_connections_of_a_session_share_its_databases_in_memory() {
        let target = target(vec![memory("main"), memory("scratch")], &[]);
        let shared = OpenOptions {
            session_key: Some("shared".to_string()),
            ..OpenOptions::default()
        };
        let mut first = DRIVER.open(&target, shared.clone()).await.unwrap();
        rows_of(first.as_mut(), "CREATE TABLE scratch.m (x)")
            .await
            .unwrap();
        let mut second = DRIVER.open(&target, shared).await.unwrap();
        assert!(
            rows_of(second.as_mut(), "SELECT x FROM scratch.m")
                .await
                .is_ok()
        );
        let mut alone = DRIVER.open(&target, OpenOptions::default()).await.unwrap();
        assert!(
            rows_of(alone.as_mut(), "SELECT x FROM scratch.m")
                .await
                .is_err()
        );
        for session in [first, second, alone] {
            session.close().await;
        }
    }

    #[tokio::test]
    async fn follows_the_transaction_and_ends_the_session() {
        let mut session = DRIVER
            .open(&target(vec![memory("main")], &[]), OpenOptions::default())
            .await
            .unwrap();
        assert!(!session.transaction_state(false).await.unwrap());
        rows_of(session.as_mut(), "BEGIN").await.unwrap();
        assert!(session.transaction_state(true).await.unwrap());
        assert_eq!(
            session.current_namespace().await.unwrap(),
            Some(Namespace::single("main"))
        );
        session.canceller().terminate().await.unwrap();
        assert!(session.transaction_state(false).await.is_err());
        assert!(session.execute("SELECT 1").await.is_err());
        session.close().await;
    }

    #[tokio::test]
    async fn opens_only_on_the_main_schema() {
        let target = target(vec![memory("main")], &[]);
        let on = |name: &str| OpenOptions {
            namespace: Some(Namespace::single(name)),
            ..OpenOptions::default()
        };
        assert!(DRIVER.open(&target, on("main")).await.is_ok());
        assert!(matches!(
            DRIVER.open(&target, on("other")).await.err(),
            Some(ConnectorError::Unsupported(_))
        ));
    }
}
