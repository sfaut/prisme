//! Exports of a full result to a CSV file for Excel in French, written by
//! the server.
//!
//! The interface downloads the rows it holds itself (`web/src/components/csv.ts`).
//! The full result of a query, without the limit of Prisme, can hold
//! millions of rows: the server runs the query again, on its own connection,
//! read-only, and writes its rows to a file of the exports folder
//! (`<data-dir>/exports/`) as they arrive (`job.rs`, `csv.rs`). The export
//! goes on without the page; the export manager of the interface follows it
//! and offers its file once ready.
//!
//! The list of the exports is kept in the internal database (`table.rs`), and
//! survives a restart: an export that was running when Prisme stopped becomes
//! interrupted, with the rows of its partial file: those of its progress
//! saved last, and those written after it, counted in the file. A file is
//! only deleted by removing its export from the manager, never
//! automatically. The table and the folder are read together
//! (`folder.rs`): an export whose file is gone is listed as such, and the
//! files of Prisme that no export references are counted apart, to be removed.
//!
//! Routes:
//!
//! | Method and path                  | Role                                                         |
//! |----------------------------------|--------------------------------------------------------------|
//! | `POST /api/exports`              | Starts the export of a read (202).                           |
//! | `GET /api/exports`               | Exports of the user, the most recent first, and the folder.  |
//! | `GET /api/exports/stream`        | Server-sent events: exports started, progressing, ended, removed. |
//! | `GET /api/exports/{id}/file`     | File of the export, to save.                                 |
//! | `POST /api/exports/{id}/cancel`  | Stops a running export; its partial file stays (204).        |
//! | `DELETE /api/exports/{id}`       | Removes the export and deletes its file (204).               |
//! | `DELETE /api/exports/orphans`    | Deletes the files of Prisme that no export references (204). |

mod csv;
mod folder;
mod job;
mod table;

use std::collections::HashMap;
use std::convert::Infallible;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use axum::Json;
use axum::Router;
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::http::header::{CONTENT_DISPOSITION, CONTENT_LENGTH, CONTENT_TYPE};
use axum::response::Response;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::{delete, get, post};
use futures_util::{Stream, stream};
use prisme_connectors::mysql::{self, MysqlTarget};
use prisme_dialects::Dialect;
use prisme_dialects::classify::{StatementKind, classify};
use prisme_dialects::limit::apply_default_limit;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::{broadcast, watch};
use tokio_util::io::ReaderStream;
use tracing::Instrument;
use ts_rs::TS;

use crate::analyze::dialect_of;
use crate::api::AppState;
use crate::api_error::ApiError;
use crate::auth::CurrentUser;
use crate::connections::{self, ConnectionsError};
use crate::console::{self, Action};
use crate::error_detail::{ErrorDetail, Field};
use crate::query_session::limit_error;

use self::job::{Job, Outcome};
use self::table::NewExport;

/// Name of the exports folder in the data folder.
pub const FOLDER: &str = "exports";

/// Maximum length of the name of a file, in characters.
const NAME_MAX_LENGTH: usize = 200;

/// Events waiting for a slow subscriber, beyond which it receives `lagged`.
const EVENTS_CAPACITY: usize = 256;

/// Errors managing exports.
#[derive(Debug, thiserror::Error)]
pub enum ExportsError {
    /// Error of the internal database.
    #[error("internal database: {0}")]
    Sqlite(#[from] rusqlite::Error),

    /// Failure of the exports folder.
    #[error("exports folder: {0}")]
    Folder(#[from] std::io::Error),
}

/// Export requested by the interface.
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct ExportRequest {
    /// Connection to run the query on.
    pub connection_id: i64,
    /// Database selected on connection, in place of the connection's.
    #[serde(default)]
    #[ts(optional)]
    pub database: Option<String>,
    /// Query, a read, run as it is written: without the limit of Prisme.
    pub sql: String,
    /// Index of each exported column in the result, in the displayed order;
    /// empty: all the columns, in the order of the result.
    #[serde(default)]
    pub columns: Vec<u32>,
    /// Name of the file offered on download.
    pub name: String,
    /// Rows the result should have, when known in advance, for a percentage.
    #[serde(default)]
    #[ts(optional)]
    pub expected_rows: Option<u64>,
}

/// State of an export.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum ExportState {
    /// The rows are being written.
    Running,
    /// The file is complete.
    Ready,
    /// The query or the writing failed; the file holds the rows written before.
    Failed,
    /// Stopped by the user; the file holds the rows written before.
    Cancelled,
    /// Prisme stopped during the export; the file holds the rows written before.
    Interrupted,
}

impl ExportState {
    /// All the states.
    const ALL: [Self; 5] = [
        Self::Running,
        Self::Ready,
        Self::Failed,
        Self::Cancelled,
        Self::Interrupted,
    ];

    /// Name of the state, as stored.
    fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Ready => "ready",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Interrupted => "interrupted",
        }
    }

    /// State named `name`, `None` if unknown.
    fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|state| state.as_str() == name)
    }
}

/// Export, as the interface receives it.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ExportInfo {
    /// Id of the export.
    pub id: i64,
    /// Name of the file offered on download.
    pub name: String,
    /// Name of the connection, as it was when the export started.
    pub connection_name: String,
    /// Query exported.
    pub sql: String,
    /// State of the export.
    pub state: ExportState,
    /// Rows written.
    pub rows: u64,
    /// Bytes written; in the list of the exports, the size of the file of an
    /// ended export, interrupted ones included.
    pub bytes: u64,
    /// Rows the result should have, when known in advance.
    pub expected_rows: Option<u64>,
    /// Instant the export started, in UTC, in ISO 8601 format.
    pub created_at: String,
    /// Instant the export ended, in UTC, in ISO 8601 format; `None` while it
    /// runs. For an interrupted export, the last write to its file.
    pub ended_at: Option<String>,
    /// Failure, for a failed export.
    pub error: Option<ErrorDetail>,
    /// The file is no longer in the exports folder (deleted outside Prisme).
    pub file_missing: bool,
}

/// Files and their total size.
#[derive(Debug, Default, Serialize, TS)]
#[ts(export)]
pub struct FolderUsage {
    /// Number of files.
    pub count: u32,
    /// Total size, in bytes.
    pub bytes: u64,
}

impl FolderUsage {
    /// Counts a file of `bytes` bytes.
    fn add(&mut self, bytes: u64) {
        self.count += 1;
        self.bytes += bytes;
    }
}

/// Exports of the user, and the files of the exports folder that no export
/// references.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct ExportList {
    /// Exports, the most recent first.
    pub exports: Vec<ExportInfo>,
    /// Files of Prisme in the folder that no export references (copied
    /// there, internal database replaced...).
    pub orphans: FolderUsage,
}

/// Event of the exports stream.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ExportEvent {
    /// An export started, progressed or ended.
    Changed {
        /// The export, with its counters.
        export: Box<ExportInfo>,
    },
    /// An export was removed, its file deleted.
    Removed {
        /// Id of the export.
        id: i64,
    },
}

/// Event of the exports stream, with the user it concerns.
#[derive(Debug, Clone)]
struct Announced {
    /// User of the export.
    user_id: i64,
    /// The event.
    event: ExportEvent,
}

/// Export being written: its counters, kept up to date by its task, and its stop.
pub(crate) struct Running {
    /// The export as it started; the counters complete it.
    info: ExportInfo,
    /// User of the export.
    user_id: i64,
    /// Rows written.
    rows: AtomicU64,
    /// Bytes written.
    bytes: AtomicU64,
    /// Stop requested by the user.
    cancelled: AtomicBool,
    /// Settings of the export's connection, to close it on the server.
    target: MysqlTarget,
    /// Id (`Conn::id()`) of the export's connection.
    connection_id: u32,
    /// Becomes `true` once the export has ended and its outcome is saved.
    ended: watch::Sender<bool>,
}

impl Running {
    /// The export, with its counters.
    fn snapshot(&self) -> ExportInfo {
        ExportInfo {
            rows: self.rows.load(Ordering::Relaxed),
            bytes: self.bytes.load(Ordering::Relaxed),
            ..self.info.clone()
        }
    }

    /// Has the user stopped the export?
    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    /// Stops the export: its connection is closed on the server, and its
    /// task stops on its next row, or on the failure of its query.
    async fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        self.interrupt().await;
    }

    /// Closes the export's connection on the server (`KILL CONNECTION`),
    /// which stops its query, even when the order arrives before the query.
    async fn interrupt(&self) {
        let span = console::sql_span(self.user_id, &self.info.connection_name, Action::Cancel);
        let killing = mysql::kill_connection(&self.target, self.connection_id).instrument(span);
        if let Err(error) = killing.await {
            tracing::warn!(
                "cannot close the connection of export {}: {error}",
                self.info.id
            );
        }
    }

    /// Waits for the end of the export, its outcome saved.
    async fn ended(&self) {
        let mut ended = self.ended.subscribe();
        // The sender lives as long as `self`: the wait cannot fail.
        let _ = ended.wait_for(|ended| *ended).await;
    }
}

/// Exports of the server: the folder of their files, those being written and
/// their events.
pub struct Exports {
    /// Exports folder.
    folder: PathBuf,
    /// Running exports, by id.
    running: Mutex<HashMap<i64, Arc<Running>>>,
    /// Events, to the streams of the interfaces.
    events: broadcast::Sender<Announced>,
}

impl Exports {
    /// Exports writing their files into `folder`, created with the first export.
    pub fn new(folder: PathBuf) -> Self {
        Self {
            folder,
            running: Mutex::default(),
            events: broadcast::Sender::new(EVENTS_CAPACITY),
        }
    }

    /// Running export `id` of `user_id`.
    fn running(&self, id: i64, user_id: i64) -> Option<Arc<Running>> {
        let running = self.running.lock().unwrap_or_else(PoisonError::into_inner);
        running
            .get(&id)
            .filter(|export| export.user_id == user_id)
            .cloned()
    }

    /// Follows a running export.
    fn track(&self, export: Arc<Running>) {
        let mut running = self.running.lock().unwrap_or_else(PoisonError::into_inner);
        running.insert(export.info.id, export);
    }

    /// Stops following export `id`, which has ended.
    fn untrack(&self, id: i64) {
        let mut running = self.running.lock().unwrap_or_else(PoisonError::into_inner);
        running.remove(&id);
    }

    /// Announces an event to the interfaces of `user_id`.
    fn announce(&self, user_id: i64, event: ExportEvent) {
        // Without a subscriber, nothing to announce.
        let _ = self.events.send(Announced { user_id, event });
    }
}

/// Marks the exports that were running when Prisme stopped as interrupted,
/// at startup. Their files stay in `folder`, and give their counters: the
/// rows of the progress saved last, plus those written after it, counted in
/// the file; its size; its last write as their end. An export whose file
/// cannot be read keeps its last saved counters. Returns their number.
pub fn interrupt_unfinished(
    db: &rusqlite::Connection,
    folder: &std::path::Path,
) -> rusqlite::Result<usize> {
    let unfinished = table::unfinished(db)?;
    for export in &unfinished {
        let (rows, bytes, ended) = match folder::tail(&folder.join(&export.file), export.bytes) {
            // Before the first save, the file starts with the line of the
            // column names.
            Ok(tail) if export.bytes == 0 => {
                (tail.lines.saturating_sub(1), tail.size, Some(tail.modified))
            }
            Ok(tail) => (export.rows + tail.lines, tail.size, Some(tail.modified)),
            Err(error) => {
                if error.kind() != std::io::ErrorKind::NotFound {
                    tracing::warn!("cannot read the file of export {}: {error}", export.id);
                }
                (export.rows, export.bytes, None)
            }
        };
        let ended_millis = ended
            .and_then(|instant| instant.duration_since(UNIX_EPOCH).ok())
            .map(|since| since.as_millis());
        table::interrupt(db, export.id, rows, bytes, ended_millis)?;
    }
    Ok(unfinished.len())
}

/// Exports folder of the tests that start no export: never created.
#[cfg(test)]
pub fn unused_folder() -> PathBuf {
    std::env::temp_dir().join("prisme-tests-without-exports")
}

/// Routes of the exports, to merge into the API router.
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/exports", get(list_exports).post(start_export))
        .route("/api/exports/stream", get(stream_events))
        .route("/api/exports/orphans", delete(remove_orphans))
        .route("/api/exports/{id}", delete(remove_export))
        .route("/api/exports/{id}/file", get(download_export))
        .route("/api/exports/{id}/cancel", post(cancel_export))
}

/// Refusal of the name of a file: between 1 and `NAME_MAX_LENGTH` characters.
fn name_refusal(name: &str) -> Option<ErrorDetail> {
    let length = name.chars().count();
    (length == 0 || length > NAME_MAX_LENGTH).then(|| ErrorDetail::FieldLength {
        field: Field::Name,
        min: 1,
        max: NAME_MAX_LENGTH as u32,
        length: u32::try_from(length).unwrap_or(u32::MAX),
    })
}

/// Refusal of the query of an export: a single query, which Prisme can
/// parse and which is a read.
fn query_refusal(sql: &str, dialect: Dialect) -> Option<ErrorDetail> {
    match classify(sql, dialect) {
        StatementKind::Read => {}
        StatementKind::Unparsed => return Some(ErrorDetail::UnparsedQueryRefused),
        _ => return Some(ErrorDetail::ExportNotRead),
    }
    // Only the checks of the default limit are kept, one query at a time: the
    // export runs the query as it is written.
    apply_default_limit(sql, dialect, 0)
        .err()
        .map(|error| limit_error(&error))
}

/// Instant now, in ms since the Unix epoch.
fn now_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_millis())
}

/// `POST /api/exports`: starts the export of a read and returns it (202).
///
/// The connection to the target server opens first, read-only, on the
/// requested database: a connection failure stays an HTTP error response
/// (502, 504), and the query is checked in the dialect of the server. Once
/// the export is saved, a failure to create its file ends it as failed.
async fn start_export(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(request): Json<ExportRequest>,
) -> Result<(StatusCode, Json<ExportInfo>), ApiError> {
    if let Some(refusal) = name_refusal(&request.name) {
        return Err(ApiError::BadRequest(refusal));
    }
    let user_id = user.id;
    let connection_id = request.connection_id;
    let (target, connection_name) = state
        .with_db(move |db, key| {
            let target = connections::load_target(db, key, user_id, connection_id)?;
            let name = connections::name_of(db, user_id, connection_id)?;
            Ok::<_, ConnectionsError>(target.map(|target| (target, name)))
        })
        .await?
        .ok_or(ApiError::NotFound)?;
    // A connection of its own, read-only whatever the settings of the sessions.
    let mut target = connections::with_read_only(target, true);
    if let Some(database) = &request.database {
        target.database_name.clone_from(database);
    }

    let span = console::sql_span(user_id, &connection_name, Action::Export);
    let mut connection = mysql::connect(&target).instrument(span.clone()).await?;
    let server = match mysql::server_info(&mut connection)
        .instrument(span.clone())
        .await
    {
        Ok(server) => server,
        Err(error) => {
            let _ = connection.disconnect().await;
            return Err(error.into());
        }
    };
    if let Some(refusal) = query_refusal(&request.sql, dialect_of(server.flavor)) {
        let _ = connection.disconnect().await;
        return Err(ApiError::BadRequest(refusal));
    }

    let ExportRequest {
        sql,
        columns,
        name,
        expected_rows,
        ..
    } = request;
    let saved_sql = sql.clone();
    let saved = state
        .with_db(move |db, _| {
            let new = NewExport {
                connection_name: &connection_name,
                name: &name,
                sql: &saved_sql,
                expected_rows,
            };
            table::insert(db, user_id, &new, now_millis()).map_err(ExportsError::from)
        })
        .await;
    let (export, file) = match saved {
        Ok(saved) => saved,
        Err(error) => {
            let _ = connection.disconnect().await;
            return Err(error);
        }
    };
    let running = Arc::new(Running {
        info: export.clone(),
        user_id,
        rows: AtomicU64::new(0),
        bytes: AtomicU64::new(0),
        cancelled: AtomicBool::new(false),
        connection_id: connection.id(),
        target,
        ended: watch::Sender::new(false),
    });
    state.exports.track(Arc::clone(&running));
    state.exports.announce(
        user_id,
        ExportEvent::Changed {
            export: Box::new(export.clone()),
        },
    );

    let path = state.exports.folder.join(&file);
    let created = async {
        tokio::fs::create_dir_all(&state.exports.folder).await?;
        tokio::fs::File::create_new(&path).await
    };
    let file = match created.await {
        Ok(file) => file,
        Err(error) => {
            let _ = connection.disconnect().await;
            let detail = ErrorDetail::ExportFile {
                cause: error.to_string(),
            };
            let export = finish(&state, &running, Outcome::Failed(detail)).await;
            return Ok((
                StatusCode::ACCEPTED,
                Json(export.unwrap_or(running.info.clone())),
            ));
        }
    };
    let job = Job {
        state: state.clone(),
        running,
        connection,
        flavor: server.flavor,
        sql,
        columns: columns.into_iter().map(|index| index as usize).collect(),
        file,
    };
    tokio::spawn(job.run().instrument(span));
    Ok((StatusCode::ACCEPTED, Json(export)))
}

/// Saves the outcome of a running export, stops following it, announces it,
/// and wakes up those waiting for its end. Returns the export as saved;
/// `None` if it was removed in the meantime or could not be saved.
async fn finish(state: &AppState, running: &Running, outcome: Outcome) -> Option<ExportInfo> {
    let (export_state, error) = match outcome {
        Outcome::Ready => (ExportState::Ready, None),
        Outcome::Cancelled => (ExportState::Cancelled, None),
        Outcome::Failed(detail) => (ExportState::Failed, Some(detail)),
    };
    let id = running.info.id;
    let rows = running.rows.load(Ordering::Relaxed);
    let bytes = running.bytes.load(Ordering::Relaxed);
    let saved = state
        .with_db(move |db, _| {
            table::finish(db, id, export_state, rows, bytes, error.as_ref())
                .map_err(ExportsError::from)
        })
        .await;
    state.exports.untrack(id);
    // A failure to save is already in the log (`ApiError`).
    let export = saved.ok().flatten();
    if let Some(export) = &export {
        let event = ExportEvent::Changed {
            export: Box::new(export.clone()),
        };
        state.exports.announce(running.user_id, event);
    }
    running.ended.send_replace(true);
    export
}

/// `GET /api/exports`: exports of the current user, the most recent first,
/// with the files of the folder: their total size, the exports whose file
/// is gone, and the files that no export references.
async fn list_exports(
    State(state): State<AppState>,
    user: CurrentUser,
) -> Result<Json<ExportList>, ApiError> {
    let user_id = user.id;
    let folder = state.exports.folder.clone();
    // The table and the folder are read under the lock of the internal
    // database: no export is saved in between.
    let (saved, referenced, files) = state
        .with_db(move |db, _| {
            let saved = table::list(db, user_id)?;
            let referenced = table::files(db)?;
            let files = folder::scan(&folder)?;
            Ok::<_, ExportsError>((saved, referenced, files))
        })
        .await?;

    let mut exports = Vec::with_capacity(saved.len());
    for (export, file) in saved {
        let size = files.get(&file).copied();
        let export = match state.exports.running(export.id, user_id) {
            Some(running) => running.snapshot(),
            None => ExportInfo {
                bytes: size.unwrap_or(export.bytes),
                ..export
            },
        };
        exports.push(ExportInfo {
            file_missing: size.is_none(),
            ..export
        });
    }
    let mut orphans = FolderUsage::default();
    for (file, size) in &files {
        if !referenced.contains(file) {
            orphans.add(*size);
        }
    }
    Ok(Json(ExportList { exports, orphans }))
}

/// `GET /api/exports/stream`: server-sent events `export`, one per export of
/// the user started, progressing (every half second at most), ended or
/// removed. A subscriber that missed events receives `lagged`, and reads
/// the list again.
async fn stream_events(
    State(state): State<AppState>,
    user: CurrentUser,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let user_id = user.id;
    let receiver = state.exports.events.subscribe();
    let events = stream::unfold(receiver, move |mut receiver| async move {
        loop {
            match receiver.recv().await {
                Ok(announced) if announced.user_id == user_id => {
                    if let Ok(event) = Event::default().event("export").json_data(&announced.event)
                    {
                        return Some((Ok(event), receiver));
                    }
                }
                Ok(_) => {}
                Err(RecvError::Lagged(_)) => {
                    return Some((Ok(Event::default().event("lagged").data("")), receiver));
                }
                Err(RecvError::Closed) => return None,
            }
        }
    });
    Sse::new(events).keep_alive(KeepAlive::default())
}

/// `GET /api/exports/{id}/file`: file of the export, read from the folder as
/// it is sent, with its length (the browser shows an exact progress) and
/// the name offered to save it. 410 if the file is gone.
async fn download_export(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<i64>,
) -> Result<Response, ApiError> {
    let user_id = user.id;
    let (file, name) = state
        .with_db(move |db, _| table::file_of(db, user_id, id).map_err(ExportsError::from))
        .await?
        .ok_or(ApiError::NotFound)?;
    let opened = match tokio::fs::File::open(state.exports.folder.join(&file)).await {
        Ok(opened) => opened,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(ApiError::Gone(ErrorDetail::ExportFileMissing { file }));
        }
        Err(error) => return Err(ExportsError::Folder(error).into()),
    };
    let length = opened.metadata().await.map_err(ExportsError::Folder)?.len();
    Response::builder()
        .header(CONTENT_TYPE, "text/csv; charset=utf-8")
        .header(CONTENT_DISPOSITION, folder::content_disposition(&name))
        .header(CONTENT_LENGTH, length)
        .body(Body::from_stream(ReaderStream::new(opened)))
        .map_err(|error| {
            tracing::error!("response of export {id}: {error}");
            ApiError::Internal
        })
}

/// `POST /api/exports/{id}/cancel`: stops a running export (204); its
/// partial file stays. An export already ended stays as it is.
async fn cancel_export(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    let user_id = user.id;
    if let Some(running) = state.exports.running(id, user_id) {
        running.cancel().await;
        return Ok(StatusCode::NO_CONTENT);
    }
    state
        .with_db(move |db, _| table::file_of(db, user_id, id).map_err(ExportsError::from))
        .await?
        .map(|_| StatusCode::NO_CONTENT)
        .ok_or(ApiError::NotFound)
}

/// `DELETE /api/exports/{id}`: removes the export and deletes its file,
/// complete or partial (204). A running export is stopped first. The file
/// goes first: if it cannot be deleted (opened in another program), the
/// export stays listed (409); a file already gone does not prevent removal.
async fn remove_export(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    let user_id = user.id;
    if let Some(running) = state.exports.running(id, user_id) {
        running.cancel().await;
        running.ended().await;
    }
    let folder = state.exports.folder.clone();
    let removed = state
        .with_db(move |db, _| {
            let Some((file, _)) = table::file_of(db, user_id, id)? else {
                return Ok(false);
            };
            folder::remove(&folder, &file)?;
            Ok::<_, ExportsError>(table::delete(db, user_id, id)?)
        })
        .await?;
    if !removed {
        return Err(ApiError::NotFound);
    }
    state.exports.announce(user_id, ExportEvent::Removed { id });
    Ok(StatusCode::NO_CONTENT)
}

/// `DELETE /api/exports/orphans`: deletes the files of Prisme in the folder
/// that no export references (204); the other files of the folder stay.
/// 409 if a file cannot be deleted.
async fn remove_orphans(
    State(state): State<AppState>,
    _user: CurrentUser,
) -> Result<StatusCode, ApiError> {
    let folder = state.exports.folder.clone();
    // Under the lock of the internal database: no export is saved in between.
    state
        .with_db(move |db, _| {
            let referenced = table::files(db)?;
            folder::remove_orphans(&folder, &referenced)?;
            Ok::<_, ExportsError>(())
        })
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests;
