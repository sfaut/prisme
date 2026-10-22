//! Classification of a query: read or write, from its AST.
//!
//! Serves the write switches (see `docs/V1.md`, section 2.4). Classification
//! works on the AST, never on the text: `WITH ... DELETE` or
//! `SELECT ... INTO` are writes, even though they start like a read. An allow
//! list decides what a read is: everything else is a write.
//!
//! A write has the kinds of the switches it needs (`WriteKind`): `REPLACE`
//! inserts and deletes, `INSERT ... ON DUPLICATE KEY UPDATE` inserts and
//! updates, `CREATE INDEX` / `DROP INDEX` change the table, `ALTER VIEW`
//! creates the view again. A write that no switch covers (`CALL`, database,
//! trigger, account, file, server setting...) is forbidden.
//!
//! A setting of the session (`SET @v = 1`, `SET SESSION sql_mode = ...`,
//! `SET NAMES`) writes nothing in the database: it is always allowed, except
//! when it would lift the read-only mode or open a transaction
//! (`transaction_read_only`, `tx_read_only`, `autocommit`,
//! `SET TRANSACTION`).
//!
//! Also tells whether a query can change the current database of the
//! session (`changes_database`): the session reads it again only then.
//!
//! The queries run when a connection opens (`SET NAMES`,
//! `SET SESSION sql_mode`...) are checked separately
//! (`session_setting_refusal`): only settings of the session itself are
//! allowed.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use sqlparser::ast::{ContextModifier, ObjectType, OnInsert, Set, SetExpr, Statement};
use ts_rs::TS;

use crate::Dialect;
use crate::limit::returns_rows;
use crate::mysql_statements::MySqlStatement;
use crate::parse::{Parsed, parse_statements};

/// Nature of a query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatementKind {
    /// Read: `SELECT` that returns rows, `SHOW`, `DESCRIBE`, `EXPLAIN`, `USE`.
    Read,
    /// Setting of the session (`SET @v = 1`...), with reads at most: allowed,
    /// it only makes sense on the connection of the session.
    Setting,
    /// Write of data or schema, allowed when each of its kinds is.
    Write(BTreeSet<WriteKind>),
    /// Write that no switch allows: procedure call, database, trigger,
    /// account, file, server setting, transaction, read-only mode or
    /// autocommit of the session...
    Forbidden,
    /// Query that cannot be parsed: its nature is unknown.
    Unparsed,
}

/// Kind of write, allowed or not by its switch in the settings of the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum WriteKind {
    /// `INSERT`, and the insertion of `REPLACE`.
    Insert,
    /// `UPDATE`, single or multiple-table, and `ON DUPLICATE KEY UPDATE`.
    Update,
    /// `DELETE`, single or multiple-table, and the deletion of `REPLACE`.
    Delete,
    /// `CREATE [TEMPORARY] TABLE`.
    CreateTable,
    /// `ALTER TABLE`, `CREATE INDEX`, `DROP INDEX`.
    AlterTable,
    /// `RENAME TABLE`.
    RenameTable,
    /// `TRUNCATE TABLE`.
    TruncateTable,
    /// `DROP [TEMPORARY] TABLE`.
    DropTable,
    /// `CREATE [OR REPLACE] VIEW`, `ALTER VIEW`.
    CreateView,
    /// `DROP VIEW`.
    DropView,
    /// `CREATE FUNCTION` (stored function).
    CreateFunction,
    /// `DROP FUNCTION`.
    DropFunction,
    /// `CREATE PROCEDURE`.
    CreateProcedure,
    /// `DROP PROCEDURE`.
    DropProcedure,
    /// `CREATE EVENT`.
    CreateEvent,
    /// `ALTER EVENT`.
    AlterEvent,
    /// `DROP EVENT`.
    DropEvent,
}

impl WriteKind {
    /// Every kind, in the order of the settings.
    pub const ALL: [Self; 17] = [
        Self::Insert,
        Self::Update,
        Self::Delete,
        Self::CreateTable,
        Self::AlterTable,
        Self::RenameTable,
        Self::TruncateTable,
        Self::DropTable,
        Self::CreateView,
        Self::DropView,
        Self::CreateFunction,
        Self::DropFunction,
        Self::CreateProcedure,
        Self::DropProcedure,
        Self::CreateEvent,
        Self::AlterEvent,
        Self::DropEvent,
    ];

    /// Name of the kind, as serialized (`create_table`).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Insert => "insert",
            Self::Update => "update",
            Self::Delete => "delete",
            Self::CreateTable => "create_table",
            Self::AlterTable => "alter_table",
            Self::RenameTable => "rename_table",
            Self::TruncateTable => "truncate_table",
            Self::DropTable => "drop_table",
            Self::CreateView => "create_view",
            Self::DropView => "drop_view",
            Self::CreateFunction => "create_function",
            Self::DropFunction => "drop_function",
            Self::CreateProcedure => "create_procedure",
            Self::DropProcedure => "drop_procedure",
            Self::CreateEvent => "create_event",
            Self::AlterEvent => "alter_event",
            Self::DropEvent => "drop_event",
        }
    }

    /// Kind of the name `name` (`create_table`), `None` if unknown.
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.as_str() == name)
    }
}

/// Classifies the text `sql`. If it contains several queries, it needs the
/// kinds of all its writes; a single forbidden write forbids it.
pub fn classify(sql: &str, dialect: Dialect) -> StatementKind {
    let Ok(statements) = parse_statements(sql, dialect) else {
        return StatementKind::Unparsed;
    };
    let mut kinds = BTreeSet::new();
    let mut setting = false;
    for statement in &statements {
        let needed = match statement {
            Parsed::Sql(statement) if is_read(statement) => Some(Vec::new()),
            Parsed::Sql(Statement::Set(set)) if is_session_setting(set) => {
                setting = true;
                Some(Vec::new())
            }
            Parsed::Sql(statement) => write_kinds(statement),
            Parsed::MySql(statement) => Some(vec![mysql_write_kind(statement)]),
        };
        let Some(needed) = needed else {
            return StatementKind::Forbidden;
        };
        kinds.extend(needed);
    }
    if !kinds.is_empty() {
        StatementKind::Write(kinds)
    } else if setting {
        StatementKind::Setting
    } else {
        StatementKind::Read
    }
}

/// Variables that keep the session in its mode: read-only, and one
/// transaction per query (`autocommit`).
const GUARDED_VARIABLES: [&str; 3] = ["transaction_read_only", "tx_read_only", "autocommit"];

/// Is a `SET` a setting of the session that is always allowed? Neither
/// global nor on a variable of `GUARDED_VARIABLES`; `SET TRANSACTION` and
/// the other forms are not.
fn is_session_setting(set: &Set) -> bool {
    let guarded = |name: &str| {
        let name = name.to_ascii_lowercase();
        let name = name.strip_prefix("@@").unwrap_or(&name);
        let name = ["session.", "local."]
            .iter()
            .find_map(|scope| name.strip_prefix(scope))
            .unwrap_or(name);
        GUARDED_VARIABLES.contains(&name)
    };
    match set {
        Set::SingleAssignment { variable, .. } => {
            !is_global(set) && !guarded(&variable.to_string())
        }
        Set::MultipleAssignments { assignments } => {
            !is_global(set)
                && !assignments
                    .iter()
                    .any(|assignment| guarded(&assignment.name.to_string()))
        }
        Set::SetNames { .. } | Set::SetNamesDefault {} | Set::SetTimeZone { .. } => true,
        _ => false,
    }
}

/// Kinds of a write of `sqlparser`, `None` if no switch allows it.
fn write_kinds(statement: &Statement) -> Option<Vec<WriteKind>> {
    use WriteKind::*;
    let kinds = match statement {
        Statement::Insert(insert) => {
            let mut kinds = vec![Insert];
            if insert.replace_into {
                kinds.push(Delete);
            }
            if matches!(insert.on, Some(OnInsert::DuplicateKeyUpdate(_))) {
                kinds.push(Update);
            }
            kinds
        }
        Statement::Update(_) => vec![Update],
        Statement::Delete(_) => vec![Delete],
        // WITH ... INSERT / UPDATE / DELETE; SELECT ... INTO stays forbidden.
        Statement::Query(query) => match query.body.as_ref() {
            SetExpr::Insert(statement)
            | SetExpr::Update(statement)
            | SetExpr::Delete(statement) => {
                return write_kinds(statement);
            }
            _ => return None,
        },
        // EXPLAIN ANALYZE runs the query.
        Statement::Explain { statement, .. } => return write_kinds(statement),
        Statement::CreateTable(_) => vec![CreateTable],
        Statement::AlterTable(_) | Statement::CreateIndex(_) => vec![AlterTable],
        Statement::RenameTable(_) => vec![RenameTable],
        Statement::Truncate(_) => vec![TruncateTable],
        Statement::CreateView(_) | Statement::AlterView { .. } => vec![CreateView],
        Statement::Drop { object_type, .. } => match object_type {
            ObjectType::Table => vec![DropTable],
            ObjectType::View => vec![DropView],
            ObjectType::Index => vec![AlterTable],
            _ => return None,
        },
        Statement::DropFunction(_) => vec![DropFunction],
        Statement::DropProcedure { .. } => vec![DropProcedure],
        _ => return None,
    };
    Some(kinds)
}

/// Kind of a write that `sqlparser` does not parse.
fn mysql_write_kind(statement: &MySqlStatement) -> WriteKind {
    match statement {
        MySqlStatement::CreateProcedure { .. } => WriteKind::CreateProcedure,
        MySqlStatement::CreateFunction { .. } => WriteKind::CreateFunction,
        MySqlStatement::CreateEvent { .. } => WriteKind::CreateEvent,
        MySqlStatement::AlterEvent { .. } => WriteKind::AlterEvent,
        MySqlStatement::DropEvent { .. } => WriteKind::DropEvent,
        MySqlStatement::AlterTable { .. } => WriteKind::AlterTable,
    }
}

/// Can the text `sql` change the current database of the session? `USE`,
/// `DROP DATABASE` / `DROP SCHEMA` (the current database may disappear), and
/// a text Prisme cannot parse, whose effect is unknown.
pub fn changes_database(sql: &str, dialect: Dialect) -> bool {
    let Ok(statements) = parse_statements(sql, dialect) else {
        return true;
    };
    statements.iter().any(|statement| {
        matches!(
            statement,
            Parsed::Sql(
                Statement::Use(_)
                    | Statement::Drop {
                        object_type: ObjectType::Database | ObjectType::Schema,
                        ..
                    }
            )
        )
    })
}

/// Refusal of a query to run when a connection opens, `None` if it is
/// allowed: only session settings (`SET NAMES`, `SET SESSION x = ...`,
/// `SET @v = ...`, `SET TIME ZONE`...) are. A global setting (`SET GLOBAL`,
/// `SET @@global.x`), which would change the whole server, and any other
/// query are refused, like a query Prisme does not understand.
pub fn session_setting_refusal(sql: &str, dialect: Dialect) -> Option<SettingRefusal> {
    let Ok(statements) = parse_statements(sql, dialect) else {
        return Some(SettingRefusal::NotUnderstood);
    };
    for statement in &statements {
        match statement {
            Parsed::Sql(Statement::Set(set)) if is_global(set) => {
                return Some(SettingRefusal::Global);
            }
            Parsed::Sql(Statement::Set(_)) => {}
            _ => return Some(SettingRefusal::NotASetting),
        }
    }
    None
}

/// Why a query to run when a connection opens is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingRefusal {
    /// Prisme does not understand the query.
    NotUnderstood,
    /// A global setting, which would change the whole server.
    Global,
    /// Not a setting (`SET ...`).
    NotASetting,
}

/// Does a `SET` target the whole server (`GLOBAL`, `@@global.x`, `@@persist.x`)?
fn is_global(set: &Set) -> bool {
    let global_name = |name: &str| {
        let name = name.to_ascii_lowercase();
        name.starts_with("@@global.") || name.starts_with("@@persist")
    };
    match set {
        Set::SingleAssignment {
            scope, variable, ..
        } => scope == &Some(ContextModifier::Global) || global_name(&variable.to_string()),
        Set::MultipleAssignments { assignments } => assignments.iter().any(|assignment| {
            assignment.scope == Some(ContextModifier::Global)
                || global_name(&assignment.name.to_string())
        }),
        _ => false,
    }
}

/// Tells whether a query is a read.
fn is_read(statement: &Statement) -> bool {
    match statement {
        Statement::Query(query) => returns_rows(&query.body),
        // EXPLAIN alone shows the plan without running; EXPLAIN ANALYZE runs the query.
        Statement::Explain {
            analyze, statement, ..
        } => !analyze || is_read(statement),
        Statement::ExplainTable { .. }
        | Statement::ShowFunctions { .. }
        | Statement::ShowVariable { .. }
        | Statement::ShowStatus { .. }
        | Statement::ShowVariables { .. }
        | Statement::ShowCreate { .. }
        | Statement::ShowColumns { .. }
        | Statement::ShowCatalogs { .. }
        | Statement::ShowDatabases { .. }
        | Statement::ShowProcessList { .. }
        | Statement::ShowSchemas { .. }
        | Statement::ShowCharset(_)
        | Statement::ShowObjects(_)
        | Statement::ShowTables { .. }
        | Statement::ShowViews { .. }
        | Statement::ShowCollation { .. }
        | Statement::Use(_) => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_only_session_settings_at_connection() {
        let refused = |sql| session_setting_refusal(sql, Dialect::MariaDb).is_some();
        assert!(!refused("SET NAMES utf8mb4"));
        assert!(!refused("SET SESSION sql_mode = 'ANSI_QUOTES'"));
        assert!(!refused("SET @start = NOW()"));
        assert!(!refused("SET time_zone = '+00:00'"));
        assert!(refused("SET GLOBAL max_connections = 10"));
        assert!(refused("SET @@global.sql_mode = ''"));
        assert!(refused("DELETE FROM t"));
        assert!(refused("SELECT 1"));
        assert!(refused("SET whatever you like"));
    }

    #[test]
    fn tells_the_queries_that_can_change_the_current_database() {
        let changes = |sql| changes_database(sql, Dialect::MySql);
        assert!(changes("USE sakila"));
        assert!(changes("DROP DATABASE sakila"));
        assert!(changes("DROP SCHEMA IF EXISTS sakila"));
        assert!(changes("SELECT 1 FROM"));
        assert!(!changes("SELECT DATABASE()"));
        assert!(!changes("SHOW TABLES"));
        assert!(!changes("DROP TABLE t"));
    }

    fn kind(sql: &str) -> StatementKind {
        classify(sql, Dialect::MySql)
    }

    #[test]
    fn allows_the_settings_of_the_session() {
        for sql in [
            "SET @a = 1",
            "SET @a = 1, @b = 'x'",
            "SET SESSION sql_mode = 'ANSI_QUOTES'",
            "SET @@session.sql_mode = ''",
            "SET LOCAL time_zone = '+00:00'",
            "SET sql_safe_updates = 1",
            "SET NAMES utf8mb4",
            "SET @autocommit = 0",
            "SET @a = 1; SELECT @a",
        ] {
            assert_eq!(kind(sql), StatementKind::Setting, "query: {sql}");
        }
        assert_eq!(
            kind("SET @a = 1; DELETE FROM t"),
            StatementKind::Write([WriteKind::Delete].into())
        );
    }

    #[test]
    fn reads() {
        for sql in [
            "SELECT * FROM t",
            "SELECT * FROM t FOR UPDATE",
            "WITH x AS (SELECT 1) SELECT * FROM x",
            "SELECT a FROM t UNION SELECT a FROM u",
            "SHOW TABLES",
            "SHOW CREATE TABLE t",
            "SHOW VARIABLES LIKE 'sql_mode'",
            "DESCRIBE t",
            "EXPLAIN SELECT * FROM t",
            "EXPLAIN UPDATE t SET a = 1",
            "USE test",
            // Compound time units, unknown to `sqlparser` (`mysql_parser` module).
            "select extract(year_month from operation_date) from accounts.operations",
            "SELECT DATE_ADD(d, INTERVAL '1 2' DAY_HOUR) FROM t",
        ] {
            assert_eq!(kind(sql), StatementKind::Read, "query: {sql}");
        }
    }

    #[test]
    fn writes_need_the_switches_of_their_kinds() {
        use WriteKind::*;
        for (sql, kinds) in [
            ("INSERT INTO t VALUES (1)", vec![Insert]),
            ("INSERT INTO t SELECT * FROM u", vec![Insert]),
            (
                "INSERT INTO t VALUES (1) ON DUPLICATE KEY UPDATE a = 2",
                vec![Insert, Update],
            ),
            ("REPLACE INTO t VALUES (1)", vec![Insert, Delete]),
            ("UPDATE t SET a = 1", vec![Update]),
            ("UPDATE t JOIN u ON t.a = u.a SET t.b = u.b", vec![Update]),
            ("DELETE FROM t", vec![Delete]),
            ("DELETE t FROM t JOIN u ON t.a = u.a", vec![Delete]),
            (
                "WITH x AS (SELECT 1 AS a) DELETE FROM t WHERE a IN (SELECT a FROM x)",
                vec![Delete],
            ),
            ("EXPLAIN ANALYZE DELETE FROM t", vec![Delete]),
            ("CREATE TABLE t (a INT)", vec![CreateTable]),
            ("CREATE TEMPORARY TABLE t (a INT)", vec![CreateTable]),
            ("CREATE TABLE t AS SELECT 1", vec![CreateTable]),
            ("ALTER TABLE t ADD b INT", vec![AlterTable]),
            ("ALTER TABLE t ALTER INDEX i INVISIBLE", vec![AlterTable]),
            ("ALTER TABLE t RENAME KEY i TO j", vec![AlterTable]),
            ("CREATE INDEX i ON t (a)", vec![AlterTable]),
            ("DROP INDEX i ON t", vec![AlterTable]),
            ("RENAME TABLE t TO u", vec![RenameTable]),
            ("TRUNCATE TABLE t", vec![TruncateTable]),
            ("DROP TABLE t", vec![DropTable]),
            ("DROP TEMPORARY TABLE IF EXISTS t", vec![DropTable]),
            ("CREATE VIEW v AS SELECT 1", vec![CreateView]),
            ("CREATE OR REPLACE VIEW v AS SELECT 1", vec![CreateView]),
            ("ALTER VIEW v AS SELECT 2", vec![CreateView]),
            ("DROP VIEW v", vec![DropView]),
            (
                "CREATE FUNCTION f() RETURNS INT RETURN 1",
                vec![CreateFunction],
            ),
            ("DROP FUNCTION f", vec![DropFunction]),
            (
                "CREATE PROCEDURE p() BEGIN SELECT 1; END",
                vec![CreateProcedure],
            ),
            ("DROP PROCEDURE IF EXISTS p", vec![DropProcedure]),
            (
                "CREATE EVENT e ON SCHEDULE EVERY 1 DAY DO DELETE FROM t",
                vec![CreateEvent],
            ),
            ("ALTER EVENT e DISABLE", vec![AlterEvent]),
            ("DROP EVENT e", vec![DropEvent]),
            (
                "SELECT 1; DELETE FROM t; INSERT INTO t VALUES (1)",
                vec![Insert, Delete],
            ),
        ] {
            assert_eq!(
                kind(sql),
                StatementKind::Write(kinds.into_iter().collect()),
                "query: {sql}"
            );
        }
    }

    #[test]
    fn forbids_the_writes_without_a_switch() {
        for sql in [
            "CALL p()",
            "CREATE DATABASE d",
            "DROP DATABASE d",
            "DROP TRIGGER g",
            "GRANT SELECT ON t TO u",
            "SET GLOBAL max_connections = 10",
            "SET @@global.sql_mode = ''",
            "SET SESSION TRANSACTION READ WRITE",
            "SET TRANSACTION ISOLATION LEVEL READ COMMITTED",
            "SET SESSION transaction_read_only = OFF",
            "SET @@tx_read_only = 0",
            "SET autocommit = 0",
            "SET @a = 1, @@session.autocommit = 0",
            "SELECT * INTO t2 FROM t",
            "START TRANSACTION",
            "DELETE FROM t; CALL p()",
        ] {
            assert_eq!(kind(sql), StatementKind::Forbidden, "query: {sql}");
        }
        // Refused too, as `sqlparser` does not parse their MySQL syntax.
        for sql in [
            "LOAD DATA INFILE 'f.csv' INTO TABLE t",
            "SET PERSIST max_connections = 10",
            "SELECT * INTO OUTFILE '/tmp/t.csv' FROM t",
            "CREATE TRIGGER g BEFORE INSERT ON t FOR EACH ROW SET NEW.a = 1",
        ] {
            assert_eq!(kind(sql), StatementKind::Unparsed, "query: {sql}");
        }
    }

    #[test]
    fn names_the_kinds_as_serialized() {
        for kind in WriteKind::ALL {
            let name = yaml_serde::to_string(&kind).unwrap();
            assert_eq!(name.trim(), kind.as_str());
            assert_eq!(WriteKind::from_name(kind.as_str()), Some(kind));
        }
        assert_eq!(WriteKind::from_name("grant"), None);
    }

    #[test]
    fn unparsable() {
        assert_eq!(
            kind("SELECT * FROM t PROCEDURE ANALYSE()"),
            StatementKind::Unparsed
        );
    }
}
