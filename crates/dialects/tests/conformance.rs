//! Conformance suite (rule 8 of `CLAUDE.md`, `docs/DEV-DIALECTS.md`,
//! section 4): each query of the files of `tests/conformance/`, written in a
//! dialect, runs on each server of that dialect, and its transpilation by
//! Prisme runs on each server of the other dialect. Both give the same
//! result: the same columns, of the same type family, with the same values
//! in the same order. Otherwise Prisme refuses the query, with the rule the
//! case expects.
//!
//! The servers are Docker images started for the run, listed by the
//! variable `PRISME_CONFORMANCE_SERVERS` (by default `mysql:8.0.11`,
//! `mysql:8.4`, `mariadb:10.6` and `mariadb:11.8`): a new version is checked
//! by adding its image, without writing the cases again. Each server holds
//! the table `sample`, then the tables of each file (`setup`).
//!
//! Ignored by default, as it requires Docker Desktop running:
//! `cargo test -p prisme-dialects --test conformance -- --include-ignored`.

use std::cmp::Ordering;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use indoc::indoc;
use prisme_connectors::mysql::{
    self, Cell, Connection, MysqlTarget, TypeFamily, catalog, connect, server_info,
};
use prisme_dialects::Dialect;
use prisme_dialects::build::Tables;
use prisme_dialects::emit::OutputDialect;
use prisme_dialects::lexer::SqlMode;
use prisme_dialects::transpile::{InputDialect, prepare};
use prisme_dialects::versions::Version;
use prisme_plan::Ident;
use secrecy::SecretString;
use serde::Deserialize;
use testcontainers_modules::testcontainers::core::{IntoContainerPort, WaitFor};
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::{ContainerAsync, GenericImage, ImageExt};
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

/// Images of the servers when `PRISME_CONFORMANCE_SERVERS` does not list
/// them, separated by commas.
const DEFAULT_SERVERS: &str = "mysql:8.0.11,mysql:8.4,mariadb:10.6,mariadb:11.8";

/// Time allowed for a server to start, the image being already downloaded.
const START_TIMEOUT: Duration = Duration::from_secs(300);

/// Pairs of servers checked at once.
const PARALLEL_CHECKS: usize = 8;

/// Table that every server holds: one row of the common types, as the
/// measurements of the tool `probe` use it.
const SAMPLE: [&str; 3] = [
    "DROP TABLE IF EXISTS sample",
    indoc! {"
        CREATE TABLE sample (
          d DATE NOT NULL, dt DATETIME(3) NOT NULL, ts TIMESTAMP(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
          t TIME NOT NULL, s VARCHAR(30) NOT NULL, i INT NOT NULL, n DECIMAL(10,2) NOT NULL,
          f DOUBLE NOT NULL, dn DATE NULL, sn VARCHAR(30) NULL, j JSON NULL
        )
    "},
    indoc! {r#"
        INSERT INTO sample VALUES ('2024-05-06', '2024-05-06 07:08:09.123', '2024-05-06 07:08:09.123456',
          '10:20:30', 'Hello World', 42, 12.50, 2.5, NULL, NULL, '{"a": [1, 2]}')
    "#},
];

/// File of cases: the tables it reads, then its queries.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CaseFile {
    /// Statements run once on every server before the cases, in a SQL both
    /// dialects read.
    #[serde(default)]
    setup: Vec<String>,
    /// Queries.
    cases: Vec<Case>,
}

/// Query of a file and what is expected of its transpilation.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    /// Dialect the query is written in: `mysql`, `mariadb`.
    from: String,
    /// Query.
    sql: String,
    /// Statements run before the query on both connections (`SET time_zone =
    /// ...`): the transpiler reads the SQL mode of the session after them.
    #[serde(default)]
    session: Vec<String>,
    /// Rule of the refusal expected; absent, the transpilation must run and
    /// give the same result.
    #[serde(default)]
    refused: Option<String>,
    /// Servers of the query, as a condition (`mysql >= 8.0.17`); all those of
    /// its dialect when absent.
    #[serde(default)]
    inputs: Option<String>,
    /// Servers of the transpilation, as a condition (`mariadb < 11.2.1`); all
    /// those of the other dialect when absent.
    #[serde(default)]
    outputs: Option<String>,
    /// What is compared: `values` (by default), or `types` for a value that
    /// changes from one run to the next (`NOW()`, `UUID()`), compared by its
    /// columns and its number of rows.
    #[serde(default)]
    compare: Compare,
}

/// Part of the results compared.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Compare {
    /// Columns, types and values.
    #[default]
    Values,
    /// Columns, types and number of rows.
    Types,
}

/// Condition on the servers of a case: a dialect, an operator and a version.
#[derive(Debug)]
struct Condition {
    /// Dialect of the servers kept.
    dialect: Dialect,
    /// Orderings of the version of the server against `version` that keep
    /// it.
    accepted: Vec<Ordering>,
    /// Version compared.
    version: Version,
}

impl Condition {
    /// Reads `mariadb >= 11.2.1`.
    ///
    /// # Panics
    ///
    /// On a condition written otherwise.
    fn parse(text: &str) -> Self {
        let parts: Vec<&str> = text.split_whitespace().collect();
        let [dialect, operator, version] = parts.as_slice() else {
            panic!("condition `{text}`: expected `<dialect> <operator> <version>`");
        };
        let accepted = match *operator {
            "<" => vec![Ordering::Less],
            "<=" => vec![Ordering::Less, Ordering::Equal],
            "=" => vec![Ordering::Equal],
            ">=" => vec![Ordering::Greater, Ordering::Equal],
            ">" => vec![Ordering::Greater],
            other => panic!("condition `{text}`: unknown operator `{other}`"),
        };
        Self {
            dialect: dialect_named(dialect),
            accepted,
            version: Version::parse(version)
                .unwrap_or_else(|| panic!("condition `{text}`: unknown version")),
        }
    }

    /// Tells whether `server` meets the condition.
    fn keeps(&self, server: &Server) -> bool {
        server.dialect == self.dialect && self.accepted.contains(&server.version.cmp(&self.version))
    }
}

/// Server of the run, in its container.
struct Server {
    /// Docker image, `mysql:8.0.11`.
    image: String,
    /// Dialect.
    dialect: Dialect,
    /// Version, as `SELECT VERSION()` gives it.
    version: Version,
    /// Connection settings.
    target: MysqlTarget,
    /// Container, removed when the server is dropped.
    _container: ContainerAsync<GenericImage>,
}

impl Server {
    /// Image and version of the server: `mariadb:10.6 (MariaDB 10.6.28)`.
    fn label(&self) -> String {
        format!("{} ({} {})", self.image, self.dialect.label(), self.version)
    }
}

/// Result of a query, as the interface shows it.
#[derive(Debug)]
struct Answer {
    /// Names of the columns.
    names: Vec<String>,
    /// Type families of the columns.
    families: Vec<TypeFamily>,
    /// Rows, each value as text.
    rows: Vec<Vec<Cell>>,
}

/// What a case gave on a pair of servers.
enum Outcome {
    /// Same result.
    Same,
    /// Refused with the rule expected.
    Refused,
}

/// Dialect of the name `name` in the cases and the images.
///
/// # Panics
///
/// On an unknown dialect.
fn dialect_named(name: &str) -> Dialect {
    match name {
        "mysql" => Dialect::MySql,
        "mariadb" => Dialect::MariaDb,
        other => panic!("unknown dialect `{other}`"),
    }
}

/// Starts the server of `image` (`mariadb:10.6`) and reads its version.
///
/// The server is awaited on the message of the server that listens on its
/// port, which all the versions write; the server of the initialisation of
/// the image listens on none. It resolves no host name: the reverse lookup
/// of each client address slows every connection down in Docker.
///
/// # Panics
///
/// When the container does not start or the server does not answer.
async fn start(image: String) -> Server {
    let (name, tag) = image
        .split_once(':')
        .unwrap_or_else(|| panic!("image `{image}`: expected `<name>:<tag>`"));
    let dialect = dialect_named(name);
    let (empty_password, database) = match dialect {
        Dialect::MySql => ("MYSQL_ALLOW_EMPTY_PASSWORD", "MYSQL_DATABASE"),
        Dialect::MariaDb => ("MARIADB_ALLOW_EMPTY_ROOT_PASSWORD", "MARIADB_DATABASE"),
    };
    let container = GenericImage::new(name, tag)
        .with_exposed_port(3306.tcp())
        .with_wait_for(WaitFor::message_on_stderr("port: 3306"))
        .with_env_var(empty_password, "yes")
        .with_env_var(database, "test")
        .with_cmd(["--skip-name-resolve"])
        .with_startup_timeout(START_TIMEOUT)
        .start()
        .await
        .unwrap_or_else(|error| panic!("{image}: {error}"));
    let target = MysqlTarget {
        host: container.get_host().await.unwrap().to_string(),
        port: container.get_host_port_ipv4(3306).await.unwrap(),
        username: "root".to_string(),
        password: SecretString::from(""),
        database_name: "test".to_string(),
        init_statements: Vec::new(),
    };
    let mut connection = connect(&target)
        .await
        .unwrap_or_else(|error| panic!("{image}: {error}"));
    let info = server_info(&mut connection).await.unwrap();
    let version = Version::parse(&info.version)
        .unwrap_or_else(|| panic!("{image}: version `{}` unread", info.version));
    Server {
        image,
        dialect,
        version,
        target,
        _container: container,
    }
}

/// Servers of the run, started together.
async fn start_servers() -> Vec<Server> {
    let images =
        std::env::var("PRISME_CONFORMANCE_SERVERS").unwrap_or_else(|_| DEFAULT_SERVERS.to_string());
    let tasks: Vec<_> = images
        .split(',')
        .map(|image| tokio::spawn(start(image.trim().to_string())))
        .collect();
    let mut servers = Vec::new();
    for task in tasks {
        servers.push(task.await.expect("server start"));
    }
    servers
}

/// Files of cases, by name.
fn case_files() -> Vec<(PathBuf, CaseFile)> {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/conformance");
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&directory)
        .unwrap_or_else(|error| panic!("{}: {error}", directory.display()))
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "yaml")
        })
        .collect();
    paths.sort();
    paths
        .into_iter()
        .map(|path| {
            let text = std::fs::read_to_string(&path).unwrap();
            let file = yaml_serde::from_str(&text)
                .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            (path, file)
        })
        .collect()
}

/// Runs `statements`, which give no rows, on `connection`.
async fn run_all(connection: &mut Connection, statements: &[String]) -> Result<(), String> {
    for statement in statements {
        let stream = mysql::execute(connection, statement)
            .await
            .map_err(|error| format!("`{statement}`: {error}"))?;
        stream.discard().await;
    }
    Ok(())
}

/// Runs `sql` on `connection` and reads its whole result.
async fn answer(connection: &mut Connection, sql: &str) -> Result<Answer, String> {
    let mut stream = mysql::execute(connection, sql)
        .await
        .map_err(|error| error.to_string())?;
    let names = stream.column_names();
    let families = stream
        .column_types()
        .into_iter()
        .map(|info| info.family)
        .collect();
    let mut rows = Vec::new();
    while let Some(row) = stream.next_row().await.map_err(|error| error.to_string())? {
        rows.push(row);
    }
    Ok(Answer {
        names,
        families,
        rows,
    })
}

/// Tells whether two type families give the same result: a `JSON` of MySQL
/// is a text in MariaDB, and the column of a `NULL` has no family.
fn same_family(left: TypeFamily, right: TypeFamily) -> bool {
    let family = |family| match family {
        TypeFamily::Json => TypeFamily::Text,
        family => family,
    };
    left == TypeFamily::Other || right == TypeFamily::Other || family(left) == family(right)
}

/// Compares the answers of the query and of its transpilation; `None` when
/// they give the same result.
fn difference(expected: &Answer, got: &Answer, compare: Compare) -> Option<String> {
    if expected.names != got.names {
        return Some(format!(
            "columns {:?}, transpiled {:?}",
            expected.names, got.names
        ));
    }
    let families = expected.families.iter().zip(&got.families);
    if !families
        .clone()
        .all(|(left, right)| same_family(*left, *right))
    {
        return Some(format!(
            "types {:?}, transpiled {:?}",
            expected.families, got.families
        ));
    }
    let same_rows = match compare {
        Compare::Values => values(expected) == values(got),
        Compare::Types => expected.rows.len() == got.rows.len(),
    };
    (!same_rows).then(|| format!("rows {:?}\n  transpiled {:?}", expected.rows, got.rows))
}

/// Rows of `answer` as they compare: a decimal by its value, `42` as
/// `42.00` (MySQL writes the values of a `DECIMAL` `CASE` at the scale of
/// each result, MariaDB at the scale of the type).
fn values(answer: &Answer) -> Vec<Vec<Cell>> {
    answer
        .rows
        .iter()
        .map(|row| {
            row.iter()
                .zip(&answer.families)
                .map(|(cell, family)| match (cell, family) {
                    (Some(text), TypeFamily::Decimal) => Some(decimal_value(text)),
                    _ => cell.clone(),
                })
                .collect()
        })
        .collect()
}

/// Decimal `text` without the zeros that end its fraction.
fn decimal_value(text: &str) -> String {
    match text.split_once('.') {
        Some((whole, fraction)) => match fraction.trim_end_matches('0') {
            "" => whole.to_string(),
            fraction => format!("{whole}.{fraction}"),
        },
        None => text.to_string(),
    }
}

/// Opens a connection to `server` and runs the statements of the session.
async fn session(server: &Server, statements: &[String]) -> Result<Connection, String> {
    let mut connection = connect(&server.target)
        .await
        .map_err(|error| error.to_string())?;
    run_all(&mut connection, statements).await?;
    Ok(connection)
}

/// Transpiles `case` from `input` to `output`, runs both queries and
/// compares their results.
async fn check(case: &Case, input: &Server, output: &Server) -> Result<Outcome, String> {
    let mut source = session(input, &case.session).await?;
    let mut target = session(output, &case.session).await?;
    let info = server_info(&mut target)
        .await
        .map_err(|error| error.to_string())?;
    let translation = prepare(
        &case.sql,
        InputDialect {
            dialect: input.dialect,
            version: input.version,
        },
        OutputDialect {
            dialect: output.dialect,
            version: output.version,
            mode: SqlMode::parse(&info.sql_mode),
        },
    )
    .map_err(|error| format!("not read: {error}"))?;
    let mut tables = Tables::default();
    for table in translation.tables() {
        let database = table.database.as_ref().map(|name| name.value.as_str());
        let columns = catalog::table_columns(&mut target, database, &table.name.value)
            .await
            .map_err(|error| error.to_string())?;
        if let Some(columns) = columns {
            let columns = columns
                .into_iter()
                .map(|(name, ty)| (Ident::new(name), ty))
                .collect();
            tables.insert(database, &table.name.value, columns);
        }
    }
    let transpilation = translation
        .transpile(&tables)
        .map_err(|error| format!("not built: {error}"))?;
    let rules: Vec<&str> = transpilation
        .errors
        .iter()
        .map(|error| error.rule)
        .collect();
    match (&case.refused, rules.is_empty()) {
        (Some(rule), false) if rules.contains(&rule.as_str()) => return Ok(Outcome::Refused),
        (Some(rule), true) => {
            return Err(format!(
                "translated, `{rule}` expected: {}",
                transpilation.sql
            ));
        }
        (_, false) => return Err(format!("refused: {:?}", transpilation.errors)),
        (None, true) => {}
    }
    let expected = answer(&mut source, &case.sql)
        .await
        .map_err(|error| format!("{}: {error}", input.label()))?;
    let got = answer(&mut target, &transpilation.sql)
        .await
        .map_err(|error| {
            format!(
                "{}: {error}\n  transpiled {}",
                output.label(),
                transpilation.sql
            )
        })?;
    match difference(&expected, &got, case.compare) {
        Some(difference) => Err(format!("{difference}\n  transpiled {}", transpilation.sql)),
        None => Ok(Outcome::Same),
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires Docker Desktop running"]
async fn transpilations_give_the_same_result() {
    let servers = start_servers().await;
    let files = case_files();
    for server in &servers {
        let mut connection = session(server, &[]).await.unwrap();
        let sample: Vec<String> = SAMPLE.iter().map(ToString::to_string).collect();
        run_all(&mut connection, &sample)
            .await
            .unwrap_or_else(|error| panic!("{}: {error}", server.label()));
        for (path, file) in &files {
            run_all(&mut connection, &file.setup)
                .await
                .unwrap_or_else(|error| {
                    panic!("{} on {}: {error}", path.display(), server.label())
                });
        }
    }
    // Each pair of servers of each case, checked in parallel; the failures
    // are listed in the order of the files.
    let servers = Arc::new(servers);
    let files = Arc::new(files);
    let permits = Arc::new(Semaphore::new(PARALLEL_CHECKS));
    let mut checks = JoinSet::new();
    let mut failures = Vec::new();
    for (file_index, (path, file)) in files.iter().enumerate() {
        for (case_index, case) in file.cases.iter().enumerate() {
            let from = dialect_named(&case.from);
            let inputs = case.inputs.as_deref().map(Condition::parse);
            let outputs = case.outputs.as_deref().map(Condition::parse);
            let kept = |server: &Server, input: bool| {
                let condition = if input { &inputs } else { &outputs };
                (server.dialect == from) == input
                    && condition
                        .as_ref()
                        .is_none_or(|condition| condition.keeps(server))
            };
            let mut pairs = 0;
            for (input, input_server) in servers.iter().enumerate() {
                for (output, output_server) in servers.iter().enumerate() {
                    if !kept(input_server, true) || !kept(output_server, false) {
                        continue;
                    }
                    pairs += 1;
                    let (servers, files, permits) = (
                        Arc::clone(&servers),
                        Arc::clone(&files),
                        Arc::clone(&permits),
                    );
                    checks.spawn(async move {
                        let _permit = permits.acquire().await.expect("open semaphore");
                        let case = &files[file_index].1.cases[case_index];
                        let outcome = check(case, &servers[input], &servers[output]).await;
                        ((file_index, case_index, input, output), outcome)
                    });
                }
            }
            if pairs == 0 {
                let name = path.file_name().unwrap().to_string_lossy();
                let key = (file_index, case_index, 0, 0);
                failures.push((key, format!("{name}: no server for {}", case.sql)));
            }
        }
    }
    let (mut same, mut refused) = (0, 0);
    while let Some(joined) = checks.join_next().await {
        let (key, outcome) = joined.expect("check task");
        let (file_index, case_index, input, output) = key;
        match outcome {
            Ok(Outcome::Same) => same += 1,
            Ok(Outcome::Refused) => refused += 1,
            Err(failure) => {
                let (path, file) = &files[file_index];
                let name = path.file_name().unwrap().to_string_lossy();
                let message = format!(
                    "{name}: {} -> {}: {}\n  {failure}",
                    servers[input].label(),
                    servers[output].label(),
                    file.cases[case_index].sql
                );
                failures.push((key, message));
            }
        }
    }
    failures.sort_by_key(|(key, _)| *key);
    let failures: Vec<String> = failures.into_iter().map(|(_, message)| message).collect();
    println!(
        "{same} same results, {refused} refusals, {} failures",
        failures.len()
    );
    assert!(failures.is_empty(), "\n{}", failures.join("\n\n"));
}
