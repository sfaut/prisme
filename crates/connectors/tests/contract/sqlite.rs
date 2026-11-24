//! Fixture of SQLite: a main database and a second one attached as `other`,
//! two empty files of a temporary folder of their own per test.

use std::path::Path;

use prisme_connectors::api::{
    Engine, FileDatabase, FilesSettings, MAIN_SCHEMA, Namespace, Settings, Target,
};
use prisme_connectors::flavor::Flavor;
use prisme_connectors::location::{Location, Remote, RemoteLimits, Scheme};

use super::suite::Fixture;

/// Schema of the second database.
const OTHER_SCHEMA: &str = "other";

/// SQLite, embedded in Prisme.
pub struct Sqlite;

/// The single SQLite of Prisme.
pub fn fixtures() -> impl Iterator<Item = Sqlite> {
    std::iter::once(Sqlite)
}

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

impl Fixture for Sqlite {
    fn name(&self) -> String {
        "SQLite".to_string()
    }

    fn flavor(&self) -> Flavor {
        Flavor::Sqlite
    }

    async fn with_source(&self, _name: &str, test: impl AsyncFnOnce(Target, Namespace)) {
        let folder = tempfile::tempdir().unwrap();
        let main = folder.path().join("main.db");
        let other = folder.path().join("other.db");
        // An empty file is an empty database: the connector creates none.
        for path in [&main, &other] {
            std::fs::File::create(path).unwrap();
        }
        let target = Target {
            engine: Engine::Sqlite,
            settings: Settings::Files(FilesSettings {
                databases: vec![file(MAIN_SCHEMA, &main), file(OTHER_SCHEMA, &other)],
                optimize: false,
                remote: Remote {
                    cache_dir: folder.path().join("cache"),
                    limits: RemoteLimits::default(),
                },
            }),
            init_statements: Vec::new(),
        };
        test(target, Namespace::single(OTHER_SCHEMA)).await;
    }

    fn long_statement(&self) -> &'static str {
        "WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c) SELECT count(*) FROM c"
    }
}
