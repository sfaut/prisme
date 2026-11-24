//! Purge of the cache of the remote databases.
//!
//! A remote database (`https://…`) is read from a copy in the cache of its
//! user, `cache/locations/<user id>` in the data folder
//! (`connections::load_target`). The copies that no session opened for the
//! `remote_cache_days` of their user leave the cache (`location::purge`), at
//! startup, then every hour.

use std::path::Path;
use std::time::Duration;

use prisme_connectors::location;
use rusqlite::Connection;

use crate::api::AppState;
use crate::settings::SettingsError;

/// Time between two purges.
const PURGE_INTERVAL: Duration = Duration::from_secs(3600);

/// Seconds in a day.
const DAY_SECONDS: u64 = 86_400;

/// Users and the days after which a copy they no longer open leaves their
/// cache.
fn retentions(db: &Connection) -> rusqlite::Result<Vec<(i64, u32)>> {
    let mut statement = db.prepare("SELECT id, remote_cache_days FROM users ORDER BY id")?;
    let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
    rows.collect()
}

/// Purges the cache of each user of `retentions` in the cache folder
/// `locations`, and returns the number of files deleted. A folder that
/// cannot be read is logged and left for the next purge.
async fn purge(locations: &Path, retentions: &[(i64, u32)]) -> usize {
    let mut deleted = 0;
    for &(user_id, days) in retentions {
        let folder = locations.join(user_id.to_string());
        let unused_for = Duration::from_secs(u64::from(days) * DAY_SECONDS);
        match location::purge(&folder, unused_for).await {
            Ok(count) => deleted += count,
            Err(error) => tracing::warn!("cannot purge {}: {error}", folder.display()),
        }
    }
    deleted
}

/// Purges the caches of the users now, then every `PURGE_INTERVAL`.
pub async fn purge_regularly(state: AppState) {
    let mut interval = tokio::time::interval(PURGE_INTERVAL);
    loop {
        interval.tick().await;
        let reading = state.with_db(|db, _| retentions(db).map_err(SettingsError::from));
        let Ok(retentions) = reading.await else {
            continue;
        };
        let deleted = purge(&state.locations(), &retentions).await;
        if deleted > 0 {
            tracing::debug!("remote cache: {deleted} files unused purged");
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::store;

    #[tokio::test]
    async fn purges_the_cache_of_each_user() {
        let db = store::open_in_memory();
        let first = store::ensure_user(&db, "first@example.com").unwrap();
        let second = store::ensure_user(&db, "second@example.com").unwrap();
        let retentions = super::retentions(&db).unwrap();
        // 30 days by default (migration 0039).
        assert_eq!(retentions, [(first, 30), (second, 30)]);

        let locations = tempfile::tempdir().unwrap();
        // A copy that no record names: left by a download cut.
        let copies = [first, second].map(|user_id| {
            let folder = locations.path().join(user_id.to_string());
            std::fs::create_dir(&folder).unwrap();
            let copy = folder.join("orphan.db");
            std::fs::write(&copy, "").unwrap();
            copy
        });
        assert_eq!(super::purge(locations.path(), &retentions).await, 2);
        assert!(copies.iter().all(|copy| !copy.exists()));
        // A user without a cache folder: nothing to purge.
        assert_eq!(super::purge(locations.path(), &[(99, 30)]).await, 0);
    }
}
