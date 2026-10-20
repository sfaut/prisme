//! Console of the interface: the history of the queries sent to the target
//! servers (`history.rs`), kept in the internal database and streamed to the
//! interface as it grows.
//!
//! `tracing` gathers what the console shows: a layer (`layer`), added to the
//! log at startup, hands each traced element to the console's writer
//! through a channel. The writer (`write`) saves the elements by batches,
//! then announces them to the subscribed interfaces (`ConsoleEvent`).
//!
//! | Route                          | Role                                                       |
//! |--------------------------------|------------------------------------------------------------|
//! | `GET /api/console/history`     | Page of the history: `before` (id), `limit` (500 by default). |
//! | `GET /api/console/stream`      | Server-sent events: `history` for each new query, `lagged` when events were missed. |

mod history;

pub use history::{Action, QueryEntry, action_span, connection_span, sql_span};

use std::convert::Infallible;

use axum::extract::{Query, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::get;
use axum::{Json, Router};
use futures_util::Stream;
use futures_util::stream;
use prisme_connectors::mysql::sql_log::TARGET;
use rusqlite::Connection;
use serde::Deserialize;
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::mpsc;
use tracing::{Level, Subscriber};
use tracing_subscriber::Layer;
use tracing_subscriber::filter::Targets;
use tracing_subscriber::registry::LookupSpan;

use crate::api::AppState;
use crate::api_error::ApiError;
use crate::auth::CurrentUser;
use history::{HistoryLayer, TracedQuery};

/// Number of console events kept for the slowest subscriber: beyond, it
/// receives `lagged` and reads the history again.
pub const EVENTS_CAPACITY: usize = 1024;

/// Most elements saved in one transaction.
const BATCH: usize = 500;

/// Size of a page of the history, by default and at most.
const PAGE: u32 = 500;
const MAX_PAGE: u32 = 2000;

/// Element traced for the console, before it is saved.
#[derive(Debug)]
pub enum Traced {
    /// Query sent to a target server.
    Query(TracedQuery),
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
}

impl ConsoleEvent {
    /// Server-sent event for user `user_id`, `None` if the element is not hers.
    fn to_sse(&self, user_id: i64) -> Option<Event> {
        match self {
            Self::Query {
                user_id: owner,
                entry,
            } if *owner == user_id => Event::default().event("history").json_data(entry).ok(),
            Self::Query { .. } => None,
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
        let saved = state
            .with_db(move |db, _| save(db, local_user_id, &batch))
            .await;
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
        }
    }
    transaction.commit()?;
    Ok(events)
}

/// Routes of the console, to merge into the API router.
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/console/history", get(list_history))
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
    fn saves_the_queries_traced_in_their_context() {
        let db = store::open_in_memory();
        let local = store::ensure_user(&db, "a@example.com").unwrap();
        let owner = store::ensure_user(&db, "b@example.com").unwrap();
        let (sender, mut receiver) = channel();
        let subscriber = tracing_subscriber::registry().with(layer(sender));
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
            // Out of the target of the history: ignored.
            tracing::info!("listening");
        });

        let mut batch = Vec::new();
        while let Ok(traced) = receiver.try_recv() {
            batch.push(traced);
        }
        assert_eq!(batch.len(), 2);
        let events = save(&db, local, &batch).unwrap();
        let entries: Vec<&QueryEntry> = events
            .iter()
            .map(|ConsoleEvent::Query { entry, .. }| entry)
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
    }
}
