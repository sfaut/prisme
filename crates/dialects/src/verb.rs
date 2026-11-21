//! Verb of a statement: what it does to the data, as the interface names it
//! in the outcome of a statement without rows ("3 lignes insérées").
//!
//! The verb follows the classification of the statement (`classify`), from
//! its AST: the same for every engine, whatever the counters its engine
//! reports.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::classify::{StatementKind, WriteKind};

/// What a statement does to the data.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum StatementVerb {
    /// `INSERT`.
    Insert,
    /// `REPLACE`: inserts, after deleting the rows of the same key.
    Replace,
    /// Insertion that updates the rows of the same key
    /// (`INSERT ... ON DUPLICATE KEY UPDATE`).
    Upsert,
    /// `UPDATE`.
    Update,
    /// `DELETE`.
    Delete,
    /// `TRUNCATE TABLE`.
    Truncate,
    /// Change of the schema: table, view, routine, event.
    Schema,
    /// Anything else: read, setting, control of a transaction, several
    /// kinds of write at once, or a statement that cannot be parsed.
    #[default]
    Other,
}

impl StatementKind {
    /// Verb of the statement.
    pub fn verb(&self) -> StatementVerb {
        let Self::Write(kinds) = self else {
            return StatementVerb::Other;
        };
        let has = |kind| kinds.contains(&kind);
        let data = [
            WriteKind::Insert,
            WriteKind::Update,
            WriteKind::Delete,
            WriteKind::TruncateTable,
        ];
        match kinds.len() {
            1 if has(WriteKind::Insert) => StatementVerb::Insert,
            1 if has(WriteKind::Update) => StatementVerb::Update,
            1 if has(WriteKind::Delete) => StatementVerb::Delete,
            1 if has(WriteKind::TruncateTable) => StatementVerb::Truncate,
            2 if has(WriteKind::Insert) && has(WriteKind::Delete) => StatementVerb::Replace,
            2 if has(WriteKind::Insert) && has(WriteKind::Update) => StatementVerb::Upsert,
            _ if kinds.iter().all(|kind| !data.contains(kind)) => StatementVerb::Schema,
            _ => StatementVerb::Other,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::Dialect;
    use crate::classify::classify;

    use super::*;

    /// Verb of the MySQL statement `sql`.
    fn verb(sql: &str) -> StatementVerb {
        classify(sql, Dialect::MySql).verb()
    }

    #[test]
    fn names_the_writes_of_data() {
        assert_eq!(verb("INSERT INTO t VALUES (1)"), StatementVerb::Insert);
        assert_eq!(verb("INSERT INTO t SELECT * FROM u"), StatementVerb::Insert);
        assert_eq!(verb("REPLACE INTO t VALUES (1)"), StatementVerb::Replace);
        assert_eq!(
            verb("INSERT INTO t VALUES (1) ON DUPLICATE KEY UPDATE a = 2"),
            StatementVerb::Upsert
        );
        assert_eq!(verb("UPDATE t SET a = 1"), StatementVerb::Update);
        assert_eq!(verb("DELETE FROM t WHERE a = 1"), StatementVerb::Delete);
        assert_eq!(verb("TRUNCATE TABLE t"), StatementVerb::Truncate);
    }

    #[test]
    fn gathers_the_changes_of_the_schema() {
        assert_eq!(verb("CREATE TABLE t (a INT)"), StatementVerb::Schema);
        assert_eq!(verb("ALTER TABLE t ADD b INT"), StatementVerb::Schema);
        assert_eq!(verb("DROP VIEW v"), StatementVerb::Schema);
        assert_eq!(
            verb("CREATE TEMPORARY TABLE t (a INT)"),
            StatementVerb::Schema
        );
    }

    #[test]
    fn leaves_the_rest_without_a_verb() {
        assert_eq!(verb("SELECT 1"), StatementVerb::Other);
        assert_eq!(verb("SET @a = 1"), StatementVerb::Other);
        assert_eq!(verb("START TRANSACTION"), StatementVerb::Other);
        assert_eq!(verb("CALL p()"), StatementVerb::Other);
        assert_eq!(verb("NOT SQL AT ALL"), StatementVerb::Other);
    }
}
