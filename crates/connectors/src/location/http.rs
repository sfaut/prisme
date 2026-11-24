//! Provider of the `http` and `https` protocols: a file on a public web
//! server, read through a copy in the cache of the user.
//!
//! The copy of a location lives in the cache folder (`Remote::cache_dir`),
//! named by a digest of its URL, beside a record of what the server said of
//! it (`ETag`, `Last-Modified`) and of its last opening. Each opening asks
//! the server whether the file changed (`If-None-Match`,
//! `If-Modified-Since`): unchanged (304), the copy is opened again; changed,
//! it is downloaded again under a new name, so that a session still open on
//! the former copy keeps reading it. A copy is always read-only. The copies
//! no session opened for a while are deleted by `purge`
//! (`docs/TODO-SQLITE.md`, decision 15).

use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use futures_util::future::BoxFuture;
use reqwest::header::{
    CONTENT_LENGTH, ETAG, HeaderMap, IF_MODIFIED_SINCE, IF_NONE_MATCH, LAST_MODIFIED,
};
use reqwest::{Client, StatusCode, redirect};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

use super::{
    Local, Location, LocationError, LocationProvider, LocationReport, ProviderCapabilities, Remote,
    iso_8601,
};

/// Provider of the web files, for the registry (`locations::of`).
pub static PROVIDER: HttpProvider = HttpProvider;

/// Redirects followed at most for a request.
const MAX_REDIRECTS: usize = 5;

/// Extension of the record of a copy.
const RECORD_EXTENSION: &str = "json";

/// Extension of a copy.
const COPY_EXTENSION: &str = "db";

/// Extension of a copy being downloaded.
const PART_EXTENSION: &str = "part";

/// Provider of the files of public web servers.
pub struct HttpProvider;

/// What the cache knows of the copy of a location.
#[derive(Debug, Serialize, Deserialize)]
struct Record {
    /// URL of the location.
    url: String,
    /// Name of the copy in the cache folder.
    file: String,
    /// `ETag` the server sent with the copy, if any.
    etag: Option<String>,
    /// `Last-Modified` the server sent with the copy, if any.
    last_modified: Option<String>,
    /// Last opening of the copy, in seconds since 1970-01-01 (UTC).
    opened_at: u64,
}

impl LocationProvider for HttpProvider {
    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            writable: false,
            creatable: false,
            local: false,
        }
    }

    fn probe<'a>(
        &'a self,
        location: &'a Location,
        remote: &'a Remote,
    ) -> BoxFuture<'a, Result<LocationReport, LocationError>> {
        Box::pin(async move {
            let response = client(remote)
                .map_err(|error| failure(location, &error))?
                .head(&location.address)
                .send()
                .await
                .map_err(|error| failure(location, &error))?;
            let status = response.status();
            if matches!(status, StatusCode::NOT_FOUND | StatusCode::GONE) {
                return Ok(LocationReport {
                    exists: false,
                    size: None,
                    modified_at: None,
                    version_tag: None,
                    read_only: true,
                });
            }
            if !status.is_success() {
                return Err(status_error(location, status));
            }
            let headers = response.headers();
            Ok(LocationReport {
                exists: true,
                size: header(headers, CONTENT_LENGTH).and_then(|size| size.parse().ok()),
                modified_at: header(headers, LAST_MODIFIED)
                    .and_then(|date| httpdate::parse_http_date(&date).ok())
                    .map(iso_8601),
                version_tag: header(headers, ETAG),
                read_only: true,
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
        location: &'a Location,
        remote: &'a Remote,
    ) -> BoxFuture<'a, Result<Local, LocationError>> {
        Box::pin(async move {
            let path = fetch(location, remote).await?;
            Ok(Local::File {
                path,
                read_only: true,
            })
        })
    }
}

/// Client of the requests of a user: their timeouts, the redirects
/// followed (`MAX_REDIRECTS` at most, never from `https` to `http`).
fn client(remote: &Remote) -> reqwest::Result<Client> {
    Client::builder()
        .connect_timeout(remote.limits.connect_timeout)
        .timeout(remote.limits.download_timeout)
        .redirect(redirect::Policy::custom(|attempt| {
            match redirect_refusal(attempt.url(), attempt.previous()) {
                Some(refusal) => attempt.error(refusal),
                None => attempt.follow(),
            }
        }))
        .build()
}

/// Why the redirect to `next` is refused, after the URLs `previous` (the
/// one first requested included): `None` if it is followed.
fn redirect_refusal(next: &url::Url, previous: &[url::Url]) -> Option<String> {
    if next.scheme() == "http" && previous.iter().any(|url| url.scheme() == "https") {
        Some("redirect from https to http refused".to_string())
    } else if previous.len() > MAX_REDIRECTS {
        Some(format!("more than {MAX_REDIRECTS} redirects"))
    } else {
        None
    }
}

/// Path of the copy of `location` in the cache, checked with the server, and
/// downloaded again if it changed.
async fn fetch(location: &Location, remote: &Remote) -> Result<PathBuf, LocationError> {
    let cache = &remote.cache_dir;
    tokio::fs::create_dir_all(cache)
        .await
        .map_err(|error| failure(location, &error))?;
    let key = key_of(&location.address);
    let record_path = cache.join(format!("{key}.{RECORD_EXTENSION}"));
    let mut known = read_record(&record_path)
        .await
        .filter(|record| record.url == location.address);
    if let Some(record) = &known
        && !tokio::fs::try_exists(cache.join(&record.file))
            .await
            .unwrap_or(false)
    {
        known = None;
    }

    let mut request = client(remote)
        .map_err(|error| failure(location, &error))?
        .get(&location.address);
    if let Some(record) = &known {
        if let Some(etag) = &record.etag {
            request = request.header(IF_NONE_MATCH, etag);
        }
        if let Some(date) = &record.last_modified {
            request = request.header(IF_MODIFIED_SINCE, date);
        }
    }
    let mut response = request
        .send()
        .await
        .map_err(|error| failure(location, &error))?;
    let status = response.status();

    if let Some(mut record) = known
        && status == StatusCode::NOT_MODIFIED
    {
        record.opened_at = now();
        write_record(&record_path, &record, location).await?;
        return Ok(cache.join(&record.file));
    }
    if !status.is_success() {
        return Err(status_error(location, status));
    }
    let max = remote.limits.max_bytes;
    let too_large = || LocationError::TooLarge { max_bytes: max };
    if response.content_length().is_some_and(|length| length > max) {
        return Err(too_large());
    }
    let headers = response.headers().clone();
    let file = format!("{key}-{}.{COPY_EXTENSION}", now_millis());
    let part = cache.join(format!("{file}.{PART_EXTENSION}"));
    let mut output = tokio::fs::File::create(&part)
        .await
        .map_err(|error| failure(location, &error))?;
    let mut written: u64 = 0;
    let copied: Result<(), LocationError> = async {
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|error| failure(location, &error))?
        {
            written += chunk.len() as u64;
            if written > max {
                return Err(too_large());
            }
            output
                .write_all(&chunk)
                .await
                .map_err(|error| failure(location, &error))?;
        }
        output
            .flush()
            .await
            .map_err(|error| failure(location, &error))
    }
    .await;
    drop(output);
    if let Err(error) = copied {
        let _ = tokio::fs::remove_file(&part).await;
        return Err(error);
    }
    tokio::fs::rename(&part, cache.join(&file))
        .await
        .map_err(|error| failure(location, &error))?;

    let former = read_record(&record_path).await.map(|record| record.file);
    let record = Record {
        url: location.address.clone(),
        file: file.clone(),
        etag: header(&headers, ETAG),
        last_modified: header(&headers, LAST_MODIFIED),
        opened_at: now(),
    };
    write_record(&record_path, &record, location).await?;
    if let Some(former) = former.filter(|former| *former != file) {
        // A session may still read the former copy: `purge` takes it later.
        let _ = tokio::fs::remove_file(cache.join(former)).await;
    }
    Ok(cache.join(file))
}

/// Deletes from the cache folder `cache_dir` the copies no session opened
/// for `unused_for`, the copies no record names any more (replaced, or left
/// by a download cut), and returns the number of files deleted. A copy still
/// open is left for the next purge.
pub async fn purge(cache_dir: &Path, unused_for: Duration) -> std::io::Result<usize> {
    let mut entries = match tokio::fs::read_dir(cache_dir).await {
        Ok(entries) => entries,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(error),
    };
    let oldest = now().saturating_sub(unused_for.as_secs());
    let mut records = Vec::new();
    let mut copies = Vec::new();
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        match path.extension().and_then(|extension| extension.to_str()) {
            Some(RECORD_EXTENSION) => records.push(path),
            Some(COPY_EXTENSION | PART_EXTENSION) => copies.push(path),
            _ => {}
        }
    }
    let mut kept = Vec::new();
    let mut deleted = 0;
    for path in records {
        match read_record(&path).await {
            Some(record) if record.opened_at >= oldest => kept.push(record.file),
            _ => {
                if tokio::fs::remove_file(&path).await.is_ok() {
                    deleted += 1;
                }
            }
        }
    }
    for path in copies {
        let name = path.file_name().and_then(|name| name.to_str());
        if name.is_some_and(|name| kept.iter().any(|file| file == name)) {
            continue;
        }
        if tokio::fs::remove_file(&path).await.is_ok() {
            deleted += 1;
        }
    }
    Ok(deleted)
}

/// Name of the copies of the URL `url` in the cache: its SHA-256 digest in
/// hexadecimal, stable from one version of Prisme to the next.
fn key_of(url: &str) -> String {
    Sha256::digest(url.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Record at `path`, `None` if missing or unreadable.
async fn read_record(path: &Path) -> Option<Record> {
    let text = tokio::fs::read(path).await.ok()?;
    serde_json::from_slice(&text).ok()
}

/// Writes the record of a copy.
async fn write_record(
    path: &Path,
    record: &Record,
    location: &Location,
) -> Result<(), LocationError> {
    let text = serde_json::to_vec(record).map_err(|error| failure(location, &error))?;
    tokio::fs::write(path, text)
        .await
        .map_err(|error| failure(location, &error))
}

/// Text of the header `name`, if present and readable.
fn header(headers: &HeaderMap, name: reqwest::header::HeaderName) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
}

/// Error of a location the server answered with `status`.
fn status_error(location: &Location, status: StatusCode) -> LocationError {
    LocationError::Unreachable {
        address: location.address.clone(),
        message: format!("HTTP {status}"),
    }
}

/// Error of a location that could not be reached, with its cause.
fn failure(location: &Location, error: &dyn std::error::Error) -> LocationError {
    let mut message = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        message = format!("{message}: {cause}");
        source = cause.source();
    }
    LocationError::Unreachable {
        address: location.address.clone(),
        message,
    }
}

/// Seconds since 1970-01-01 (UTC).
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Milliseconds since 1970-01-01 (UTC), which name a new copy.
fn now_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use axum::Router;
    use axum::extract::{Path as UrlPath, State};
    use axum::http::{HeaderMap as Headers, HeaderValue, StatusCode as Status, header};
    use axum::response::{IntoResponse, Redirect, Response};
    use axum::routing::get;

    use super::*;
    use crate::location::RemoteLimits;

    /// Content of the file served, by version.
    fn content(version: usize) -> Vec<u8> {
        format!("SQLite format 3\0version {version}").into_bytes()
    }

    /// State of the test server: version of the file, downloads served.
    #[derive(Default)]
    struct Served {
        version: AtomicUsize,
        downloads: AtomicUsize,
    }

    /// The file `/v.db`, with its `ETag`; 304 when the tag sent is its own.
    async fn file(State(served): State<Arc<Served>>, headers: Headers) -> Response {
        let version = served.version.load(Ordering::SeqCst);
        let etag = format!("\"v{version}\"");
        if headers
            .get(header::IF_NONE_MATCH)
            .is_some_and(|tag| tag.as_bytes() == etag.as_bytes())
        {
            return Status::NOT_MODIFIED.into_response();
        }
        served.downloads.fetch_add(1, Ordering::SeqCst);
        let mut response = content(version).into_response();
        let headers = response.headers_mut();
        headers.insert(header::ETAG, HeaderValue::from_str(&etag).unwrap());
        headers.insert(
            header::LAST_MODIFIED,
            HeaderValue::from_static("Sun, 22 Nov 2026 14:54:42 GMT"),
        );
        response
    }

    /// `/hop/n`: `n` more redirects before the file.
    async fn hop(UrlPath(hops): UrlPath<usize>) -> Redirect {
        if hops == 0 {
            Redirect::temporary("/v.db")
        } else {
            Redirect::temporary(&format!("/hop/{}", hops - 1))
        }
    }

    /// Local web server of a test, its address and its state.
    async fn server() -> (String, Arc<Served>) {
        let served = Arc::new(Served::default());
        let app = Router::new()
            .route("/v.db", get(file))
            .route("/hop/{hops}", get(hop))
            .route("/big.db", get(|| async { vec![0_u8; 4_096] }))
            .with_state(served.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (format!("http://{address}"), served)
    }

    fn location(url: &str) -> Location {
        Location::parse(url).unwrap()
    }

    fn remote(cache: &Path) -> Remote {
        Remote {
            cache_dir: cache.to_path_buf(),
            limits: RemoteLimits::default(),
        }
    }

    /// Path of the copy of `location`, read-only.
    async fn copy(location: &Location, remote: &Remote) -> PathBuf {
        match PROVIDER.materialize(location, remote).await.unwrap() {
            Local::File {
                path,
                read_only: true,
            } => path,
            other => panic!("{other:?}"),
        }
    }

    #[tokio::test]
    async fn downloads_once_then_again_only_when_the_file_changed() {
        let (base, served) = server().await;
        let cache = tempfile::tempdir().unwrap();
        let remote = remote(cache.path());
        let file = location(&format!("{base}/v.db"));

        let first = copy(&file, &remote).await;
        assert_eq!(std::fs::read(&first).unwrap(), content(0));
        assert_eq!(copy(&file, &remote).await, first);
        assert_eq!(served.downloads.load(Ordering::SeqCst), 1);

        served.version.store(1, Ordering::SeqCst);
        let changed = copy(&file, &remote).await;
        assert_ne!(changed, first);
        assert_eq!(std::fs::read(&changed).unwrap(), content(1));
        assert!(!first.exists(), "the former copy is deleted");
        assert_eq!(served.downloads.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn follows_five_redirects_and_no_more() {
        let (base, _) = server().await;
        let cache = tempfile::tempdir().unwrap();
        let remote = remote(cache.path());
        // `/hop/4` makes 5 redirects to the file, `/hop/5` makes 6.
        let path = copy(&location(&format!("{base}/hop/4")), &remote).await;
        assert_eq!(std::fs::read(path).unwrap(), content(0));
        let refused = PROVIDER
            .materialize(&location(&format!("{base}/hop/5")), &remote)
            .await;
        match refused {
            Err(LocationError::Unreachable { message, .. }) => {
                assert!(message.contains("more than 5 redirects"), "{message}");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn never_follows_a_redirect_from_https_to_http() {
        let url = |text: &str| url::Url::parse(text).unwrap();
        let secure = [url("https://example.org/v.db")];
        assert!(redirect_refusal(&url("http://example.org/v.db"), &secure).is_some());
        assert!(redirect_refusal(&url("https://mirror.example.org/v.db"), &secure).is_none());
        let plain = [url("http://example.org/")];
        assert!(redirect_refusal(&url("https://example.org/v.db"), &plain).is_none());
    }

    #[tokio::test]
    async fn refuses_a_missing_file_and_one_too_large() {
        let (base, _) = server().await;
        let cache = tempfile::tempdir().unwrap();
        let mut remote = remote(cache.path());
        match PROVIDER
            .materialize(&location(&format!("{base}/missing.db")), &remote)
            .await
        {
            Err(LocationError::Unreachable { message, .. }) => {
                assert!(message.contains("404"), "{message}");
            }
            other => panic!("{other:?}"),
        }
        remote.limits.max_bytes = 1_000;
        assert_eq!(
            PROVIDER
                .materialize(&location(&format!("{base}/big.db")), &remote)
                .await,
            Err(LocationError::TooLarge { max_bytes: 1_000 })
        );
        let left = std::fs::read_dir(cache.path()).unwrap().count();
        assert_eq!(left, 0, "nothing is left of a refused download");
    }

    #[tokio::test]
    async fn probes_a_file_without_keeping_it() {
        let (base, _) = server().await;
        let cache = tempfile::tempdir().unwrap();
        let remote = remote(cache.path());
        let report = PROVIDER
            .probe(&location(&format!("{base}/v.db")), &remote)
            .await
            .unwrap();
        assert!(report.exists && report.read_only);
        assert_eq!(report.size, Some(content(0).len() as u64));
        assert_eq!(report.version_tag.as_deref(), Some("\"v0\""));
        assert_eq!(
            report.modified_at.as_deref(),
            Some("2026-11-22T14:54:42.000Z")
        );
        let missing = PROVIDER
            .probe(&location(&format!("{base}/missing.db")), &remote)
            .await
            .unwrap();
        assert!(!missing.exists);
        assert!(matches!(
            PROVIDER.create(&location(&format!("{base}/new.db"))).await,
            Err(LocationError::NotCreatable { .. })
        ));
        assert_eq!(std::fs::read_dir(cache.path()).unwrap().count(), 0);
    }

    #[tokio::test]
    async fn purges_the_copies_unused_and_those_no_record_names() {
        let (base, _) = server().await;
        let cache = tempfile::tempdir().unwrap();
        let remote = remote(cache.path());
        let url = format!("{base}/v.db");
        let kept = copy(&location(&url), &remote).await;
        let stray = cache.path().join("0000-1.db");
        std::fs::write(&stray, b"former copy").unwrap();
        let part = cache.path().join("0000-2.db.part");
        std::fs::write(&part, b"download cut").unwrap();

        let hour = Duration::from_secs(3_600);
        assert_eq!(purge(cache.path(), hour).await.unwrap(), 2);
        assert!(kept.exists() && !stray.exists() && !part.exists());

        // Unused for longer than the delay: the copy and its record go.
        let record_path = cache.path().join(format!("{}.json", key_of(&url)));
        let mut record: Record =
            serde_json::from_slice(&std::fs::read(&record_path).unwrap()).unwrap();
        record.opened_at -= 7_200;
        std::fs::write(&record_path, serde_json::to_vec(&record).unwrap()).unwrap();
        assert_eq!(purge(cache.path(), hour).await.unwrap(), 2);
        assert!(!kept.exists() && !record_path.exists());
        let missing = cache.path().join("none");
        assert_eq!(purge(&missing, Duration::ZERO).await.unwrap(), 0);
    }
}
