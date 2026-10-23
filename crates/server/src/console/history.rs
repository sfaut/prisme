//! History of the queries: the SQL sent to the target servers, by the
//! editors and by Prisme itself (session opening, navigator, connection test,
//! interruptions).
//!
//! The connector traces each statement as a `tracing` event of the target
//! `sql_log::TARGET` (`prisme_connectors::mysql::sql_log`). Spans of the same
//! target give it its context: the user and the connection
//! (`connection_span`), then what sent it (`action_span`). The layer of this
//! module (`HistoryLayer`) gathers the event and its context, and hands the
//! query to the console's writer (`console.rs`), which saves it in the
//! `query_history` table (`migrations/0012_query_history.sql`).

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use prisme_connectors::mysql::sql_log::TARGET;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id};
use tracing::{Event, Span, Subscriber};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::Context;
use tracing_subscriber::registry::LookupSpan;
use ts_rs::TS;

use super::Traced;

/// What sent a query to the server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    /// Opening of a query session: initialisation, identity of the server,
    /// current database.
    Session,
    /// Query under the cursor of an editor (Ctrl+Enter), or a table opened
    /// from the navigator.
    Run,
    /// Queries of an editor run in parallel (Ctrl+Shift+Enter).
    RunAll,
    /// Selected text of an editor only ("Exécuter précisément").
    RunExact,
    /// Refresh of a result tab.
    Refresh,
    /// Change of the current database (`USE`), from the navigator.
    Use,
    /// Interruption of a running query (`KILL QUERY`).
    Cancel,
    /// Reading of the catalog by the object navigator.
    Navigator,
    /// Test of a connection.
    Probe,
}

impl Action {
    /// Name of the action, as stored and sent to the interface.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Session => "session",
            Self::Run => "run",
            Self::RunAll => "run_all",
            Self::RunExact => "run_exact",
            Self::Refresh => "refresh",
            Self::Use => "use",
            Self::Cancel => "cancel",
            Self::Navigator => "navigator",
            Self::Probe => "probe",
        }
    }

    /// Action named `name`, `None` if unknown.
    fn parse(name: &str) -> Option<Self> {
        [
            Self::Session,
            Self::Run,
            Self::RunAll,
            Self::RunExact,
            Self::Refresh,
            Self::Use,
            Self::Cancel,
            Self::Navigator,
            Self::Probe,
        ]
        .into_iter()
        .find(|action| action.as_str() == name)
    }
}

/// Query of the history, as the interface receives it.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct QueryEntry {
    /// Number of the query in the history.
    pub id: i64,
    /// Instant the query was sent, in UTC, in ISO 8601 format.
    pub started_at: String,
    /// What sent the query.
    pub action: Action,
    /// Name of the connection, empty if unknown.
    pub connection_name: String,
    /// Text of the query, as sent.
    pub sql: String,
    /// Execution time, in microseconds: until the result, or, for a result
    /// read in streaming, until its columns.
    pub duration_us: u64,
    /// Transfer time of the rows of a result read in streaming, in
    /// microseconds; 0 when not measured.
    pub transfer_us: u64,
    /// Does the query return rows?
    pub returns_rows: bool,
    /// Number of rows read.
    pub row_count: u64,
    /// Rows changed by a write.
    pub affected_rows: u64,
    /// Were rows beyond the limit dropped?
    pub truncated: bool,
    /// Failure message, empty on success.
    pub error: String,
}

/// Query traced by the connector, with its context, before it is saved.
#[derive(Debug, Default)]
pub struct TracedQuery {
    /// User who sent it, `None` outside a user's span.
    pub user_id: Option<i64>,
    /// Instant it was sent, in milliseconds since the Unix epoch.
    pub started_at_ms: u64,
    /// What sent it, `None` outside an action's span.
    pub action: Option<Action>,
    /// Name of the connection.
    pub connection_name: String,
    /// Fields of the trace event.
    pub fields: QueryFields,
}

/// Fields of a trace event (`sql_log::record`).
#[derive(Debug, Default)]
pub struct QueryFields {
    /// Text of the query, as sent.
    pub sql: String,
    /// Execution time, in microseconds.
    pub duration_us: u64,
    /// Transfer time of the rows, in microseconds; 0 when not measured.
    pub transfer_us: u64,
    /// Does the query return rows?
    pub returns_rows: bool,
    /// Number of rows read.
    pub rows: u64,
    /// Rows changed by a write.
    pub affected_rows: u64,
    /// Were rows beyond the limit dropped?
    pub truncated: bool,
    /// Failure message, empty on success.
    pub error: String,
}

impl Visit for QueryFields {
    fn record_str(&mut self, field: &Field, value: &str) {
        match field.name() {
            "sql" => self.sql = value.to_string(),
            "error" => self.error = value.to_string(),
            _ => {}
        }
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        match field.name() {
            "duration_us" => self.duration_us = value,
            "transfer_us" => self.transfer_us = value,
            "rows" => self.rows = value,
            "affected_rows" => self.affected_rows = value,
            _ => {}
        }
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        match field.name() {
            "returns_rows" => self.returns_rows = value,
            "truncated" => self.truncated = value,
            _ => {}
        }
    }

    fn record_debug(&mut self, _field: &Field, _value: &dyn std::fmt::Debug) {
        // All the fields of `sql_log::record` are typed above.
    }
}

/// Context carried by a span of the history: user, connection, action.
#[derive(Debug, Default, Clone)]
struct SpanContext {
    /// User of the queries.
    user_id: Option<i64>,
    /// Name of their connection.
    connection_name: Option<String>,
    /// What sends them.
    action: Option<Action>,
}

impl SpanContext {
    /// Completes this context with what it lacks, from an enclosing span.
    fn inherit(&mut self, outer: &Self) {
        self.user_id = self.user_id.or(outer.user_id);
        self.action = self.action.or(outer.action);
        if self.connection_name.is_none() {
            self.connection_name.clone_from(&outer.connection_name);
        }
    }
}

impl Visit for SpanContext {
    fn record_str(&mut self, field: &Field, value: &str) {
        match field.name() {
            "connection" => self.connection_name = Some(value.to_string()),
            "action" => self.action = Action::parse(value),
            _ => {}
        }
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        if field.name() == "user_id" {
            self.user_id = Some(value);
        }
    }

    fn record_debug(&mut self, _field: &Field, _value: &dyn std::fmt::Debug) {
        // The spans of the history only carry the fields above.
    }
}

/// Span of the queries sent for user `user_id` on the connection named
/// `connection_name`.
pub fn connection_span(user_id: i64, connection_name: &str) -> Span {
    tracing::debug_span!(target: TARGET, "sql", user_id, connection = connection_name)
}

/// Span of the queries sent by `action`, within a connection's span.
pub fn action_span(action: Action) -> Span {
    tracing::debug_span!(target: TARGET, "sql", action = action.as_str())
}

/// Span of the queries sent by `action`, for user `user_id` on the
/// connection named `connection_name`, out of any session.
pub fn sql_span(user_id: i64, connection_name: &str, action: Action) -> Span {
    connection_span(user_id, connection_name).in_scope(|| action_span(action))
}

/// Layer of `tracing` that turns the trace events of the connector into
/// queries of the history, handed to the console's writer. It sees only
/// the target `TARGET` (per-layer filter set in `console::layer`).
pub struct HistoryLayer {
    /// To the console's writer.
    pub(super) sender: mpsc::UnboundedSender<Traced>,
}

impl<S> Layer<S> for HistoryLayer
where
    S: Subscriber + for<'lookup> LookupSpan<'lookup>,
{
    fn on_new_span(&self, attributes: &Attributes<'_>, id: &Id, context: Context<'_, S>) {
        let mut span_context = SpanContext::default();
        attributes.record(&mut span_context);
        if let Some(span) = context.span(id) {
            span.extensions_mut().insert(span_context);
        }
    }

    fn on_event(&self, event: &Event<'_>, context: Context<'_, S>) {
        let mut fields = QueryFields::default();
        event.record(&mut fields);
        let mut span_context = SpanContext::default();
        for span in context.event_scope(event).into_iter().flatten() {
            if let Some(outer) = span.extensions().get::<SpanContext>() {
                span_context.inherit(outer);
            }
        }
        let elapsed = Duration::from_micros(fields.duration_us.saturating_add(fields.transfer_us));
        let started = SystemTime::now()
            .checked_sub(elapsed)
            .and_then(|instant| instant.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |since| u64::try_from(since.as_millis()).unwrap_or(0));
        let _ = self.sender.send(Traced::Query(TracedQuery {
            user_id: span_context.user_id,
            started_at_ms: started,
            action: span_context.action,
            connection_name: span_context.connection_name.unwrap_or_default(),
            fields,
        }));
    }
}

/// Columns of `query_history` read into a `QueryEntry`, in order.
const COLUMNS: &str = "id, started_at, action, connection_name, sql, duration_us, transfer_us,
     returns_rows, row_count, affected_rows, truncated, error";

/// Saves a traced query in the history of `user_id` (that of the query when
/// it carries one), and returns it as the interface receives it. A query
/// sent outside an action counts as one of a session.
pub fn insert(
    db: &Connection,
    user_id: i64,
    query: &TracedQuery,
) -> Result<QueryEntry, rusqlite::Error> {
    let fields = &query.fields;
    db.query_row(
        &format!(
            "INSERT INTO query_history (user_id, started_at, action, connection_name, sql,
                 duration_us, transfer_us, returns_rows, row_count, affected_rows, truncated, error)
             VALUES (?1, strftime('%Y-%m-%dT%H:%M:%fZ', ?2 / 1000.0, 'unixepoch'), ?3, ?4, ?5,
                 ?6, ?7, ?8, ?9, ?10, ?11, ?12)
             RETURNING {COLUMNS}"
        ),
        params![
            query.user_id.unwrap_or(user_id),
            stored(query.started_at_ms),
            query.action.unwrap_or(Action::Session).as_str(),
            query.connection_name,
            fields.sql,
            stored(fields.duration_us),
            stored(fields.transfer_us),
            fields.returns_rows,
            stored(fields.rows),
            stored(fields.affected_rows),
            fields.truncated,
            fields.error,
        ],
        entry_of,
    )
}

/// Queries of the history of `user_id`, at most `limit`, the most recent
/// ones before query `before` (all of them when absent), in chronological
/// order.
pub fn list(
    db: &Connection,
    user_id: i64,
    before: Option<i64>,
    limit: u32,
) -> Result<Vec<QueryEntry>, rusqlite::Error> {
    let mut statement = db.prepare(&format!(
        "SELECT {COLUMNS} FROM query_history
         WHERE user_id = ?1 AND id < ?2 ORDER BY id DESC LIMIT ?3"
    ))?;
    let rows = statement.query_map(
        params![user_id, before.unwrap_or(i64::MAX), limit],
        entry_of,
    )?;
    let mut entries = rows.collect::<Result<Vec<_>, _>>()?;
    entries.reverse();
    Ok(entries)
}

/// Deletes the whole history of `user_id`. Returns the number of queries
/// deleted.
pub fn clear(db: &Connection, user_id: i64) -> Result<usize, rusqlite::Error> {
    db.execute("DELETE FROM query_history WHERE user_id = ?1", [user_id])
}

/// Counter as stored by SQLite, whose integers are signed: saturated at
/// `i64::MAX`. The columns refuse negative values, read back with
/// `unsigned_abs`.
fn stored(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

/// Reads a row of `query_history` (columns `COLUMNS`).
fn entry_of(row: &rusqlite::Row<'_>) -> Result<QueryEntry, rusqlite::Error> {
    Ok(QueryEntry {
        id: row.get(0)?,
        started_at: row.get(1)?,
        action: Action::parse(&row.get::<_, String>(2)?).unwrap_or(Action::Session),
        connection_name: row.get(3)?,
        sql: row.get(4)?,
        duration_us: row.get::<_, i64>(5)?.unsigned_abs(),
        transfer_us: row.get::<_, i64>(6)?.unsigned_abs(),
        returns_rows: row.get(7)?,
        row_count: row.get::<_, i64>(8)?.unsigned_abs(),
        affected_rows: row.get::<_, i64>(9)?.unsigned_abs(),
        truncated: row.get(10)?,
        error: row.get(11)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store;

    /// Traced query `sql`, sent at `started_at_ms`.
    fn traced(sql: &str, started_at_ms: u64) -> TracedQuery {
        TracedQuery {
            started_at_ms,
            action: Some(Action::Run),
            connection_name: "Local".to_string(),
            fields: QueryFields {
                sql: sql.to_string(),
                duration_us: 1200,
                returns_rows: true,
                rows: 3,
                ..QueryFields::default()
            },
            ..TracedQuery::default()
        }
    }

    #[test]
    fn saves_and_lists_the_queries_of_a_user_page_by_page() {
        let db = store::open_in_memory();
        let user_id = store::ensure_user(&db, "a@example.com").unwrap();
        let other = store::ensure_user(&db, "b@example.com").unwrap();

        let first = insert(&db, user_id, &traced("SELECT 1", 1_792_500_000_123)).unwrap();
        assert_eq!(first.started_at, "2026-10-20T12:40:00.123Z");
        assert_eq!(first.action, Action::Run);
        assert!(first.returns_rows && first.row_count == 3 && first.duration_us == 1200);
        let second = insert(&db, user_id, &traced("SELECT 2", 1_792_500_001_000)).unwrap();
        let mut foreign = traced("SELECT 3", 1_792_500_002_000);
        foreign.user_id = Some(other);
        insert(&db, user_id, &foreign).unwrap();

        let sqls =
            |entries: Vec<QueryEntry>| entries.into_iter().map(|e| e.sql).collect::<Vec<_>>();
        assert_eq!(
            sqls(list(&db, user_id, None, 10).unwrap()),
            ["SELECT 1", "SELECT 2"]
        );
        assert_eq!(sqls(list(&db, user_id, None, 1).unwrap()), ["SELECT 2"]);
        assert_eq!(
            sqls(list(&db, user_id, Some(second.id), 10).unwrap()),
            ["SELECT 1"]
        );
        assert_eq!(sqls(list(&db, other, None, 10).unwrap()), ["SELECT 3"]);
    }

    #[test]
    fn names_the_actions_both_ways() {
        for name in [
            "session",
            "run",
            "run_all",
            "run_exact",
            "refresh",
            "use",
            "cancel",
            "navigator",
            "probe",
        ] {
            assert_eq!(Action::parse(name).unwrap().as_str(), name);
        }
        assert_eq!(Action::parse("other"), None);
    }
}
