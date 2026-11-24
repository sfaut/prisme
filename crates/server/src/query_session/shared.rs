//! What the connections of a session share, with the navigator and the
//! exports of its editor: the key that names its resources (its databases in
//! memory, `OpenOptions::session_key`), and the databases attached to the
//! session's connection, which each of its other connections attaches too.
//!
//! The session lists the databases of its connection when it opens, then
//! after each statement that attaches or detaches one (`Session::databases`).
//! A database private to its connection (an `ATTACH ':memory:'` written in
//! SQL) cannot be attached elsewhere: the session then runs its batches in
//! sequence, on its own connection.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard};

use prisme_connectors::api::{AttachedDatabase, FileDatabase, OpenOptions, Settings, Target};

/// Resources of a session, shared by its connections.
pub(crate) struct Shared {
    /// Key of the session (`OpenOptions::session_key`).
    key: String,
    /// Databases of the session's connection that another connection can
    /// attach, as last listed; `None` before, and for an engine without
    /// attachments: those of the settings.
    databases: Mutex<Option<Vec<FileDatabase>>>,
    /// Databases of the session's connection, as last listed, those private
    /// to it included.
    listed: Mutex<Vec<AttachedDatabase>>,
    /// Does the session's connection hold a database private to it?
    private: AtomicBool,
}

impl Shared {
    /// Resources of the session of key `key`, its databases not listed yet.
    pub(crate) fn new(key: String) -> Self {
        Self {
            key,
            databases: Mutex::new(None),
            listed: Mutex::new(Vec::new()),
            private: AtomicBool::new(false),
        }
    }

    /// `options` of a connection of the session: on its key.
    pub(crate) fn options(&self, options: OpenOptions) -> OpenOptions {
        OpenOptions {
            session_key: Some(self.key.clone()),
            ..options
        }
    }

    /// `target` as a connection of the session opens it: on the databases
    /// of the session's connection, as last listed.
    pub(crate) fn target(&self, target: &Target) -> Target {
        let mut target = target.clone();
        if let (Settings::Files(files), Some(databases)) =
            (&mut target.settings, self.databases().as_ref())
        {
            files.databases.clone_from(databases);
        }
        target
    }

    /// Remembers the databases `listed` of the session's connection, and
    /// returns whether they changed since the last list.
    pub(crate) fn follow(&self, listed: &[AttachedDatabase]) -> bool {
        let databases: Vec<FileDatabase> = listed
            .iter()
            .filter_map(|database| {
                let location = database.location.clone()?;
                Some(FileDatabase {
                    schema: database.schema.clone(),
                    location,
                })
            })
            .collect();
        let private = listed.iter().any(|database| database.location.is_none());
        let was_private = self.private.swap(private, Ordering::Relaxed);
        let mut current = self.databases();
        let changed = current.as_ref() != Some(&databases) || was_private != private;
        *current = Some(databases);
        *self
            .listed
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = listed.to_vec();
        changed
    }

    /// Databases of the session's connection, as last listed, those private
    /// to it included.
    pub(crate) fn listed(&self) -> Vec<AttachedDatabase> {
        self.listed
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    /// Does the session's connection hold a database that no other
    /// connection can attach?
    pub(crate) fn private(&self) -> bool {
        self.private.load(Ordering::Relaxed)
    }

    /// Databases as last listed; a poisoned lock is taken over as is (each
    /// change fits in a single assignment).
    fn databases(&self) -> MutexGuard<'_, Option<Vec<FileDatabase>>> {
        self.databases
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use prisme_connectors::api::{Engine, FilesSettings};
    use prisme_connectors::location::{Location, Remote, RemoteLimits, Scheme};

    use super::*;

    /// Database `schema` of the file `path`.
    fn file(schema: &str, path: &str) -> FileDatabase {
        FileDatabase {
            schema: schema.to_string(),
            location: Location {
                scheme: Scheme::File,
                address: path.to_string(),
            },
        }
    }

    /// Listed database of `database`.
    fn listed(database: &FileDatabase) -> AttachedDatabase {
        AttachedDatabase {
            schema: database.schema.clone(),
            location: Some(database.location.clone()),
        }
    }

    #[test]
    fn opens_the_connections_on_the_databases_last_listed() {
        let main = file("main", "/data/main.db");
        let saved = Target {
            engine: Engine::Sqlite,
            settings: Settings::Files(FilesSettings {
                databases: vec![main.clone(), file("gone", "/data/gone.db")],
                remote: Remote {
                    cache_dir: std::env::temp_dir(),
                    limits: RemoteLimits::default(),
                },
            }),
            init_statements: Vec::new(),
        };
        let shared = Shared::new("session-1".to_string());
        let databases = |target: Target| {
            let Settings::Files(files) = target.settings else {
                unreachable!("a source held in files");
            };
            files.databases
        };
        assert_eq!(databases(shared.target(&saved)), databases(saved.clone()));

        let other = file("other", "/data/other.db");
        assert!(shared.follow(&[listed(&main), listed(&other)]));
        assert!(!shared.follow(&[listed(&main), listed(&other)]));
        assert_eq!(databases(shared.target(&saved)), [main.clone(), other]);
        assert!(!shared.private());

        let private = AttachedDatabase {
            schema: "scratch".to_string(),
            location: None,
        };
        assert!(shared.follow(&[listed(&main), private.clone()]));
        assert!(shared.private());
        assert_eq!(shared.listed(), [listed(&main), private]);
    }
}
