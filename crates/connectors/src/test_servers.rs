//! The servers of the tests on real servers: the six MySQL and MariaDB
//! development servers of `compose.yaml`, started beforehand by
//! `docker compose up --detach --wait` (`docs/DEVELOPER.md`, section
//! "Development servers"). For each engine: the oldest version Prisme
//! supports, the latest LTS and the latest version.
//!
//! The tests of a run share these servers: each test works in a database of
//! its own, created for it, then dropped.
//!
//! Only built for the tests, through the `test-servers` feature.

use secrecy::SecretString;

use crate::mysql::{Connection, Flavor, MysqlTarget, connect, execute};

/// A development server of `compose.yaml`.
#[derive(Debug, Clone, Copy)]
pub struct Server {
    /// Name of its service in `compose.yaml`.
    pub name: &'static str,
    /// Engine of the server.
    pub flavor: Flavor,
    /// Port published on 127.0.0.1.
    pub port: u16,
}

/// The servers of `compose.yaml`, each engine from its oldest version.
pub const SERVERS: [Server; 6] = [
    Server {
        name: "mysql-oldest",
        flavor: Flavor::MySql,
        port: 3311,
    },
    Server {
        name: "mysql-lts",
        flavor: Flavor::MySql,
        port: 3312,
    },
    Server {
        name: "mysql-latest",
        flavor: Flavor::MySql,
        port: 3313,
    },
    Server {
        name: "mariadb-oldest",
        flavor: Flavor::MariaDb,
        port: 3321,
    },
    Server {
        name: "mariadb-lts",
        flavor: Flavor::MariaDb,
        port: 3322,
    },
    Server {
        name: "mariadb-latest",
        flavor: Flavor::MariaDb,
        port: 3323,
    },
];

/// The servers of one engine.
pub fn of(flavor: Flavor) -> impl Iterator<Item = Server> {
    SERVERS
        .into_iter()
        .filter(move |server| server.flavor == flavor)
}

/// Settings of a connection to port 1 of the machine, where no server
/// listens: for the tests that do not reach a server, or that fail to.
pub fn unreachable() -> MysqlTarget {
    MysqlTarget {
        port: 1,
        ..SERVERS[0].target("")
    }
}

impl Server {
    /// Settings of a connection to `database` (empty: none), root account
    /// without a password.
    pub fn target(&self, database: &str) -> MysqlTarget {
        MysqlTarget {
            host: "127.0.0.1".to_string(),
            port: self.port,
            username: "root".to_string(),
            password: SecretString::from(""),
            database_name: database.to_string(),
            init_statements: Vec::new(),
        }
    }

    /// Runs `test` with the settings of a connection to the database
    /// `test_<name>`, created again, empty, for it, then drops the database,
    /// so that the servers only show their demo databases. After a failure,
    /// the database stays for the investigation, until the next run. Writes
    /// the server and the database on the error output, shown with the
    /// failure of a test.
    ///
    /// # Panics
    ///
    /// When the server does not answer, with the command that starts it.
    pub async fn with_database(&self, name: &str, test: impl AsyncFnOnce(MysqlTarget)) {
        let database = format!("test_{name}");
        eprintln!("{}: database {database}", self.name);
        let mut connection = connect(&self.target("")).await.unwrap_or_else(|error| {
            panic!(
                "{} does not answer on port {}: {error}\n\
                 Start the development servers: docker compose up --detach --wait",
                self.name, self.port
            )
        });
        // A connection of the test left in a transaction would make the
        // DROP DATABASE wait for a year: an error after 10 seconds instead.
        run(&mut connection, "SET SESSION lock_wait_timeout = 10").await;
        run(
            &mut connection,
            &format!("DROP DATABASE IF EXISTS {database}"),
        )
        .await;
        run(&mut connection, &format!("CREATE DATABASE {database}")).await;
        test(self.target(&database)).await;
        run(&mut connection, &format!("DROP DATABASE {database}")).await;
    }
}

/// Runs a statement that prepares or ends a test.
async fn run(connection: &mut Connection, sql: &str) {
    execute(connection, sql).await.unwrap().discard().await;
}
