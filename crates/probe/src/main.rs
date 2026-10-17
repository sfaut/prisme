//! Measurements on real MySQL and MariaDB servers, for the research of the
//! dialects (`docs/DEV-DIALECTS.md`): the servers run in Docker containers,
//! one per image, started on demand and kept between runs (`server` module).
//!
//! - `sql`: runs statements on each server and prints their output;
//! - `expr`: measures expressions on each server and marks the servers that
//!   differ (`expr` module);
//! - `functions`: checks the built-in functions of a reference file against
//!   the servers of its dialect (`functions` module).
//!
//! A development tool, outside the executable of Prisme. Docker must be
//! running.

mod expr;
mod functions;
mod server;

use std::io::Read;
use std::path::PathBuf;
use std::thread;

use anyhow::{Context, bail};
use clap::{Parser, Subcommand, ValueEnum};
use prisme_dialects::Dialect;

use crate::expr::Cases;
use crate::server::Server;

/// Measurements on real MySQL and MariaDB servers, in Docker containers.
#[derive(Parser)]
#[command(name = "probe")]
struct Cli {
    /// Docker image of a server, repeated for several servers. Its container,
    /// `prisme-probe-<name>-<tag>`, is created when missing, then kept.
    #[arg(
        long = "server",
        global = true,
        default_values = ["mysql:8.0.11", "mysql:8.4", "mariadb:10.6", "mariadb:11.8"]
    )]
    servers: Vec<String>,
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
    match cli.command {
        Command::Sql {
            file,
            types,
            verbose,
        } => {
            let script = read(file.as_ref())?;
            let servers = start(&cli.servers)?;
            let mut options = vec!["--table", "--force"];
            if types {
                options.push("--column-type-info");
            }
            if verbose {
                options.push("-vvv");
            }
            for server in &servers {
                let output = server.run(&script, &options)?;
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
            let servers = start(&cli.servers)?;
            let measures = in_parallel(&servers, |server| expr::measure(server, &cases))?;
            print!(
                "{}",
                expr::render(&servers, &cases, &measures, markdown, differ, width)
            );
        }
        Command::Functions { dialect } => {
            let dialect = match dialect {
                DialectName::Mysql => Dialect::MySql,
                DialectName::Mariadb => Dialect::MariaDb,
            };
            let servers: Vec<Server> = start(&cli.servers)?
                .into_iter()
                .filter(|server| server.dialect == dialect)
                .collect();
            if servers.is_empty() {
                bail!("no server of {}: add --server", dialect.label());
            }
            let reports = in_parallel(&servers, |server| {
                functions::check(dialect, std::slice::from_ref(server))
            })?;
            print!("{}", reports.concat());
        }
    }
    Ok(())
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

/// Servers of `images`, started in parallel.
fn start(images: &[String]) -> anyhow::Result<Vec<Server>> {
    in_parallel(images, |image| Server::start(image))
}

/// Results of `task` over `items`, run in parallel, in the order of `items`.
fn in_parallel<T: Sync, R: Send>(
    items: &[T],
    task: impl Fn(&T) -> anyhow::Result<R> + Sync,
) -> anyhow::Result<Vec<R>> {
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
