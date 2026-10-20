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
//! startup, then every hour (`purge_regularly`).
//!
//! | Route                             | Role                                                    |
//! |-----------------------------------|---------------------------------------------------------|
//! | `GET /api/console/history`        | Page of the history: `before` (id), `limit` (500 by default). |
//! | `GET /api/console/debug`          | Page of the debug log, same parameters.                 |
//! | `POST /api/console/notifications` | Notification shown by the interface, for the debug log (204). |
//! | `GET /api/console/stream`         | Server-sent events: `history` for each new query, `debug` for each new line, `lagged` when events were missed. |

mod debug;
mod history;

pub use debug::{DebugEntry, DebugLevel};
pub use history::{Action, QueryEntry, action_span, connection_span, sql_span};

use std::convert::Infallible;
use std::time::Duration;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::Stream;
use futures_util::stream;
use prisme_connectors::mysql::sql_log::TARGET;
use rusqlite::Connection;
use serde::Deserialize;
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::mpsc;
use tracing::{Level, Subscriber};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::Layer;
use tracing_subscriber::filter::Targets;
use tracing_subscriber::registry::LookupSpan;
use ts_rs::TS;

use crate::api::AppState;
use crate::api_error::ApiError;
use crate::auth::CurrentUser;
use debug::{DebugLayer, DebugSource, TracedLine};
use history::{HistoryLayer, TracedQuery};

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
/// through, that of the terminal.
pub fn debug_layer<S>(sender: mpsc::UnboundedSender<Traced>, filter: EnvFilter) -> impl Layer<S>
where
    S: Subscriber + for<'lookup> LookupSpan<'lookup>,
{
    DebugLayer { sender }.with_filter(filter)
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
        .route("/api/console/history", get(list_history))
        .route("/api/console/debug", get(list_debug))
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

/// `GET /api/console/history`: page of the user's history, in chronological order.
async fn list_history(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(page): Query<Page>,
) -> Result<Json<Vec<QueryEntry>>, ApiError> {
    let limit = page.limit.unwrap_or(PAGE).min(MAX_PAGE);
    let entries = state
        .with_db(move |db, _| {
            history::list(db, user.id, page.before, limit).map_err(ConsoleError::from)
        })
        .await?;
    Ok(Json(entries))
}

/// `GET /api/console/debug`: page of the debug log, in chronological order.
async fn list_debug(
    State(state): State<AppState>,
    _user: CurrentUser,
    Query(page): Query<Page>,
) -> Result<Json<Vec<DebugEntry>>, ApiError> {
    let limit = page.limit.unwrap_or(PAGE).min(MAX_PAGE);
    let entries = state
        .with_db(move |db, _| debug::list(db, page.before, limit).map_err(ConsoleError::from))
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
}

/// `POST /api/console/notifications`: adds a notification of the interface
/// to the debug log (204).
async fn notify(
    State(state): State<AppState>,
    _user: CurrentUser,
    Json(notification): Json<Notification>,
) -> Result<StatusCode, ApiError> {
    let line = TracedLine::now(
        notification.level,
        DebugSource::Interface,
        "",
        &notification.message,
    );
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

    use prisme_connectors::mysql::sql_log::{self, Sent};
    use tracing_subscriber::layer::SubscriberExt;

    use super::*;
    use crate::store;

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
        let history = history::list(&db, user_id, None, 10).unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].sql, "recent");
        let lines = debug::list(&db, None, 10).unwrap();
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
        });

        let mut batch = Vec::new();
        while let Ok(traced) = receiver.try_recv() {
            batch.push(traced);
        }
        assert_eq!(batch.len(), 3);
        let events = save(&db, local, &batch).unwrap();
        let entries: Vec<&QueryEntry> = events
            .iter()
            .filter_map(|event| match event {
                ConsoleEvent::Query { entry, .. } => Some(entry),
                ConsoleEvent::Line(_) => None,
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
        // The user of the span owns the queries, not the local user.
        assert!(matches!(events[0], ConsoleEvent::Query { user_id, .. } if user_id == owner));
        assert_eq!(history::list(&db, owner, None, 10).unwrap().len(), 2);
        let lines = debug::list(&db, None, 10).unwrap();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].message, "listening port=8484");
        assert_eq!(lines[0].level, DebugLevel::Info);
    }
}
