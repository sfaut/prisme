//! Files created when a connection is saved: the empty databases its user
//! asked for (`NewFileDatabase::create`), each at a location that must not
//! exist yet (`docs/TODO-SQLITE.md`, decision 5). They are created before the
//! connection is written, and removed again if it cannot be: a session then
//! only opens files that exist.

use std::path::PathBuf;

use prisme_connectors::location::{Scheme, locations};

use crate::connections::{ConnectionsError, NewConnection, NewSettings};
use crate::error_detail::ErrorDetail;

/// Files created for a connection being saved, removed by `discard` if it
/// is not saved.
#[must_use = "the files are to discard if the connection is not saved"]
pub struct NewFiles {
    /// Paths of the files created.
    paths: Vec<PathBuf>,
}

impl NewFiles {
    /// Removes the files created: the connection was not saved.
    pub fn discard(self) {
        for path in self.paths {
            if let Err(error) = std::fs::remove_file(&path) {
                tracing::warn!("cannot remove {}: {error}", path.display());
            }
        }
    }
}

/// Creates the empty databases `connection` asks for, validated; at the
/// first failure, removes those created already.
pub async fn create(connection: &NewConnection) -> Result<NewFiles, ConnectionsError> {
    let NewSettings::Files(files) = &connection.settings else {
        return Ok(NewFiles { paths: Vec::new() });
    };
    let mut created = NewFiles { paths: Vec::new() };
    for (database, create) in files.databases()? {
        if !create {
            continue;
        }
        let location = &database.location;
        if let Err(error) = locations::of(location.scheme).create(location).await {
            created.discard();
            return Err(ConnectionsError::Invalid(ErrorDetail::Location {
                schema: database.schema,
                error,
            }));
        }
        // Only a file is created (`NewFilesSettings::databases`).
        if location.scheme == Scheme::File {
            created.paths.push(PathBuf::from(&location.address));
        }
    }
    Ok(created)
}

#[cfg(test)]
mod tests {
    use prisme_connectors::api::Engine;
    use prisme_connectors::location::LocationError;

    use super::*;
    use crate::connections::{NewFileDatabase, NewFilesSettings};

    /// Connection of a main database in memory and one at `path`, to create.
    fn connection(path: &std::path::Path) -> NewConnection {
        NewConnection {
            name: "files".to_string(),
            engine: Engine::Sqlite,
            settings: NewSettings::Files(NewFilesSettings {
                optimize: false,
                databases: vec![
                    NewFileDatabase {
                        schema: "main".to_string(),
                        location: ":memory:".to_string(),
                        create: false,
                    },
                    NewFileDatabase {
                        schema: "other".to_string(),
                        location: path.to_string_lossy().into_owned(),
                        create: true,
                    },
                ],
            }),
            init_sql: String::new(),
            session_timeout_minutes: 5,
            show_system_schemas: true,
        }
    }

    #[tokio::test]
    async fn creates_the_files_asked_and_refuses_one_that_exists() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("new.db");
        let created = create(&connection(&path)).await.unwrap();
        assert!(path.exists());
        assert!(matches!(
            create(&connection(&path)).await,
            Err(ConnectionsError::Invalid(ErrorDetail::Location {
                error: LocationError::AlreadyExists { .. },
                ..
            }))
        ));
        assert!(path.exists(), "a file that existed stays");
        created.discard();
        assert!(!path.exists());
    }
}
