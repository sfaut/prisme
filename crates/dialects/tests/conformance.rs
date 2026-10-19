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
//! `mysql:8.4`, `mariadb:10.6` and `mariadb:11.8`), or `all` for every local
//! image of MySQL and MariaDB, one per version: a new version is checked by
//! adding its image, without writing the cases again. The images older than
//! the floor of their dialect (`versions::Registry::floor`), which Prisme
//! refuses, are left out.
//! `PRISME_CONFORMANCE_FILES` limits the run to some files (`grammar,cast`).
//! The version of each image is read from its environment before the run, so
//! that the servers need not run together: they start by groups of 5
//! (`PRISME_CONFORMANCE_GROUP`), many servers at once running out of memory.
//! Each server of a group holds the table `sample` and the tables of each
//! file (`setup`), runs the queries it is the input of, then the
//! transpilations it is the output of, each distinct text once; the results
//! are compared once every group has run.
//!
//! Ignored by default, as it requires Docker Desktop running:
//! `cargo test -p prisme-dialects --test conformance -- --include-ignored`.

use std::cmp::Ordering;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
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
use prisme_dialects::versions::{Version, registry};
use prisme_plan::Ident;
use secrecy::SecretString;
use serde::Deserialize;
use testcontainers_modules::testcontainers::core::{IntoContainerPort, Mount, WaitFor};
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::{ContainerAsync, GenericImage, ImageExt};
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

/// Images of the servers when `PRISME_CONFORMANCE_SERVERS` does not list
/// them, separated by commas.
const DEFAULT_SERVERS: &str = "mysql:8.0.11,mysql:8.4,mariadb:10.6,mariadb:11.8";

/// Servers started together when `PRISME_CONFORMANCE_GROUP` does not say.
const DEFAULT_GROUP: usize = 5;

/// Time allowed for a server to start, the image being already downloaded.
const START_TIMEOUT: Duration = Duration::from_secs(300);

/// Queries run at once on the servers of a group.
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
    /// ...`): the transpiler reads the SQL mode of the session after them. A
    /// time zone is an offset (`'+05:00'`): the servers have no table of the
    /// named time zones.
    #[serde(default)]
    session: Vec<String>,
    /// Rule of the refusal expected; absent, the transpilation must run and
    /// give the same result.
    #[serde(default)]
    refused: Option<String>,
    /// Servers of the query, as a condition (`mysql >= 8.0.17`, see
    /// `Selection`); all those of its dialect when absent.
    #[serde(default)]
    inputs: Option<String>,
    /// Servers of the transpilation, as a condition (`mariadb < 11.2.1`, see
    /// `Selection`); all those of the other dialect when absent.
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

/// Servers of a case: alternatives separated by `or`, each one of
/// conditions separated by `and`
/// (`mysql >= 8.0.31 and mysql < 8.0.42 or mysql >= 9.0.0`).
#[derive(Debug)]
struct Selection(Vec<Vec<Condition>>);

impl Selection {
    /// Reads the alternatives of `text`.
    ///
    /// # Panics
    ///
    /// On a condition written otherwise.
    fn parse(text: &str) -> Self {
        let alternatives = text
            .split(" or ")
            .map(|alternative| (alternative.split(" and ")).map(Condition::parse).collect());
        Self(alternatives.collect())
    }

    /// Tells whether the server of `image` meets one of the alternatives.
    fn keeps(&self, image: &Image) -> bool {
        (self.0.iter()).any(|conditions| conditions.iter().all(|condition| condition.keeps(image)))
    }
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

    /// Tells whether the server of `image` meets the condition.
    fn keeps(&self, image: &Image) -> bool {
        image.dialect == self.dialect && self.accepted.contains(&image.version.cmp(&self.version))
    }
}

/// Image of a server of the run.
#[derive(Debug, Clone)]
struct Image {
    /// Docker image, `mysql:8.0.11`.
    name: String,
    /// Dialect.
    dialect: Dialect,
    /// Version of the server, read from the environment of the image, and
    /// checked against `SELECT VERSION()` at its start.
    version: Version,
}

impl Image {
    /// Image and version of the server: `mariadb:10.6 (MariaDB 10.6.28)`.
    fn label(&self) -> String {
        format!("{} ({} {})", self.name, self.dialect.label(), self.version)
    }
}

/// Server of the run, in its container.
struct Server {
    /// Image of the server.
    image: Image,
    /// Connection settings.
    target: MysqlTarget,
    /// Container, removed by `Server::remove` or when the server is
    /// dropped.
    container: ContainerAsync<GenericImage>,
}

impl Server {
    /// Removes the container of the server, freeing its memory before the
    /// next group.
    async fn remove(self) {
        let label = self.image.label();
        self.container
            .rm()
            .await
            .unwrap_or_else(|error| panic!("{label}: {error}"));
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

/// Transpilation of a case for a pair of servers.
enum Transpiled {
    /// Refused with the rule expected.
    Refused,
    /// Refused otherwise than expected, or translated when a refusal was
    /// expected.
    Failed(String),
    /// Text run on the output.
    Sql(String),
}

/// Result of a query on a server: its answer, or the error of the server.
type Run = Result<Answer, String>;

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
/// port, which all the versions write, `port: 3306` followed by a space: the
/// X Plugin of MySQL writes `port: 33060` before it, and a connection made
/// then is closed when the servers start slowly. The server of the
/// initialisation of the image listens on no port. It resolves no host
/// name: the reverse lookup of each client address slows every connection
/// down in Docker. Its start skips the tables of the named time zones, and
/// its data directory is in memory, lost with the container (`docs/PERF.md`,
/// chapter 8).
///
/// # Panics
///
/// When the container does not start, the server does not answer or gives
/// another version than its image.
async fn start(image: Image) -> Server {
    let (name, tag) = image
        .name
        .split_once(':')
        .unwrap_or_else(|| panic!("image `{}`: expected `<name>:<tag>`", image.name));
    let (empty_password, database, skip_time_zones) = match image.dialect {
        Dialect::MySql => (
            "MYSQL_ALLOW_EMPTY_PASSWORD",
            "MYSQL_DATABASE",
            "MYSQL_INITDB_SKIP_TZINFO",
        ),
        Dialect::MariaDb => (
            "MARIADB_ALLOW_EMPTY_ROOT_PASSWORD",
            "MARIADB_DATABASE",
            "MARIADB_INITDB_SKIP_TZINFO",
        ),
    };
    let container = GenericImage::new(name, tag)
        .with_exposed_port(3306.tcp())
        .with_wait_for(WaitFor::message_on_stderr("port: 3306 "))
        .with_env_var(empty_password, "yes")
        .with_env_var(database, "test")
        .with_env_var(skip_time_zones, "yes")
        .with_mount(Mount::tmpfs_mount("/var/lib/mysql"))
        .with_cmd(["--skip-name-resolve"])
        .with_startup_timeout(START_TIMEOUT)
        .start()
        .await
        .unwrap_or_else(|error| panic!("{}: {error}", image.name));
    let target = MysqlTarget {
        host: ipv4_host(container.get_host().await.unwrap().to_string()),
        port: container.get_host_port_ipv4(3306).await.unwrap(),
        username: "root".to_string(),
        password: SecretString::from(""),
        database_name: "test".to_string(),
        init_statements: Vec::new(),
    };
    let mut connection = connect(&target)
        .await
        .unwrap_or_else(|error| panic!("{}: {error}", image.name));
    let info = server_info(&mut connection).await.unwrap();
    let version = Version::parse(&info.version)
        .unwrap_or_else(|| panic!("{}: version `{}` unread", image.name, info.version));
    assert_eq!(
        version, image.version,
        "{}: the server gives another version than its image",
        image.name
    );
    Server {
        image,
        target,
        container,
    }
}

/// Host `host` of a container, `localhost` written `127.0.0.1`:
/// `localhost` resolves to `::1` first, where the port of the container
/// does not answer, and Windows reports the refusal after 2 seconds, on
/// each connection (`docs/PERF.md`, chapter 8).
fn ipv4_host(host: String) -> String {
    if host == "localhost" {
        "127.0.0.1".to_string()
    } else {
        host
    }
}

/// Servers of `images`, started together.
async fn start_servers(images: &[Image]) -> Vec<Server> {
    let tasks: Vec<_> = images
        .iter()
        .map(|image| tokio::spawn(start(image.clone())))
        .collect();
    let mut servers = Vec::new();
    for task in tasks {
        servers.push(task.await.expect("server start"));
    }
    servers
}

/// Images of the run, MySQL first, then by version: those of
/// `PRISME_CONFORMANCE_SERVERS`, separated by commas, or every local image
/// of MySQL and MariaDB for `all`, one per version (of the most precise
/// tag: `mariadb:11.8.9` rather than `mariadb:11.8`), from the floor of
/// their dialect.
///
/// # Panics
///
/// When Docker does not answer, or an image has no version in its
/// environment.
fn images() -> Vec<Image> {
    let list =
        std::env::var("PRISME_CONFORMANCE_SERVERS").unwrap_or_else(|_| DEFAULT_SERVERS.to_string());
    let names: Vec<String> = if list.trim() == "all" {
        docker(&["images", "--format", "{{.Repository}}:{{.Tag}}"])
            .lines()
            .filter(|name| name.starts_with("mysql:") || name.starts_with("mariadb:"))
            .filter(|name| !name.ends_with(":<none>"))
            .map(ToString::to_string)
            .collect()
    } else {
        list.split(',')
            .map(|name| name.trim().to_string())
            .collect()
    };
    let mut images: Vec<Image> = Vec::new();
    for name in names {
        let image = image_of(&name);
        let precision = |name: &str| name.matches('.').count();
        match images
            .iter_mut()
            .find(|other| other.dialect == image.dialect && other.version == image.version)
        {
            Some(known) if precision(&name) > precision(&known.name) => *known = image,
            Some(_) => {}
            None => images.push(image),
        }
    }
    images.retain(|image| {
        let floor = registry(image.dialect).floor();
        let kept = image.version >= floor;
        if !kept {
            println!("{}: left out, older than the floor {floor}", image.name);
        }
        kept
    });
    images.sort_by_key(|image| (image.dialect == Dialect::MariaDb, image.version));
    images
}

/// Image `name`, downloaded when missing, with the version of its server,
/// read from its environment (`MYSQL_VERSION=8.0.31-1.el8`,
/// `MARIADB_VERSION=1:11.8.9+maria~ubu2404`).
///
/// # Panics
///
/// When Docker does not answer, or the image has no version.
fn image_of(name: &str) -> Image {
    let (dialect, _) = name
        .split_once(':')
        .unwrap_or_else(|| panic!("image `{name}`: expected `<name>:<tag>`"));
    let dialect = dialect_named(dialect);
    let inspect = ["image", "inspect", "--format", "{{json .Config.Env}}", name];
    let environment = docker_output(&inspect).unwrap_or_else(|_| {
        docker(&["pull", "--quiet", name]);
        docker(&inspect)
    });
    let value = ["\"MYSQL_VERSION=", "\"MARIADB_VERSION="]
        .iter()
        .find_map(|variable| environment.split_once(variable))
        .unwrap_or_else(|| panic!("image `{name}`: no version in its environment"))
        .1;
    // The epoch of the Debian packages of MariaDB, `1:`, comes first.
    let value = value.split_once(':').map_or(value, |(_, rest)| rest);
    let version =
        Version::parse(value).unwrap_or_else(|| panic!("image `{name}`: version `{value}` unread"));
    Image {
        name: name.to_string(),
        dialect,
        version,
    }
}

/// Output of `docker arguments`.
///
/// # Panics
///
/// When the command fails.
fn docker(arguments: &[&str]) -> String {
    docker_output(arguments).unwrap_or_else(|error| panic!("docker {}: {error}", arguments[0]))
}

/// Output of `docker arguments`, or its error.
fn docker_output(arguments: &[&str]) -> Result<String, String> {
    let output = Command::new("docker")
        .args(arguments)
        .output()
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Files of cases, by name; those of `PRISME_CONFORMANCE_FILES`, names
/// without extension separated by commas, when it lists them.
///
/// # Panics
///
/// On a file unread, or a name of `PRISME_CONFORMANCE_FILES` of no file.
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
    if let Ok(list) = std::env::var("PRISME_CONFORMANCE_FILES") {
        let names: Vec<&str> = list.split(',').map(str::trim).collect();
        for name in &names {
            assert!(
                paths
                    .iter()
                    .any(|path| path.file_stem().is_some_and(|stem| stem == *name)),
                "PRISME_CONFORMANCE_FILES: no file `{name}.yaml`"
            );
        }
        paths.retain(|path| {
            path.file_stem()
                .is_some_and(|stem| names.iter().any(|name| stem == *name))
        });
    }
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
/// each result, MariaDB at the scale of the type), a floating point number
/// by its value too, `20240506070809.120` as `20240506070809.12` (MySQL
/// writes the `DOUBLE` of a date and time with the digits of its fraction).
fn values(answer: &Answer) -> Vec<Vec<Cell>> {
    answer
        .rows
        .iter()
        .map(|row| {
            row.iter()
                .zip(&answer.families)
                .map(|(cell, family)| match (cell, family) {
                    (Some(text), TypeFamily::Decimal) => Some(decimal_value(text)),
                    (Some(text), TypeFamily::Float) => Some(float_value(text)),
                    _ => cell.clone(),
                })
                .collect()
        })
        .collect()
}

/// Floating point number `text` written as Rust writes its value, the
/// shortest text that reads it back (`-0` kept); `text` when it is not a
/// number.
fn float_value(text: &str) -> String {
    text.parse::<f64>()
        .map_or_else(|_| text.to_string(), |value| value.to_string())
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

/// Answer of the query of `case` on `server`, its input.
async fn run_input(case: &Case, server: &Server) -> Run {
    let mut connection = session(server, &case.session).await?;
    answer(&mut connection, &case.sql).await
}

/// Transpilations of `case` from the servers of `inputs` to `server`, and
/// the answer of each distinct text on `server`.
async fn run_output(
    case: &Case,
    inputs: &[Image],
    server: &Server,
) -> (Vec<Transpiled>, HashMap<String, Run>) {
    let mut runs = HashMap::new();
    let mut target = match session(server, &case.session).await {
        Ok(connection) => connection,
        Err(error) => {
            let failed = inputs
                .iter()
                .map(|_| Transpiled::Failed(error.clone()))
                .collect();
            return (failed, runs);
        }
    };
    let mut transpiled = Vec::with_capacity(inputs.len());
    for input in inputs {
        let outcome = transpile(case, input, server, &mut target)
            .await
            .unwrap_or_else(Transpiled::Failed);
        if let Transpiled::Sql(sql) = &outcome
            && !runs.contains_key(sql)
        {
            let run = answer(&mut target, sql).await;
            runs.insert(sql.clone(), run);
        }
        transpiled.push(outcome);
    }
    (transpiled, runs)
}

/// Transpilation of `case` from the server of `input` to `output`, whose
/// session `target` gives the SQL mode, the columns of the tables read and
/// the types of the user variables read.
async fn transpile(
    case: &Case,
    input: &Image,
    output: &Server,
    target: &mut Connection,
) -> Result<Transpiled, String> {
    let info = server_info(target)
        .await
        .map_err(|error| error.to_string())?;
    let translation = prepare(
        &case.sql,
        InputDialect {
            dialect: input.dialect,
            version: input.version,
        },
        OutputDialect {
            dialect: output.image.dialect,
            version: output.image.version,
            mode: SqlMode::parse(&info.sql_mode),
        },
    )
    .map_err(|error| format!("not read: {error}"))?;
    let mut tables = Tables::default();
    for table in translation.tables() {
        let database = table.database.as_ref().map(|name| name.value.as_str());
        let columns = catalog::table_columns(target, info.flavor, database, &table.name.value)
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
    let names: Vec<String> = translation
        .user_variables()
        .into_iter()
        .map(|name| name.value)
        .collect();
    let variables = catalog::user_variable_types(target, &names)
        .await
        .map_err(|error| error.to_string())?;
    for (name, ty) in variables {
        tables.insert_user_variable(&name, ty);
    }
    let transpilation = translation
        .transpile(&tables)
        .map_err(|error| format!("not built: {error}"))?;
    let rules: Vec<&str> = transpilation
        .errors
        .iter()
        .map(|error| error.rule)
        .collect();
    Ok(match (&case.refused, rules.is_empty()) {
        (Some(rule), false) if rules.contains(&rule.as_str()) => Transpiled::Refused,
        (Some(rule), true) => Transpiled::Failed(format!(
            "translated, `{rule}` expected: {}",
            transpilation.sql
        )),
        (_, false) => Transpiled::Failed(format!("refused: {:?}", transpilation.errors)),
        (None, true) => Transpiled::Sql(transpilation.sql),
    })
}

/// Index of a case: its file, then its place in the file.
type CaseKey = (usize, usize);

/// What a task of a group gives.
enum Done {
    /// Answer of a case on an input, the index of its image.
    Input(CaseKey, usize, Run),
    /// Transpilations of a case towards an output, the index of its image,
    /// for each of its inputs, and the answers of their texts.
    Output(CaseKey, usize, Vec<Transpiled>, HashMap<String, Run>),
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires Docker Desktop running"]
async fn transpilations_give_the_same_result() {
    let images = images();
    let files = Arc::new(case_files());
    let group = std::env::var("PRISME_CONFORMANCE_GROUP").map_or(DEFAULT_GROUP, |text| {
        text.parse()
            .ok()
            .filter(|&size| size > 0)
            .expect("PRISME_CONFORMANCE_GROUP: a number of servers")
    });
    // The inputs and the outputs of each case, as indexes of `images`.
    let mut servers_of: HashMap<CaseKey, (Vec<usize>, Vec<usize>)> = HashMap::new();
    let mut failures = Vec::new();
    for (file_index, (path, file)) in files.iter().enumerate() {
        for (case_index, case) in file.cases.iter().enumerate() {
            let from = dialect_named(&case.from);
            let kept = |image: &Image, condition: &Option<String>, input: bool| {
                (image.dialect == from) == input
                    && condition
                        .as_deref()
                        .map(Selection::parse)
                        .is_none_or(|condition| condition.keeps(image))
            };
            let indexes = |condition: &Option<String>, input: bool| -> Vec<usize> {
                (0..images.len())
                    .filter(|&index| kept(&images[index], condition, input))
                    .collect()
            };
            let (inputs, outputs) = (indexes(&case.inputs, true), indexes(&case.outputs, false));
            if inputs.is_empty() || outputs.is_empty() {
                let name = path.file_name().unwrap().to_string_lossy();
                let key = (file_index, case_index, 0, 0);
                failures.push((key, format!("{name}: no server for {}", case.sql)));
            }
            servers_of.insert((file_index, case_index), (inputs, outputs));
        }
    }
    let servers_of = Arc::new(servers_of);
    let mut expected: HashMap<(CaseKey, usize), Run> = HashMap::new();
    let mut transpiled: HashMap<(CaseKey, usize, usize), Transpiled> = HashMap::new();
    let mut got: HashMap<(CaseKey, usize, String), Run> = HashMap::new();
    for (chunk, group_images) in images.chunks(group).enumerate() {
        let first = chunk * group;
        let servers = start_servers(group_images).await;
        for server in &servers {
            let mut connection = session(server, &[]).await.unwrap();
            let sample: Vec<String> = SAMPLE.iter().map(ToString::to_string).collect();
            run_all(&mut connection, &sample)
                .await
                .unwrap_or_else(|error| panic!("{}: {error}", server.image.label()));
            for (path, file) in files.iter() {
                run_all(&mut connection, &file.setup)
                    .await
                    .unwrap_or_else(|error| {
                        panic!("{} on {}: {error}", path.display(), server.image.label())
                    });
            }
        }
        let servers = Arc::new(servers);
        let images = Arc::new(images.clone());
        let permits = Arc::new(Semaphore::new(PARALLEL_CHECKS));
        let mut tasks = JoinSet::new();
        for (offset, index) in (first..first + servers.len()).enumerate() {
            for (key, (inputs, outputs)) in servers_of.iter() {
                let (servers, files, images, permits, servers_of) = (
                    Arc::clone(&servers),
                    Arc::clone(&files),
                    Arc::clone(&images),
                    Arc::clone(&permits),
                    Arc::clone(&servers_of),
                );
                let key = *key;
                let (input, output) = (inputs.contains(&index), outputs.contains(&index));
                if !input && !output {
                    continue;
                }
                tasks.spawn(async move {
                    let _permit = permits.acquire().await.expect("open semaphore");
                    let case = &files[key.0].1.cases[key.1];
                    let server = &servers[offset];
                    if input {
                        return Done::Input(key, index, run_input(case, server).await);
                    }
                    let inputs: Vec<Image> = servers_of[&key]
                        .0
                        .iter()
                        .map(|&input| images[input].clone())
                        .collect();
                    let (outcomes, runs) = run_output(case, &inputs, server).await;
                    Done::Output(key, index, outcomes, runs)
                });
            }
        }
        while let Some(done) = tasks.join_next().await {
            match done.expect("check task") {
                Done::Input(key, index, run) => {
                    expected.insert((key, index), run);
                }
                Done::Output(key, index, outcomes, runs) => {
                    for (&input, outcome) in servers_of[&key].0.iter().zip(outcomes) {
                        transpiled.insert((key, input, index), outcome);
                    }
                    for (sql, run) in runs {
                        got.insert((key, index, sql), run);
                    }
                }
            }
        }
        let servers = Arc::into_inner(servers).expect("tasks of the group done");
        for server in servers {
            server.remove().await;
        }
    }
    let (mut same, mut refused) = (0, 0);
    for ((key, input, output), outcome) in transpiled {
        let (path, file) = &files[key.0];
        let case = &file.cases[key.1];
        let failure = match outcome {
            Transpiled::Refused => {
                refused += 1;
                continue;
            }
            Transpiled::Failed(failure) => failure,
            Transpiled::Sql(sql) => {
                let failure = match (&expected[&(key, input)], &got[&(key, output, sql.clone())]) {
                    (Err(error), _) => format!("{}: {error}", images[input].label()),
                    (_, Err(error)) => format!("{}: {error}", images[output].label()),
                    (Ok(expected), Ok(got)) => match difference(expected, got, case.compare) {
                        Some(difference) => difference,
                        None => {
                            same += 1;
                            continue;
                        }
                    },
                };
                format!("{failure}\n  transpiled {sql}")
            }
        };
        let name = path.file_name().unwrap().to_string_lossy();
        let message = format!(
            "{name}: {} -> {}: {}\n  {failure}",
            images[input].label(),
            images[output].label(),
            case.sql
        );
        failures.push(((key.0, key.1, input, output), message));
    }
    failures.sort_by_key(|(key, _)| *key);
    let failures: Vec<String> = failures.into_iter().map(|(_, message)| message).collect();
    println!(
        "{} servers, {same} same results, {refused} refusals, {} failures",
        images.len(),
        failures.len()
    );
    assert!(failures.is_empty(), "\n{}", failures.join("\n\n"));
}
