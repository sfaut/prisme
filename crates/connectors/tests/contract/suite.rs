//! The tests of the contract suite, generic over the `Fixture` of an engine:
//! each one opens sessions through the driver of the registry and uses only
//! the traits of `prisme_connectors::api`.

use std::time::{Duration, Instant};

use prisme_connectors::api::{ConnectorError, Namespace, OpenOptions, Session, Target};
use prisme_connectors::catalog::ObjectKind;
use prisme_connectors::drivers;
use prisme_connectors::flavor::Flavor;
use prisme_dialects::verb::StatementVerb;

/// What the suite needs to know of an engine, and of one of its sources.
pub trait Fixture {
    /// Name of the source in the messages of the failures.
    fn name(&self) -> String;

    /// Flavor the source must tell.
    fn flavor(&self) -> Flavor;

    /// Runs `test` on a source of its own, empty, created again for it,
    /// opened on its namespace; a second namespace, empty too, is given for
    /// the engines that change namespaces.
    async fn with_source(&self, name: &str, test: impl AsyncFnOnce(Target, Namespace));

    /// Statement that runs long enough to be stopped (30 seconds at least).
    fn long_statement(&self) -> &'static str;
}

/// Opens a session on `target` through the driver of its engine.
async fn open(target: &Target, options: OpenOptions) -> Box<dyn Session> {
    drivers::of(target.engine)
        .open(target, options)
        .await
        .unwrap_or_else(|error| panic!("cannot open a session: {error}"))
}

/// Runs `sql` and reads its rows to the end.
async fn rows_of(session: &mut dyn Session, sql: &str) -> Vec<Vec<Option<String>>> {
    let mut stream = session.execute(sql).await.unwrap();
    let mut rows = Vec::new();
    while let Some(row) = stream.next_row().await.unwrap() {
        rows.push(row);
    }
    stream.discard().await;
    rows
}

/// Runs `sql`, a statement without rows, and returns its outcome.
async fn run(session: &mut dyn Session, sql: &str, verb: StatementVerb) -> Option<u64> {
    let mut stream = session.execute(sql).await.unwrap();
    assert!(stream.columns().is_empty(), "{sql}");
    assert_eq!(stream.next_row().await.unwrap(), None, "{sql}");
    let outcome = stream.outcome(verb);
    stream.discard().await;
    outcome.affected
}

/// Creates the table `t` with two rows.
async fn fill(session: &mut dyn Session) {
    run(
        session,
        "CREATE TABLE t (id INTEGER, label VARCHAR(20))",
        StatementVerb::Other,
    )
    .await;
    let affected = run(
        session,
        "INSERT INTO t VALUES (1, 'clé €'), (2, NULL)",
        StatementVerb::Insert,
    )
    .await;
    assert_eq!(affected, Some(2));
}

/// The driver tests the source and tells its identity, the same as an open
/// session tells; the capabilities are those of the engine.
pub async fn probes_the_source(fixture: &impl Fixture) {
    fixture
        .with_source("probes", async |target, _| {
            let driver = drivers::of(target.engine);
            let info = driver.probe(&target).await.unwrap();
            assert_eq!(info.flavor, fixture.flavor(), "{}", fixture.name());
            assert_eq!(info.capabilities, driver.capabilities());
            assert!(!info.version.is_empty());
            let session = open(&target, OpenOptions::default()).await;
            assert_eq!(session.info(), &info, "{}", fixture.name());
            session.close().await;
        })
        .await;
}

/// A write tells its rows; a read gives its columns and its rows, text and
/// NULL as they are.
pub async fn streams_rows_and_reports_writes(fixture: &impl Fixture) {
    fixture
        .with_source("streams", async |target, _| {
            let mut session = open(&target, OpenOptions::default()).await;
            fill(session.as_mut()).await;
            let mut stream = session
                .execute("SELECT id, label FROM t ORDER BY id")
                .await
                .unwrap();
            let names: Vec<&str> = stream
                .columns()
                .iter()
                .map(|column| column.name.as_str())
                .collect();
            assert_eq!(names, ["id", "label"], "{}", fixture.name());
            assert!(stream.columns()[0].right_aligned);
            assert!(!stream.columns()[1].right_aligned);
            let mut rows = Vec::new();
            while let Some(row) = stream.next_row().await.unwrap() {
                rows.push(row);
            }
            stream.discard().await;
            assert_eq!(
                rows,
                [
                    [Some("1".to_string()), Some("clé €".to_string())],
                    [Some("2".to_string()), None],
                ]
            );
            session.close().await;
        })
        .await;
}

/// A failed statement gives the text of the source, and the session goes on.
pub async fn fails_a_statement_and_stays_usable(fixture: &impl Fixture) {
    fixture
        .with_source("fails", async |target, _| {
            let mut session = open(&target, OpenOptions::default()).await;
            let failed = match session.execute("SELECT * FROM missing_table").await {
                Ok(_) => panic!("{}: a missing table is read", fixture.name()),
                Err(error) => error,
            };
            assert!(
                matches!(failed, ConnectorError::Query(_)),
                "{}: {failed:?}",
                fixture.name()
            );
            assert!(failed.cause().to_string().contains("missing_table"));
            assert_eq!(
                rows_of(session.as_mut(), "SELECT 1").await,
                [[Some("1".to_string())]]
            );
            session.close().await;
        })
        .await;
}

/// The session tells when a transaction opens and ends.
pub async fn follows_the_transaction(fixture: &impl Fixture) {
    fixture
        .with_source("transaction", async |target, _| {
            let mut session = open(&target, OpenOptions::default()).await;
            fill(session.as_mut()).await;
            assert!(!session.transaction_state(false).await.unwrap());
            run(session.as_mut(), "BEGIN", StatementVerb::Other).await;
            run(session.as_mut(), "DELETE FROM t", StatementVerb::Delete).await;
            assert!(session.transaction_state(false).await.unwrap());
            run(session.as_mut(), "ROLLBACK", StatementVerb::Other).await;
            assert!(!session.transaction_state(true).await.unwrap());
            assert_eq!(
                rows_of(session.as_mut(), "SELECT COUNT(*) FROM t").await,
                [[Some("2".to_string())]]
            );
            session.close().await;
        })
        .await;
}

/// The canceller stops a long statement from another task; the session goes
/// on. Interrupted, a statement may end with an error or with a result,
/// depending on the moment: only the duration counts.
pub async fn interrupts_a_statement_and_stays_usable(fixture: &impl Fixture) {
    fixture
        .with_source("interrupts", async |target, _| {
            let mut session = open(&target, OpenOptions::default()).await;
            let canceller = session.canceller();
            let stopping = tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(500)).await;
                canceller.interrupt().await.unwrap();
            });
            let started = Instant::now();
            if let Ok(mut stream) = session.execute(fixture.long_statement()).await {
                while let Ok(Some(_)) = stream.next_row().await {}
                stream.discard().await;
            }
            stopping.await.unwrap();
            assert!(
                started.elapsed() < Duration::from_secs(10),
                "{}",
                fixture.name()
            );
            assert_eq!(
                rows_of(session.as_mut(), "SELECT 1").await,
                [[Some("1".to_string())]]
            );
            session.close().await;
        })
        .await;
}

/// A read-only session reads, and the source refuses its writes.
pub async fn refuses_writes_when_read_only(fixture: &impl Fixture) {
    fixture
        .with_source("read_only", async |target, _| {
            let mut writer = open(&target, OpenOptions::default()).await;
            fill(writer.as_mut()).await;
            writer.close().await;

            let read_only = OpenOptions {
                read_only: true,
                ..OpenOptions::default()
            };
            let mut reader = open(&target, read_only).await;
            assert_eq!(
                rows_of(reader.as_mut(), "SELECT COUNT(*) FROM t").await,
                [[Some("2".to_string())]]
            );
            let refused = reader.execute("DELETE FROM t").await.is_err();
            assert!(refused, "{}: a write passes", fixture.name());
            reader.close().await;
        })
        .await;
}

/// A session opens on the namespace of its options, else on that of its
/// settings.
pub async fn opens_on_a_namespace(fixture: &impl Fixture) {
    fixture
        .with_source("namespace", async |target, other| {
            let mut session = open(&target, OpenOptions::default()).await;
            assert_eq!(
                session.current_namespace().await.unwrap(),
                target.namespace(),
                "{}",
                fixture.name()
            );
            session.close().await;

            let elsewhere = OpenOptions {
                namespace: Some(other.clone()),
                ..OpenOptions::default()
            };
            let mut session = open(&target, elsewhere).await;
            assert_eq!(session.current_namespace().await.unwrap(), Some(other));
            session.close().await;
        })
        .await;
}

/// An engine that changes namespaces does it; another refuses it with
/// `Unsupported`.
pub async fn switches_the_namespace(fixture: &impl Fixture) {
    fixture
        .with_source("switch", async |target, other| {
            let mut session = open(&target, OpenOptions::default()).await;
            let switched = session.switch_namespace(&other).await;
            if session.info().capabilities.switch_namespace {
                switched.unwrap();
                assert_eq!(session.current_namespace().await.unwrap(), Some(other));
            } else {
                assert!(matches!(switched, Err(ConnectorError::Unsupported(_))));
            }
            session.close().await;
        })
        .await;
}

/// The catalog lists the namespace of the source, its table, and the columns
/// of that table.
pub async fn reads_the_catalog(fixture: &impl Fixture) {
    fixture
        .with_source("catalog", async |target, _| {
            let mut session = open(&target, OpenOptions::default()).await;
            fill(session.as_mut()).await;
            let namespace = target.namespace().expect("the source opens on a namespace");
            let namespaces = session.namespaces().await.unwrap();
            let listed = namespaces.iter().find(|info| info.namespace == namespace);
            assert!(
                listed.is_some_and(|info| !info.system),
                "{}",
                fixture.name()
            );

            let objects = session.objects(&namespace).await.unwrap();
            let table = objects
                .iter()
                .find(|object| object.name == "t")
                .unwrap_or_else(|| panic!("{}: no table t", fixture.name()));
            assert_eq!(table.kind, ObjectKind::Table);
            assert!(
                session
                    .info()
                    .capabilities
                    .object_kinds
                    .contains(&ObjectKind::Table)
            );

            let details = session.details(&namespace).await.unwrap();
            let columns: Vec<&str> = details
                .iter()
                .find(|details| details.table == "t")
                .map(|details| {
                    details
                        .columns
                        .iter()
                        .map(|column| column.name.as_str())
                        .collect()
                })
                .unwrap_or_default();
            assert_eq!(columns, ["id", "label"], "{}", fixture.name());
            session.close().await;
        })
        .await;
}
