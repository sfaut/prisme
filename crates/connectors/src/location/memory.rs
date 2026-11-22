//! Provider of the `memory` protocol: a database in memory, which the engine
//! holds and which lives with its session. There is nothing to reach,
//! create or copy.

use futures_util::future::BoxFuture;

use super::{
    Local, Location, LocationError, LocationProvider, LocationReport, ProviderCapabilities, Remote,
};

/// Provider of the databases in memory, for the registry (`locations::of`).
pub static PROVIDER: MemoryProvider = MemoryProvider;

/// Provider of the databases in memory.
pub struct MemoryProvider;

impl LocationProvider for MemoryProvider {
    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            writable: true,
            creatable: false,
            local: true,
        }
    }

    fn probe<'a>(
        &'a self,
        _location: &'a Location,
        _remote: &'a Remote,
    ) -> BoxFuture<'a, Result<LocationReport, LocationError>> {
        Box::pin(async move {
            Ok(LocationReport {
                exists: true,
                size: None,
                modified_at: None,
                version_tag: None,
                read_only: false,
            })
        })
    }

    fn create<'a>(&'a self, location: &'a Location) -> BoxFuture<'a, Result<(), LocationError>> {
        Box::pin(async move {
            Err(LocationError::NotCreatable {
                scheme: location.scheme,
            })
        })
    }

    fn materialize<'a>(
        &'a self,
        _location: &'a Location,
        _remote: &'a Remote,
    ) -> BoxFuture<'a, Result<Local, LocationError>> {
        Box::pin(async move { Ok(Local::Memory) })
    }
}
