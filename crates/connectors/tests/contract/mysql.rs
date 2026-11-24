//! Fixture of MySQL and MariaDB: each development server of `compose.yaml`
//! (`test_servers`), in a database of its own per test.

use prisme_connectors::api::{Namespace, Target};
use prisme_connectors::flavor::Flavor;
use prisme_connectors::mysql::{connect, execute};
use prisme_connectors::test_servers::{SERVERS, Server};

use super::suite::Fixture;

/// The six development servers.
pub fn fixtures() -> impl Iterator<Item = Server> {
    SERVERS.into_iter()
}

impl Fixture for Server {
    fn name(&self) -> String {
        self.name.to_string()
    }

    fn flavor(&self) -> Flavor {
        self.flavor
    }

    async fn with_source(&self, name: &str, test: impl AsyncFnOnce(Target, Namespace)) {
        let other = format!("test_{name}_other");
        self.with_database(name, async |target| {
            let mut connection = connect(&target).await.unwrap();
            for sql in [
                format!("DROP DATABASE IF EXISTS {other}"),
                format!("CREATE DATABASE {other}"),
            ] {
                execute(&mut connection, &sql)
                    .await
                    .unwrap()
                    .discard()
                    .await;
            }
            test(
                self.source(&target.database_name),
                Namespace::single(&other),
            )
            .await;
            let sql = format!("DROP DATABASE {other}");
            execute(&mut connection, &sql)
                .await
                .unwrap()
                .discard()
                .await;
        })
        .await;
    }

    fn long_statement(&self) -> &'static str {
        "SELECT SLEEP(30)"
    }
}
