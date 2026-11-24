//! Console of the interface: the history of the queries sent to the target
//! servers (`history.rs`) and the debug log (`debug.rs`), kept in the
//! internal database and streamed to the interface as they grow.
//!
//! `tracing` gathers what the console shows: two layers (`layer`,
//! `debug_layer`), added to the log at startup, hand each traced element to
//! the console's writer through a channel. The writer (`write`) saves the
//! elements by batches, then announces them to the subscribed interfaces
//! (`ConsoleEvent`). It never writes to the log itself: a failure to save
//! would otherwise feed the log it saves.
//!
//! The elements older than the retention set in the profile are purged at
//! startup, then every hour (`purge_regularly`). The user clears either list
//! from the console: their own history, or the whole debug log, or the
//! elements of an editor only.
//!
//! Each element knows the editor whose session wrote it (`editor_id`, 0
//! outside an editor): the span of the history of an editor's session
//! (`editor_span`) carries it, and both layers read it, so that the console
//! of an editor shows its own elements.
//!
//! | Route                             | Role                                                    |
//! |-----------------------------------|---------------------------------------------------------|
//! | `GET /api/console/history`        | Page of the history: `before` (id), `limit` (500 by default), `action` (an action kept, repeated: `action=run&action=run_all`; all when absent), `editor` (the queries of an editor; all when absent). |
//! | `DELETE /api/console/history`     | Clears the user's history, that of an editor with `editor` (204). |
//! | `GET /api/console/debug`          | Page of the debug log: `before`, `limit`, `level` and `source` (a level or a source kept, repeated: `level=error&level=warn`; all when absent), `editor`. |
//! | `DELETE /api/console/debug`       | Clears the debug log, the lines of an editor with `editor` (204). |
//! | `POST /api/console/notifications` | Notification shown by the interface, for the debug log (204). |
//! | `GET /api/console/stream`         | Server-sent events: `history` for each new query, `debug` for each new line, `history-cleared` / `debug-cleared` when a list is cleared (`ConsoleCleared`), `lagged` when events were missed. |

mod debug;
mod history;

pub use debug::{DebugEntry, DebugLevel};
pub use history::{Action, QueryEntry, action_span, connection_span, editor_span, sql_span};

use std::convert::Infallible;
use std::time::Duration;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::{get, post};
use axum::{Json, Router};
use axum_extra::extract::Query;
use futures_util::Stream;
use futures_util::stream;
use prisme_connectors::sql_log::TARGET;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::mpsc;
use tracing::{Level, Subscriber};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::Layer;
use tracing_subscriber::filter::{FilterExt, Targets};
use tracing_subscriber::registry::LookupSpan;
use ts_rs::TS;

use crate::api::AppState;
use crate::api_error::ApiError;
use crate::auth::CurrentUser;
use debug::{DebugFilter, DebugLayer, DebugSource, TracedLine};
use history::{HistoryFilter, HistoryLayer, TracedQuery};

/// Number of console events kept for the slowest subscriber: beyond, it
/// receives `lagged` and reads the history again.
pub const EVENTS_CAPACITY: usize = 1024;

/// Most elements saved in one transaction.
const BATCH: usize = 500;

/// Interval between two purges of the elements past their retention.
const PURGE_INTERVAL: Duration = Duration::from_secs(3600);

/// Size of a page of the history, by default and at most.
const PAGE: u32 = 500;
const MAX_PAGE: u32 = 2000;

/// Element traced for the console, before it is saved.
#[derive(Debug)]
pub enum Traced {
    /// Query sent to a target server.
    Query(TracedQuery),
    /// Line of the debug log.
    Line(TracedLine),
}

/// Element saved, announced to the interfaces.
#[derive(Debug, Clone)]
pub enum ConsoleEvent {
    /// New query in the history of user `user_id`.
    Query {
        /// Owner of the history.
        user_id: i64,
        /// The query.
        entry: QueryEntry,
    },
    /// New line in the debug log, seen by all.
    Line(DebugEntry),
    /// History of user `user_id` cleared.
    HistoryCleared {
        /// Owner of the history.
        user_id: i64,
        /// Editor whose queries were cleared; all of them when `None`.
        editor_id: Option<i64>,
    },
    /// Debug log cleared, for all.
    DebugCleared {
        /// Editor whose lines were cleared; all of them when `None`.
        editor_id: Option<i64>,
    },
}

/// Elements cleared from a list of the console, as the interface receives
/// them (`history-cleared`, `debug-cleared`).
#[derive(Debug, Clone, Copy, Serialize, TS)]
#[ts(export)]
pub struct ConsoleCleared {
    /// Editor whose elements were cleared; all of them when `None`.
    pub editor_id: Option<i64>,
}

impl ConsoleEvent {
    /// Server-sent event for user `user_id`, `None` if the element is not theirs.
    fn to_sse(&self, user_id: i64) -> Option<Event> {
        match self {
            Self::Query {
                user_id: owner,
                entry,
            } if *owner == user_id => Event::default().event("history").json_data(entry).ok(),
            Self::Query { .. } => None,
            Self::Line(entry) => Event::default().event("debug").json_data(entry).ok(),
            Self::HistoryCleared {
                user_id: owner,
                editor_id,
            } if *owner == user_id => Event::default()
                .event("history-cleared")
                .json_data(ConsoleCleared {
                    editor_id: *editor_id,
                })
                .ok(),
            Self::HistoryCleared { .. } => None,
            Self::DebugCleared { editor_id } => Event::default()
                .event("debug-cleared")
                .json_data(ConsoleCleared {
                    editor_id: *editor_id,
                })
                .ok(),
        }
    }
}

/// Errors of the console.
#[derive(Debug, thiserror::Error)]
pub enum ConsoleError {
    /// Error of the internal database.
    #[error("internal database: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

impl From<ConsoleError> for ApiError {
    /// Translates a console error into an HTTP error.
    fn from(error: ConsoleError) -> Self {
        tracing::error!("{error}");
        Self::Internal
    }
}

/// Channel from the log to the console's writer.
pub fn channel() -> (
    mpsc::UnboundedSender<Traced>,
    mpsc::UnboundedReceiver<Traced>,
) {
    mpsc::unbounded_channel()
}

/// Layer of the log that feeds the console: the queries traced by the
/// connector, under the target `TARGET` only.
pub fn layer<S>(sender: mpsc::UnboundedSender<Traced>) -> impl Layer<S>
where
    S: Subscriber + for<'lookup> LookupSpan<'lookup>,
{
    HistoryLayer { sender }.with_filter(Targets::new().with_target(TARGET, Level::TRACE))
}

/// Layer of the log that feeds the debug log: the lines that `filter` lets
/// through, that of the terminal, and the spans of the history, which give
/// their editor to the lines written under them.
pub fn debug_layer<S>(sender: mpsc::UnboundedSender<Traced>, filter: EnvFilter) -> impl Layer<S>
where
    S: Subscriber + for<'lookup> LookupSpan<'lookup>,
{
    DebugLayer { sender }.with_filter(filter.or(Targets::new().with_target(TARGET, Level::TRACE)))
}

/// Saves the traced elements, by batches, then announces them to the
/// interfaces. Runs until the channel closes. A failure loses the batch,
/// without stopping the writer.
pub async fn write(state: AppState, mut receiver: mpsc::UnboundedReceiver<Traced>) {
    while let Some(first) = receiver.recv().await {
        let mut batch = vec![first];
        while batch.len() < BATCH
            && let Ok(next) = receiver.try_recv()
        {
            batch.push(next);
        }
        let local_user_id = state.local_user_id;
        let db = state.database();
        let saved = tokio::task::spawn_blocking(move || {
            let db = db.lock().ok()?;
            save(&db, local_user_id, &batch).ok()
        })
        .await
        .ok()
        .flatten();
        for event in saved.unwrap_or_default() {
            // Without a subscriber, the element is only saved.
            let _ = state.console.send(event);
        }
    }
}

/// Saves a batch of traced elements in one transaction. An element without
/// a user belongs to `local_user_id`.
fn save(
    db: &Connection,
    local_user_id: i64,
    batch: &[Traced],
) -> Result<Vec<ConsoleEvent>, ConsoleError> {
    let transaction = db.unchecked_transaction()?;
    let mut events = Vec::with_capacity(batch.len());
    for traced in batch {
        match traced {
            Traced::Query(query) => events.push(ConsoleEvent::Query {
                user_id: query.user_id.unwrap_or(local_user_id),
                entry: history::insert(&transaction, local_user_id, query)?,
            }),
            Traced::Line(line) => {
                events.push(ConsoleEvent::Line(debug::insert(&transaction, line)?));
            }
        }
    }
    transaction.commit()?;
    Ok(events)
}

/// Deletes the elements past their retention: the queries of each user
/// older than their `history_retention_days`, the lines of the debug log
/// older than the `debug_retention_days` of `local_user_id`. Returns the
/// number of elements deleted.
pub fn purge(db: &Connection, local_user_id: i64) -> Result<usize, ConsoleError> {
    let queries = db.execute(
        "DELETE FROM query_history
         WHERE started_at < (
             SELECT strftime('%Y-%m-%dT%H:%M:%fZ', 'now', '-' || history_retention_days || ' days')
             FROM users WHERE users.id = query_history.user_id
         )",
        [],
    )?;
    let lines = db.execute(
        "DELETE FROM debug_log
         WHERE logged_at < (
             SELECT strftime('%Y-%m-%dT%H:%M:%fZ', 'now', '-' || debug_retention_days || ' days')
             FROM users WHERE id = ?1
         )",
        [local_user_id],
    )?;
    Ok(queries + lines)
}

/// Purges the elements past their retention now, then every `PURGE_INTERVAL`.
pub async fn purge_regularly(state: AppState) {
    let mut interval = tokio::time::interval(PURGE_INTERVAL);
    loop {
        interval.tick().await;
        let local_user_id = state.local_user_id;
        match state.with_db(move |db, _| purge(db, local_user_id)).await {
            Ok(0) | Err(_) => {}
            Ok(deleted) => {
                tracing::debug!("console: {deleted} elements past their retention purged")
            }
        }
    }
}

/// Routes of the console, to merge into the API router.
pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/console/history",
            get(list_history).delete(clear_history),
        )
        .route("/api/console/debug", get(list_debug).delete(clear_debug))
        .route("/api/console/notifications", post(notify))
        .route("/api/console/stream", get(stream_events))
}

/// Page requested: the elements before `before` (the most recent when
/// absent), `limit` at most.
#[derive(Debug, Deserialize)]
struct Page {
    /// Id of the oldest element already read.
    before: Option<i64>,
    /// Number of elements, `PAGE` by default, `MAX_PAGE` at most.
    limit: Option<u32>,
}

/// Editor whose elements a request clears, read from the query string
/// (`editor=3`); all of them when absent.
#[derive(Debug, Default, Deserialize)]
struct Scope {
    /// Id of the editor.
    editor: Option<i64>,
}

/// Values kept by a filter, as a JSON array (`["run","run_all"]`) that the
/// queries read with `json_each`.
fn json_array<T: Serialize>(values: &[T]) -> Result<String, rusqlite::Error> {
    serde_json::to_string(values)
        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))
}

/// `GET /api/console/history`: page of the user's history, in chronological order.
async fn list_history(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(page): Query<Page>,
    Query(filter): Query<HistoryFilter>,
) -> Result<Json<Vec<QueryEntry>>, ApiError> {
    let limit = page.limit.unwrap_or(PAGE).min(MAX_PAGE);
    let entries = state
        .with_db(move |db, _| {
            history::list(db, user.id, &filter, page.before, limit).map_err(ConsoleError::from)
        })
        .await?;
    Ok(Json(entries))
}

/// `DELETE /api/console/history`: clears the user's history, that of an
/// editor with `editor` (204).
async fn clear_history(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(scope): Query<Scope>,
) -> Result<StatusCode, ApiError> {
    let editor = scope.editor;
    state
        .with_db(move |db, _| history::clear(db, user.id, editor).map_err(ConsoleError::from))
        .await?;
    // Without a subscriber, the history is only cleared.
    let _ = state.console.send(ConsoleEvent::HistoryCleared {
        user_id: user.id,
        editor_id: editor,
    });
    Ok(StatusCode::NO_CONTENT)
}

/// `DELETE /api/console/debug`: clears the debug log, the lines of an editor
/// with `editor` (204).
async fn clear_debug(
    State(state): State<AppState>,
    _user: CurrentUser,
    Query(scope): Query<Scope>,
) -> Result<StatusCode, ApiError> {
    let editor = scope.editor;
    state
        .with_db(move |db, _| debug::clear(db, editor).map_err(ConsoleError::from))
        .await?;
    let _ = state
        .console
        .send(ConsoleEvent::DebugCleared { editor_id: editor });
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /api/console/debug`: page of the debug log, in chronological order.
async fn list_debug(
    State(state): State<AppState>,
    _user: CurrentUser,
    Query(page): Query<Page>,
    Query(filter): Query<DebugFilter>,
) -> Result<Json<Vec<DebugEntry>>, ApiError> {
    let limit = page.limit.unwrap_or(PAGE).min(MAX_PAGE);
    let entries = state
        .with_db(move |db, _| {
            debug::list(db, &filter, page.before, limit).map_err(ConsoleError::from)
        })
        .await?;
    Ok(Json(entries))
}

/// Notification shown by the interface.
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct Notification {
    /// Level: `error` for a failure, `info` otherwise.
    pub level: DebugLevel,
    /// Text of the notification, cut beyond 2,000 characters.
    pub message: String,
    /// Editor on screen when it showed, whose console shows it; absent,
    /// outside the editors.
    #[serde(default)]
    #[ts(optional)]
    pub editor_id: Option<i64>,
}

/// `POST /api/console/notifications`: adds a notification of the interface
/// to the debug log (204).
async fn notify(
    State(state): State<AppState>,
    _user: CurrentUser,
    Json(notification): Json<Notification>,
) -> Result<StatusCode, ApiError> {
    let mut line = TracedLine::now(
        notification.level,
        DebugSource::Interface,
        "",
        &notification.message,
    );
    line.editor_id = notification.editor_id.unwrap_or(0);
    let entry = state
        .with_db(move |db, _| debug::insert(db, &line).map_err(ConsoleError::from))
        .await?;
    // Without a subscriber, the line is only saved.
    let _ = state.console.send(ConsoleEvent::Line(entry));
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /api/console/stream`: server-sent events, one per element saved for
/// the user. A subscriber that missed elements receives `lagged`, and reads
/// the pages again.
async fn stream_events(
    State(state): State<AppState>,
    user: CurrentUser,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let user_id = user.id;
    let events = stream::unfold(state.console.subscribe(), move |mut receiver| async move {
        loop {
            match receiver.recv().await {
                Ok(event) => {
                    if let Some(sse) = event.to_sse(user_id) {
                        return Some((Ok(sse), receiver));
                    }
                }
                Err(RecvError::Lagged(_)) => {
                    return Some((Ok(Event::default().event("lagged").data("")), receiver));
                }
                Err(RecvError::Closed) => return None,
            }
        }
    });
    Sse::new(events).keep_alive(KeepAlive::default())
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use prisme_connectors::sql_log::{self, Sent};
    use tracing_subscriber::layer::SubscriberExt;

    use super::*;
    use crate::store;

    #[test]
    fn clears_the_history_of_a_user_and_the_debug_log() {
        let db = store::open_in_memory();
        let ada = store::ensure_user(&db, "a@example.com").unwrap();
        let bob = store::ensure_user(&db, "b@example.com").unwrap();
        for user_id in [ada, bob] {
            db.execute(
                "INSERT INTO query_history (user_id, started_at, action, sql)
                 VALUES (?1, '2026-10-20T12:00:00.000Z', 'run', 'SELECT 1')",
                [user_id],
            )
            .unwrap();
        }
        debug::insert(
            &db,
            &TracedLine::now(DebugLevel::Info, DebugSource::Server, "", "line"),
        )
        .unwrap();

        assert_eq!(history::clear(&db, ada, None).unwrap(), 1);
        assert!(
            history::list(&db, ada, &HistoryFilter::default(), None, 10)
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            history::list(&db, bob, &HistoryFilter::default(), None, 10)
                .unwrap()
                .len(),
            1
        );
        assert_eq!(debug::clear(&db, None).unwrap(), 1);
        assert!(
            debug::list(&db, &DebugFilter::default(), None, 10)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn reads_the_filters_from_the_query_string() {
        let read = |uri: &str| uri.parse::<axum::http::Uri>().unwrap();
        let Query(history) =
            Query::<HistoryFilter>::try_from_uri(&read("/h?before=9&action=run&action=run_all"))
                .unwrap();
        assert_eq!(history.actions, [Action::Run, Action::RunAll]);
        assert_eq!(history.editor, None);
        let Query(debug) =
            Query::<DebugFilter>::try_from_uri(&read("/d?level=error&source=interface&editor=3"))
                .unwrap();
        assert_eq!(debug.levels, [DebugLevel::Error]);
        assert_eq!(debug.sources, [DebugSource::Interface]);
        assert_eq!(debug.editor, Some(3));
        let Query(all) = Query::<DebugFilter>::try_from_uri(&read("/d?before=9")).unwrap();
        assert!(all.levels.is_empty() && all.sources.is_empty() && all.editor.is_none());
        // An unknown name refuses the request.
        assert!(Query::<HistoryFilter>::try_from_uri(&read("/h?action=explorer")).is_err());
    }

    #[test]
    fn announces_a_cleared_history_to_its_owner_only() {
        let cleared = ConsoleEvent::HistoryCleared {
            user_id: 1,
            editor_id: Some(3),
        };
        assert!(cleared.to_sse(1).is_some());
        assert!(cleared.to_sse(2).is_none());
        assert!(
            ConsoleEvent::DebugCleared { editor_id: None }
                .to_sse(2)
                .is_some()
        );
    }

    #[test]
    fn purges_the_elements_past_their_retention() {
        let db = store::open_in_memory();
        let user_id = store::ensure_user(&db, "a@example.com").unwrap();
        db.execute(
            "UPDATE users SET history_retention_days = 2, debug_retention_days = 5",
            [],
        )
        .unwrap();
        for (sql, days) in [("old", 3), ("recent", 1)] {
            db.execute(
                "INSERT INTO query_history (user_id, started_at, action, sql)
                 VALUES (?1, strftime('%Y-%m-%dT%H:%M:%fZ', 'now', ?2), 'run', ?3)",
                rusqlite::params![user_id, format!("-{days} days"), sql],
            )
            .unwrap();
            db.execute(
                "INSERT INTO debug_log (logged_at, level, source, message)
                 VALUES (strftime('%Y-%m-%dT%H:%M:%fZ', 'now', ?1), 'info', 'server', ?2)",
                rusqlite::params![format!("-{} days", days * 2), sql],
            )
            .unwrap();
        }

        // History: 3 days > 2, deleted; debug log: 6 days > 5, deleted.
        assert_eq!(purge(&db, user_id).unwrap(), 2);
        let history = history::list(&db, user_id, &HistoryFilter::default(), None, 10).unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].sql, "recent");
        let lines = debug::list(&db, &DebugFilter::default(), None, 10).unwrap();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].message, "recent");
    }

    #[test]
    fn saves_the_queries_traced_in_their_context() {
        let db = store::open_in_memory();
        let local = store::ensure_user(&db, "a@example.com").unwrap();
        let owner = store::ensure_user(&db, "b@example.com").unwrap();
        let (sender, mut receiver) = channel();
        let subscriber = tracing_subscriber::registry()
            .with(layer(sender.clone()))
            .with(debug_layer(sender, EnvFilter::new("info")));
        tracing::subscriber::with_default(subscriber, || {
            let _connection = connection_span(owner, "Local").entered();
            sql_log::record(&Sent {
                sql: "SELECT VERSION()",
                ..Sent::default()
            });
            let _run = action_span(Action::Run).entered();
            sql_log::record(&Sent {
                sql: "SELECT * FROM t",
                duration: Duration::from_micros(1500),
                transfer: Duration::from_micros(300),
                rows: Some(2),
                truncated: true,
                error: Some("lost"),
                ..Sent::default()
            });
            // Out of the target of the history: in the debug log only, with its fields.
            tracing::info!(port = 8484, "listening");
            // Under the level of the debug log: ignored.
            tracing::debug!("detail");
            drop(_run);
            // The session of an editor gives it its queries and its lines.
            let _editor = editor_span(owner, "Local", 7).entered();
            sql_log::record(&Sent {
                sql: "SELECT 7",
                ..Sent::default()
            });
            tracing::warn!("slow");
        });

        let mut batch = Vec::new();
        while let Ok(traced) = receiver.try_recv() {
            batch.push(traced);
        }
        assert_eq!(batch.len(), 5);
        let events = save(&db, local, &batch).unwrap();
        let entries: Vec<&QueryEntry> = events
            .iter()
            .filter_map(|event| match event {
                ConsoleEvent::Query { entry, .. } => Some(entry),
                _ => None,
            })
            .collect();
        // Out of an action, a query counts as one of the session.
        assert_eq!(entries[0].action, Action::Session);
        assert_eq!(entries[0].connection_name, "Local");
        assert!(!entries[0].returns_rows);
        let run = entries[1];
        assert_eq!(run.action, Action::Run);
        assert_eq!(run.sql, "SELECT * FROM t");
        assert_eq!((run.duration_us, run.transfer_us), (1500, 300));
        assert!(run.returns_rows && run.truncated && run.row_count == 2);
        assert_eq!(run.error, "lost");
        assert_eq!((run.editor_id, entries[2].editor_id), (0, 7));
        assert_eq!(entries[2].sql, "SELECT 7");
        // The user of the span owns the queries, not the local user.
        assert!(matches!(events[0], ConsoleEvent::Query { user_id, .. } if user_id == owner));
        assert_eq!(
            history::list(&db, owner, &HistoryFilter::default(), None, 10)
                .unwrap()
                .len(),
            3
        );
        let lines = debug::list(&db, &DebugFilter::default(), None, 10).unwrap();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].message, "listening port=8484");
        assert_eq!(lines[0].level, DebugLevel::Info);
        assert_eq!((lines[0].editor_id, lines[1].editor_id), (0, 7));
        assert_eq!(lines[1].message, "slow");
    }

    #[test]
    fn gives_the_queries_out_of_a_session_to_their_editor() {
        let (sender, mut receiver) = channel();
        let subscriber = tracing_subscriber::registry().with(layer(sender));
        tracing::subscriber::with_default(subscriber, || {
            for editor in [Some(5), None] {
                let _navigator = sql_span(1, "Local", editor, Action::Navigator).entered();
                sql_log::record(&Sent {
                    sql: "SHOW DATABASES",
                    ..Sent::default()
                });
            }
        });

        let mut traced = Vec::new();
        while let Ok(Traced::Query(query)) = receiver.try_recv() {
            traced.push((query.editor_id, query.action));
        }
        assert_eq!(
            traced,
            [(5, Some(Action::Navigator)), (0, Some(Action::Navigator))]
        );
    }
}
