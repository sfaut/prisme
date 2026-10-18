//! Measurements on real MySQL and MariaDB servers, for the research of the
//! dialects (`docs/DEV-DIALECTS.md`): the servers run in Docker containers,
//! one per image (`server` module), the images named or all the local ones
//! (`--all`), by groups of a few servers at once (`--group`): a group is
//! started, measured, then its containers stopped, before the next one. Many
//! servers started at once would no longer answer.
//!
//! - `sql`: runs statements on each server and prints their output;
//! - `expr`: measures expressions on each server and marks the servers that
//!   differ (`expr` module);
//! - `functions`: checks the built-in functions of a reference file against
//!   the servers of its dialect (`functions` module);
//! - `calls`: writes the cases of `expr` that call each function of a
//!   reference file with a fixed set of values (`calls` module).
//!
//! A development tool, outside the executable of Prisme. Docker must be
//! running.

mod calls;
mod expr;
mod functions;
mod server;

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::PathBuf;
use std::thread;

use anyhow::{Context, bail};
use clap::{Parser, Subcommand, ValueEnum};
use prisme_dialects::Dialect;

use crate::expr::{Cases, Layout};
use crate::server::Server;

/// Measurements on real MySQL and MariaDB servers, in Docker containers.
#[derive(Parser)]
#[command(name = "probe")]
struct Cli {
    /// Docker image of a server, repeated for several servers. Its container,
    /// `prisme-probe-<name>-<tag>`, is created when missing; a container the
    /// run started is stopped after its group, one already running is left
    /// running.
    #[arg(
        long = "server",
        global = true,
        default_values = ["mysql:8.0.11", "mysql:8.4", "mariadb:10.6", "mariadb:11.8"]
    )]
    servers: Vec<String>,
    /// Every local image of MySQL and MariaDB (`docker images`), one per
    /// server, instead of `--server`.
    #[arg(long, global = true, conflicts_with = "servers")]
    all: bool,
    /// Number of servers started and measured at once; the containers a
    /// group started are stopped after it.
    #[arg(long, global = true, default_value_t = 5, value_parser = clap::value_parser!(u16).range(1..))]
    group: u16,
    /// What to do.
    #[command(subcommand)]
    command: Command,
}

/// Commands of the tool.
#[derive(Subcommand)]
enum Command {
    /// Runs SQL statements on each server, in the database `probe`, and
    /// prints their output as tables; an error does not stop the next
    /// statements.
    Sql {
        /// File of statements; the standard input when absent or `-`.
        file: Option<PathBuf>,
        /// Also prints the metadata of the columns (`--column-type-info`).
        #[arg(long)]
        types: bool,
        /// Also prints each statement before its output.
        #[arg(long)]
        verbose: bool,
    },
    /// Measures SQL expressions on each server: type and nullability of
    /// their column, value, first warning or error; `*` marks the servers
    /// that differ.
    ///
    /// The file holds one case per line, `id expression`; an id starting
    /// with `~` for an expression whose value changes at each run (only its
    /// type is compared); `! statement` for a statement run first (`! SET
    /// time_zone = '+01:00'`), again in the session that reads the values;
    /// `#` for a comment. The expressions read the
    /// one row of the table `sample`: d DATE, dt DATETIME(3), ts
    /// TIMESTAMP(6), t TIME, s VARCHAR(30), i INT, n DECIMAL(10,2), f DOUBLE,
    /// all NOT NULL, then the nullable columns dn DATE and sn VARCHAR(30),
    /// both NULL, and j JSON.
    Expr {
        /// File of cases.
        file: Option<PathBuf>,
        /// Expression to measure, after those of the file (id `e1`, `e2`...).
        #[arg(short = 'e', long = "expression")]
        expressions: Vec<String>,
        /// Statement run before the measurements, after those of the file.
        #[arg(long)]
        setup: Vec<String>,
        /// Only prints the cases where the servers differ.
        #[arg(long)]
        differ: bool,
        /// Prints a Markdown table.
        #[arg(long)]
        markdown: bool,
        /// Layout of the results: a column per server, or a line per server
        /// under each case (`rows`), for many servers.
        #[arg(long, value_enum, default_value_t = Layout::Columns)]
        layout: Layout,
        /// Characters of a cell, cut beyond (0: whole).
        #[arg(long, default_value_t = 70)]
        width: usize,
    },
    /// Checks the built-in functions of the reference file of a dialect
    /// against the servers of that dialect: periods, names that call a
    /// stored function, functions of the server absent from the file.
    Functions {
        /// Dialect of the reference file.
        dialect: DialectName,
    },
    /// Writes a file of cases of `expr` that calls each function of the
    /// reference file of a dialect: with each number of arguments, up to 3,
    /// that reaches it on the servers of that dialect (the smallest one when
    /// all are beyond), then with each value of a fixed set at each argument
    /// in turn.
    Calls {
        /// Dialect of the reference file.
        dialect: DialectName,
    },
}

/// Dialect named on the command line.
#[derive(Clone, Copy, ValueEnum)]
enum DialectName {
    /// `crates/dialects/reference/mysql.yaml`.
    Mysql,
    /// `crates/dialects/reference/mariadb.yaml`.
    Mariadb,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let images = if cli.all {
        server::local_images()?
    } else {
        cli.servers.clone()
    };
    let group = usize::from(cli.group);
    match cli.command {
        Command::Sql {
            file,
            types,
            verbose,
        } => {
            let script = read(file.as_ref())?;
            let mut options = vec!["--table", "--force"];
            if types {
                options.push("--column-type-info");
            }
            if verbose {
                options.push("-vvv");
            }
            let outputs = by_groups(&images, group, |server| server.run(&script, &options))?;
            for (server, output) in &outputs {
                println!("== {} ({})", server.label(), server.container);
                print!("{}", String::from_utf8_lossy(&output.stdout));
                let errors = String::from_utf8_lossy(&output.stderr);
                if !errors.is_empty() {
                    println!("{errors}");
                }
            }
        }
        Command::Expr {
            file,
            expressions,
            setup,
            differ,
            markdown,
            layout,
            width,
        } => {
            let mut cases = match &file {
                Some(_) => Cases::parse(&read(file.as_ref())?)?,
                None => Cases::default(),
            };
            cases.setup.extend(setup);
            for (index, expression) in expressions.iter().enumerate() {
                cases.push(&format!("e{}", index + 1), expression)?;
            }
            if cases.cases.is_empty() {
                bail!("no expression: give a file of cases or --expression");
            }
            let (servers, measures): (Vec<_>, Vec<_>) =
                by_groups(&images, group, |server| expr::measure(server, &cases))?
                    .into_iter()
                    .unzip();
            let render = match layout {
                Layout::Columns => expr::render,
                Layout::Rows => expr::render_rows,
            };
            print!(
                "{}",
                render(&servers, &cases, &measures, markdown, differ, width)
            );
        }
        Command::Functions { dialect } => {
            let dialect = dialect.dialect();
            let reports = by_groups(&of_dialect(&images, dialect)?, group, |server| {
                functions::check(dialect, std::slice::from_ref(server))
            })?;
            let reports: Vec<String> = reports.into_iter().map(|(_, report)| report).collect();
            print!("{}", reports.concat());
        }
        Command::Calls { dialect } => {
            let dialect = dialect.dialect();
            let measured = by_groups(&of_dialect(&images, dialect)?, group, |server| {
                calls::arities(dialect, server)
            })?;
            let mut arities: BTreeMap<String, BTreeSet<usize>> = BTreeMap::new();
            let mut labels = Vec::new();
            for (server, counts) in measured {
                labels.push(server.label());
                for (name, counts) in counts {
                    arities.entry(name).or_default().extend(counts);
                }
            }
            print!("{}", calls::cases(dialect, &labels, &arities));
        }
    }
    Ok(())
}

impl DialectName {
    /// Dialect named.
    fn dialect(self) -> Dialect {
        match self {
            Self::Mysql => Dialect::MySql,
            Self::Mariadb => Dialect::MariaDb,
        }
    }
}

/// Images of `images` whose servers are of `dialect`.
fn of_dialect(images: &[String], dialect: Dialect) -> anyhow::Result<Vec<String>> {
    let mut kept = Vec::new();
    for image in images {
        if server::dialect_of(image)? == dialect {
            kept.push(image.clone());
        }
    }
    if kept.is_empty() {
        bail!("no server of {}: add --server", dialect.label());
    }
    Ok(kept)
}

/// Content of `file`, or of the standard input when absent or `-`.
fn read(file: Option<&PathBuf>) -> anyhow::Result<String> {
    match file {
        Some(path) if path.as_os_str() != "-" => {
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))
        }
        _ => {
            let mut text = String::new();
            std::io::stdin()
                .read_to_string(&mut text)
                .context("reading the standard input")?;
            Ok(text)
        }
    }
}

/// Results of `task` on the servers of `images`, `group` servers at a time:
/// each group is started, then measured, in parallel, then the containers it
/// started are stopped, before the next group. Gives each server with its
/// result, in the order of `images`.
fn by_groups<R: Send>(
    images: &[String],
    group: usize,
    task: impl Fn(&Server) -> anyhow::Result<R> + Sync,
) -> anyhow::Result<Vec<(Server, R)>> {
    let mut results = Vec::with_capacity(images.len());
    for chunk in images.chunks(group) {
        let started = in_parallel(chunk, |image| Server::start(image));
        let servers: Vec<Server> = started.iter().flatten().cloned().collect();
        let outcomes = in_parallel(&servers, &task);
        for server in &servers {
            server.stop()?;
        }
        for result in started {
            result?;
        }
        for (server, outcome) in servers.into_iter().zip(outcomes) {
            results.push((server, outcome?));
        }
    }
    Ok(results)
}

/// Results of `task` over `items`, run in parallel, in the order of `items`.
fn in_parallel<T: Sync, R: Send>(
    items: &[T],
    task: impl Fn(&T) -> anyhow::Result<R> + Sync,
) -> Vec<anyhow::Result<R>> {
    thread::scope(|scope| {
        let handles: Vec<_> = items
            .iter()
            .map(|item| scope.spawn(|| task(item)))
            .collect();
        handles
            .into_iter()
            .map(|handle| {
                handle
                    .join()
                    .map_err(|_| anyhow::anyhow!("a measurement panicked"))?
            })
            .collect()
    })
}
