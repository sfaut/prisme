//! Debug log of the console: the lines of the server's log, those the
//! terminal shows (same level, `RUST_LOG` and `--verbose`), and the
//! notifications of the interface (`POST /api/console/notifications`).
//!
//! The layer of this module (`DebugLayer`) turns each event of the log into
//! a line, handed to the console's writer (`console.rs`), which saves it in
//! the `debug_log` table (`migrations/0013_debug_log.sql`). The queries of
//! the history (target `sql_log::TARGET`) stay out of it: they have their
//! own tab.
//!
//! The log is that of the server: in workstation mode, its single user sees
//! all of it.

use std::fmt::Write;
use std::time::{SystemTime, UNIX_EPOCH};

use prisme_connectors::mysql::sql_log::TARGET;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::field::{Field, Visit};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::Context;
use ts_rs::TS;

use super::Traced;

/// Longest message kept, in characters: beyond, it is cut.
pub const MAX_MESSAGE: usize = 2000;

/// Longest target kept, in characters.
const MAX_TARGET: usize = 200;

/// Level of a line, from the most to the least severe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum DebugLevel {
    /// Failure.
    Error,
    /// Anomaly that does not stop the work.
    Warn,
    /// Information.
    Info,
    /// Detail for the developers.
    Debug,
    /// Finer detail.
    Trace,
}

impl DebugLevel {
    /// Name of the level, as stored.
    fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warn => "warn",
            Self::Info => "info",
            Self::Debug => "debug",
            Self::Trace => "trace",
        }
    }

    /// Level named `name`; an unknown name counts as `Info`.
    fn parse(name: &str) -> Self {
        match name {
            "error" => Self::Error,
            "warn" => Self::Warn,
            "debug" => Self::Debug,
            "trace" => Self::Trace,
            _ => Self::Info,
        }
    }
}

impl From<Level> for DebugLevel {
    /// Level of an event of `tracing`.
    fn from(level: Level) -> Self {
        match level {
            Level::ERROR => Self::Error,
            Level::WARN => Self::Warn,
            Level::INFO => Self::Info,
            Level::DEBUG => Self::Debug,
            Level::TRACE => Self::Trace,
        }
    }
}

/// Origin of a line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum DebugSource {
    /// Log of the server.
    Server,
    /// Notification shown by the interface.
    Interface,
}

impl DebugSource {
    /// Name of the source, as stored.
    fn as_str(self) -> &'static str {
        match self {
            Self::Server => "server",
            Self::Interface => "interface",
        }
    }
}

/// Line of the debug log, as the interface receives it.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct DebugEntry {
    /// Number of the line in the log.
    pub id: i64,
    /// Instant of the line, in UTC, in ISO 8601 format.
    pub logged_at: String,
    /// Level of the line.
    pub level: DebugLevel,
    /// Origin of the line.
    pub source: DebugSource,
    /// Module that wrote it (`prisme::api`), empty for the interface.
    pub target: String,
    /// Text of the line, with its fields (`key=value`).
    pub message: String,
}

/// Line of the log, before it is saved.
#[derive(Debug)]
pub struct TracedLine {
    /// Instant of the line, in milliseconds since the Unix epoch.
    pub logged_at_ms: u64,
    /// Level of the line.
    pub level: DebugLevel,
    /// Origin of the line.
    pub source: DebugSource,
    /// Module that wrote it.
    pub target: String,
    /// Text of the line.
    pub message: String,
}

impl TracedLine {
    /// Line written now.
    pub fn now(level: DebugLevel, source: DebugSource, target: &str, message: &str) -> Self {
        Self {
            logged_at_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |since| u64::try_from(since.as_millis()).unwrap_or(0)),
            level,
            source,
            target: target.chars().take(MAX_TARGET).collect(),
            message: message.chars().take(MAX_MESSAGE).collect(),
        }
    }
}

/// Text of an event: its message, then its other fields (`key=value`).
#[derive(Default)]
struct Message {
    /// Message of the event.
    text: String,
    /// Other fields.
    fields: String,
}

impl Visit for Message {
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.text.push_str(value);
        } else {
            let _ = write!(self.fields, " {}={value}", field.name());
        }
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            let _ = write!(self.text, "{value:?}");
        } else {
            let _ = write!(self.fields, " {}={value:?}", field.name());
        }
    }
}

/// Layer of `tracing` that hands the lines of the log to the console's
/// writer. It sees what the terminal shows (per-layer filter set in
/// `console::debug_layer`), except the queries of the history.
pub struct DebugLayer {
    /// To the console's writer.
    pub(super) sender: mpsc::UnboundedSender<Traced>,
}

impl<S: Subscriber> Layer<S> for DebugLayer {
    fn on_event(&self, event: &Event<'_>, _context: Context<'_, S>) {
        let metadata = event.metadata();
        if metadata.target() == TARGET {
            return;
        }
        let mut message = Message::default();
        event.record(&mut message);
        message.text.push_str(&message.fields);
        let _ = self.sender.send(Traced::Line(TracedLine::now(
            (*metadata.level()).into(),
            DebugSource::Server,
            metadata.target(),
            &message.text,
        )));
    }
}

/// Columns of `debug_log` read into a `DebugEntry`, in order.
const COLUMNS: &str = "id, logged_at, level, source, target, message";

/// Saves a line in the debug log, and returns it as the interface receives it.
pub fn insert(db: &Connection, line: &TracedLine) -> Result<DebugEntry, rusqlite::Error> {
    db.query_row(
        &format!(
            "INSERT INTO debug_log (logged_at, level, source, target, message)
             VALUES (strftime('%Y-%m-%dT%H:%M:%fZ', ?1 / 1000.0, 'unixepoch'), ?2, ?3, ?4, ?5)
             RETURNING {COLUMNS}"
        ),
        params![
            i64::try_from(line.logged_at_ms).unwrap_or(i64::MAX),
            line.level.as_str(),
            line.source.as_str(),
            line.target,
            line.message,
        ],
        entry_of,
    )
}

/// Lines of the debug log, at most `limit`, the most recent ones before
/// line `before` (all of them when absent), in chronological order.
pub fn list(
    db: &Connection,
    before: Option<i64>,
    limit: u32,
) -> Result<Vec<DebugEntry>, rusqlite::Error> {
    let mut statement = db.prepare(&format!(
        "SELECT {COLUMNS} FROM debug_log WHERE id < ?1 ORDER BY id DESC LIMIT ?2"
    ))?;
    let rows = statement.query_map(params![before.unwrap_or(i64::MAX), limit], entry_of)?;
    let mut entries = rows.collect::<Result<Vec<_>, _>>()?;
    entries.reverse();
    Ok(entries)
}

/// Deletes the whole debug log. Returns the number of lines deleted.
pub fn clear(db: &Connection) -> Result<usize, rusqlite::Error> {
    db.execute("DELETE FROM debug_log", [])
}

/// Reads a row of `debug_log` (columns `COLUMNS`).
fn entry_of(row: &rusqlite::Row<'_>) -> Result<DebugEntry, rusqlite::Error> {
    Ok(DebugEntry {
        id: row.get(0)?,
        logged_at: row.get(1)?,
        level: DebugLevel::parse(&row.get::<_, String>(2)?),
        source: if row.get::<_, String>(3)? == "interface" {
            DebugSource::Interface
        } else {
            DebugSource::Server
        },
        target: row.get(4)?,
        message: row.get(5)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store;

    #[test]
    fn saves_and_lists_the_lines_page_by_page() {
        let db = store::open_in_memory();
        let line = |message: &str| {
            TracedLine::now(
                DebugLevel::Warn,
                DebugSource::Server,
                "prisme::api",
                message,
            )
        };
        let first = insert(&db, &line("first")).unwrap();
        assert_eq!(first.level, DebugLevel::Warn);
        assert_eq!(first.target, "prisme::api");
        let second = insert(&db, &line("second")).unwrap();
        let notification = TracedLine::now(DebugLevel::Error, DebugSource::Interface, "", "Échec");
        insert(&db, &notification).unwrap();

        let messages =
            |entries: Vec<DebugEntry>| entries.into_iter().map(|e| e.message).collect::<Vec<_>>();
        assert_eq!(
            messages(list(&db, None, 10).unwrap()),
            ["first", "second", "Échec"]
        );
        assert_eq!(messages(list(&db, Some(second.id), 10).unwrap()), ["first"]);
        assert_eq!(
            list(&db, None, 1).unwrap()[0].source,
            DebugSource::Interface
        );
    }

    #[test]
    fn cuts_the_long_messages() {
        let long = "é".repeat(MAX_MESSAGE + 10);
        let line = TracedLine::now(DebugLevel::Info, DebugSource::Server, "", &long);
        assert_eq!(line.message.chars().count(), MAX_MESSAGE);
    }
}
