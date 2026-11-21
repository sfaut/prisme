//! Outcome of a statement, as its engine reports it: what a write did.
//!
//! Each engine reports a write in its own way (MySQL and MariaDB in the OK
//! packet; PostgreSQL in a command tag; SQLite through `changes()`...). This
//! module holds the neutral form that the session and the interface read:
//! each connector fills it from what its engine reports, and leaves a field
//! empty rather than guess it. The interface draws only the fields present,
//! without knowing the engine. The meaning of each counter, per engine, is
//! recorded in `docs/SQL-COMPARISON.md`.

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
}
