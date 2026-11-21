//! Outcome of a statement, as its engine reports it: what a write did.
//!
//! Each engine reports a write in its own way (MySQL and MariaDB in the OK
//! packet; PostgreSQL in a command tag; SQLite through `changes()`...). This
//! module holds the neutral form that the session and the interface read:
//! each connector fills it from what its engine reports, and leaves a field
//! empty rather than guess it. The interface draws only the fields present,
//! without knowing the engine. The meaning of each counter, per engine, is
//! recorded in `docs/SQL-COMPARISON.md`.
//!
//! The warnings of a statement (`notices`) come with it when its engine
//! sends them along; when the engine only counts them (`warnings`), the
//! session asks the connector for them right after the statement, before
//! the next one clears them (`StatementOutcome::lacks_notices`).

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// What a statement did, as its engine reports it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(default)]
pub struct StatementOutcome {
    /// Number of rows affected, as the engine counts them (its meaning
    /// differs between engines: see `docs/SQL-COMPARISON.md`); absent when
    /// the engine does not report it.
    pub affected: Option<u64>,
    /// Rows counted by what happened to them, each count absent when the
    /// engine does not tell it.
    pub counts: OutcomeCounts,
    /// First value the statement generated for an auto-increment column, as
    /// text (an id is not always an integer); absent when it generated none.
    pub generated_id: Option<String>,
    /// Number of warnings and notes the statement raised; absent when the
    /// engine does not count them.
    pub warnings: Option<u64>,
    /// Warnings and notes the statement raised, as far as the engine keeps
    /// them (MySQL keeps 64 by default): fewer than `warnings` then.
    pub notices: Vec<Notice>,
}

impl StatementOutcome {
    /// Did the engine count warnings without sending them? They are then
    /// to read from the connection before its next statement.
    pub fn lacks_notices(&self) -> bool {
        self.warnings.is_some_and(|count| count > 0) && self.notices.is_empty()
    }
}

/// Warning or note raised by a statement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Notice {
    /// Seriousness of the notice.
    pub level: NoticeLevel,
    /// Code of the notice in the engine (`1265` for MySQL, `01000` for a
    /// SQLSTATE); empty when the engine gives none.
    pub code: String,
    /// Message of the engine, in its language.
    pub message: String,
}

/// Seriousness of a notice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum NoticeLevel {
    /// Information: nothing went wrong (a table to drop was missing).
    Note,
    /// Something was not done as written (a value truncated, a duplicate
    /// skipped).
    Warning,
    /// An error the statement went past (under `IGNORE`).
    Error,
}

/// Rows of a statement, counted by what happened to them. Each count has a
/// single meaning, whatever the engine; a row falls in one count of a
/// statement only, except `matched`, which holds the `changed` and
/// `unchanged` rows of an `UPDATE`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(default)]
pub struct OutcomeCounts {
    /// Rows the condition of an `UPDATE` found, changed or not.
    pub matched: Option<u64>,
    /// Existing rows whose values changed: by an `UPDATE`, or by the update
    /// of an upsert.
    pub changed: Option<u64>,
    /// Existing rows found and left as they were: their values were already
    /// the new ones.
    pub unchanged: Option<u64>,
    /// New rows inserted.
    pub inserted: Option<u64>,
    /// Existing rows replaced by a new one of the same key (`REPLACE`).
    pub replaced: Option<u64>,
    /// Rows deleted.
    pub deleted: Option<u64>,
    /// Rows the statement skipped, such as the duplicates of an
    /// `INSERT IGNORE`.
    pub ignored: Option<u64>,
}
