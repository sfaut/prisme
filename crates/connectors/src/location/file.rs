//! Provider of the `file` protocol: a file the process of Prisme reaches,
//! read and written where it is.

use std::io::ErrorKind;
use std::path::PathBuf;

use futures_util::future::BoxFuture;

use super::{
    Local, Location, LocationError, LocationProvider, LocationReport, ProviderCapabilities, Remote,
    iso_8601,
};

/// Provider of the files, for the registry (`locations::of`).
pub static PROVIDER: FileProvider = FileProvider;

/// Provider of the files reachable by the process of Prisme.
pub struct FileProvider;

impl LocationProvider for FileProvider {
    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            writable: true,
            creatable: true,
            local: true,
        }
    }

    fn probe<'a>(
        &'a self,
        location: &'a Location,
        _remote: &'a Remote,
    ) -> BoxFuture<'a, Result<LocationReport, LocationError>> {
        Box::pin(async move {
            let metadata = match tokio::fs::metadata(&location.address).await {
                Ok(metadata) => metadata,
                Err(error) if error.kind() == ErrorKind::NotFound => {
                    return Ok(LocationReport {
                        exists: false,
                        size: None,
                        modified_at: None,
                        version_tag: None,
                        read_only: false,
                    });
                }
                Err(error) => return Err(unreachable(location, &error)),
            };
            if !metadata.is_file() {
                return Err(LocationError::Unreachable {
                    address: location.address.clone(),
                    message: "not a file".to_string(),
                });
            }
            Ok(LocationReport {
                exists: true,
                size: Some(metadata.len()),
                modified_at: metadata.modified().ok().map(iso_8601),
                version_tag: None,
                read_only: metadata.permissions().readonly(),
            })
        })
    }

    fn create<'a>(&'a self, location: &'a Location) -> BoxFuture<'a, Result<(), LocationError>> {
        Box::pin(async move {
            // An empty file is an empty SQLite database; `create_new` refuses
            // a file that exists, even created in between.
            let created = tokio::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&location.address)
                .await;
            match created {
                Ok(_) => Ok(()),
                Err(error) if error.kind() == ErrorKind::AlreadyExists => {
                    Err(LocationError::AlreadyExists {
                        address: location.address.clone(),
                    })
                }
                Err(error) => Err(unreachable(location, &error)),
            }
        })
    }

    fn materialize<'a>(
        &'a self,
        location: &'a Location,
        _remote: &'a Remote,
    ) -> BoxFuture<'a, Result<Local, LocationError>> {
        Box::pin(async move {
            Ok(Local::File {
                path: PathBuf::from(&location.address),
                read_only: false,
            })
        })
    }
}

/// Error of a file that could not be reached, with the system's cause.
fn unreachable(location: &Location, error: &std::io::Error) -> LocationError {
    LocationError::Unreachable {
        address: location.address.clone(),
        message: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::location::{RemoteLimits, Scheme};

    fn remote() -> Remote {
        Remote {
            cache_dir: PathBuf::new(),
            limits: RemoteLimits::default(),
        }
    }

    #[tokio::test]
    async fn creates_a_missing_file_once_and_reports_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("new.db");
        let location = Location::parse(&path.to_string_lossy()).unwrap();
        assert_eq!(location.scheme, Scheme::File);

        let report = PROVIDER.probe(&location, &remote()).await.unwrap();
        assert!(!report.exists);
        PROVIDER.create(&location).await.unwrap();
        let report = PROVIDER.probe(&location, &remote()).await.unwrap();
        assert_eq!((report.exists, report.size), (true, Some(0)));
        assert!(report.modified_at.unwrap().ends_with('Z'));
        assert!(matches!(
            PROVIDER.create(&location).await,
            Err(LocationError::AlreadyExists { .. })
        ));
        assert_eq!(
            PROVIDER.materialize(&location, &remote()).await.unwrap(),
            Local::File {
                path,
                read_only: false
            }
        );
    }

    #[tokio::test]
    async fn refuses_a_folder_and_a_missing_parent() {
        let dir = tempfile::tempdir().unwrap();
        let folder = Location::parse(&dir.path().to_string_lossy()).unwrap();
        assert!(matches!(
            PROVIDER.probe(&folder, &remote()).await,
            Err(LocationError::Unreachable { .. })
        ));
        let orphan = dir.path().join("missing").join("v.db");
        let orphan = Location::parse(&orphan.to_string_lossy()).unwrap();
        assert!(matches!(
            PROVIDER.create(&orphan).await,
            Err(LocationError::Unreachable { .. })
        ));
    }
}
