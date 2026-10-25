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
//! The structure of the temporary tables has its own switches, their data
//! stays under the switches of the data. `CREATE TEMPORARY TABLE` and
//! `DROP TEMPORARY TABLE` need them by their syntax; `ALTER`, `RENAME` and
//! `DROP TABLE` need them when all the tables they name are temporary tables
//! known to the session (`classify_with`, `temporary.rs`). `TRUNCATE TABLE`
//! empties the data: it has its switch beside `DELETE`, for every table.
//!
//! A setting of the session (`SET @v = 1`, `SET SESSION sql_mode = ...`,
//! `SET NAMES`) writes nothing in the database: it is always allowed.
//!
//! The control of transactions is always allowed too (`docs/V1.md`,
//! section 2.5): `START TRANSACTION`, `BEGIN`, `COMMIT`, `ROLLBACK`,
//! savepoints, `SET TRANSACTION`, `SET autocommit`. Its forms that can lift
//! the read-only mode of the session (`READ WRITE`, the variables
//! `transaction_read_only` and `tx_read_only`) stand apart: a read-only
//! session refuses them. `SET GLOBAL TRANSACTION` changes the whole server:
//! forbidden.
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
use sqlparser::ast::{
    ContextModifier, ObjectName, ObjectType, OnInsert, Set, SetExpr, Statement,
    TransactionAccessMode, TransactionMode,
};
use ts_rs::TS;

use crate::Dialect;
use crate::limit::returns_rows;
use crate::mysql_statements::MySqlStatement;
use crate::parse::{Parsed, parse_statements};
use crate::temporary::{TableName, table_name};

/// Nature of a query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatementKind {
    /// Read: `SELECT` that returns rows, `SHOW`, `DESCRIBE`, `EXPLAIN`, `USE`.
    Read,
    /// Setting of the session (`SET @v = 1`...), with reads at most: allowed,
    /// it only makes sense on the connection of the session.
    Setting,
    /// Control of a transaction (`START TRANSACTION`, `COMMIT`, `ROLLBACK`,
    /// savepoints, `SET TRANSACTION`, `SET autocommit`), with reads and
    /// settings at most: always allowed, on the connection of the session.
    Transaction,
    /// Control of a transaction that can lift the read-only mode of the
    /// session (`START TRANSACTION READ WRITE`, `SET TRANSACTION READ WRITE`,
    /// `transaction_read_only`, `tx_read_only`): allowed, except in a
    /// read-only session.
    ReadWriteTransaction,
    /// Write of data or schema, allowed when each of its kinds is.
    Write(BTreeSet<WriteKind>),
    /// Write that no switch allows: procedure call, database, trigger,
    /// account, file, server setting...
    Forbidden,
    /// Query that cannot be parsed: its nature is unknown.
    Unparsed,
}

/// Kind of write, allowed or not by its switch in the settings of the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum WriteKind {
    /// `INSERT`, and the insertion of `REPLACE`, temporary tables included.
    Insert,
    /// `UPDATE`, single or multiple-table, and `ON DUPLICATE KEY UPDATE`,
    /// temporary tables included.
    Update,
    /// `DELETE`, single or multiple-table, and the deletion of `REPLACE`,
    /// temporary tables included.
    Delete,
    /// `TRUNCATE TABLE`, temporary tables included.
    TruncateTable,
    /// `CREATE TABLE`.
    CreateTable,
    /// `ALTER TABLE`, `CREATE INDEX`, `DROP INDEX`.
    AlterTable,
    /// `RENAME TABLE`.
    RenameTable,
    /// `DROP TABLE`.
    DropTable,
    /// `CREATE TEMPORARY TABLE`.
    CreateTemporaryTable,
    /// `ALTER TABLE`, `CREATE INDEX`, `DROP INDEX` on a temporary table.
    AlterTemporaryTable,
    /// `RENAME TABLE` of temporary tables.
    RenameTemporaryTable,
    /// `DROP TEMPORARY TABLE`, and `DROP TABLE` of temporary tables only.
    DropTemporaryTable,
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
    pub const ALL: [Self; 21] = [
        Self::Insert,
        Self::Update,
        Self::Delete,
        Self::TruncateTable,
        Self::CreateTable,
        Self::AlterTable,
        Self::RenameTable,
        Self::DropTable,
        Self::CreateTemporaryTable,
        Self::AlterTemporaryTable,
        Self::RenameTemporaryTable,
        Self::DropTemporaryTable,
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
            Self::TruncateTable => "truncate_table",
            Self::CreateTable => "create_table",
            Self::AlterTable => "alter_table",
            Self::RenameTable => "rename_table",
            Self::DropTable => "drop_table",
            Self::CreateTemporaryTable => "create_temporary_table",
            Self::AlterTemporaryTable => "alter_temporary_table",
            Self::RenameTemporaryTable => "rename_temporary_table",
            Self::DropTemporaryTable => "drop_temporary_table",
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

/// Classifies the text `sql`, without a known temporary table. If it
/// contains several queries, it needs the kinds of all its writes; a single
/// forbidden write forbids it.
pub fn classify(sql: &str, dialect: Dialect) -> StatementKind {
    classify_with(sql, dialect, &|_| false)
}

/// Classifies the text `sql` as `classify` does; `temporary` tells whether a
/// table named in it is a temporary table of the connection.
pub fn classify_with(
    sql: &str,
    dialect: Dialect,
    temporary: &dyn Fn(&TableName) -> bool,
) -> StatementKind {
    let Ok(statements) = parse_statements(sql, dialect) else {
        return StatementKind::Unparsed;
    };
    let mut kinds = BTreeSet::new();
    let mut setting = false;
    let mut transaction = None;
    for statement in &statements {
        match need(statement, temporary) {
            Need::Read => {}
            Need::Setting => setting = true,
            Need::Transaction { read_write } => {
                transaction = Some(read_write || transaction == Some(true));
            }
            Need::Write(needed) => kinds.extend(needed),
            Need::Forbidden => return StatementKind::Forbidden,
        }
    }
    match transaction {
        _ if !kinds.is_empty() => StatementKind::Write(kinds),
        Some(true) => StatementKind::ReadWriteTransaction,
        Some(false) => StatementKind::Transaction,
        None if setting => StatementKind::Setting,
        None => StatementKind::Read,
    }
}

/// What one statement needs to run.
enum Need {
    /// Nothing: a read.
    Read,
    /// The connection of the session: a setting of the session.
    Setting,
    /// The connection of the session: a control of the transaction; with
    /// `read_write`, a read-write session too.
    Transaction {
        /// Can it lift the read-only mode of the session?
        read_write: bool,
    },
    /// The switches of these kinds of write.
    Write(Vec<WriteKind>),
    /// What no switch allows.
    Forbidden,
}

/// What a statement needs; `temporary` tells the temporary tables of the
/// connection.
fn need(statement: &Parsed, temporary: &dyn Fn(&TableName) -> bool) -> Need {
    match statement {
        Parsed::Sql(statement) if is_read(statement) => Need::Read,
        Parsed::Sql(statement) => {
            if let Some(read_write) = transaction_control(statement) {
                Need::Transaction { read_write }
            } else if matches!(statement, Statement::Set(set) if is_session_setting(set)) {
                Need::Setting
            } else {
                write_kinds(statement, temporary).map_or(Need::Forbidden, Need::Write)
            }
        }
        Parsed::MySql(MySqlStatement::SetTransaction { global: true, .. }) => Need::Forbidden,
        Parsed::MySql(
            MySqlStatement::SetTransaction { read_write, .. }
            | MySqlStatement::StartTransaction { read_write },
        ) => Need::Transaction {
            read_write: *read_write,
        },
        Parsed::MySql(statement) => mysql_write_kind(statement, temporary)
            .map_or(Need::Forbidden, |kind| Need::Write(vec![kind])),
    }
}

/// Variables that set the read-only mode of the session.
const READ_ONLY_VARIABLES: [&str; 2] = ["transaction_read_only", "tx_read_only"];

/// Is a statement of `sqlparser` a control of the transaction? `Some`, with
/// whether it can lift the read-only mode of the session; `None` for any
/// other statement. A `BEGIN ... END` block, which holds statements, is none.
fn transaction_control(statement: &Statement) -> Option<bool> {
    match statement {
        Statement::StartTransaction {
            modes,
            statements,
            exception,
            ..
        } if statements.is_empty() && exception.is_none() => Some(gives_read_write(modes)),
        Statement::Commit { .. }
        | Statement::Rollback { .. }
        | Statement::Savepoint { .. }
        | Statement::ReleaseSavepoint { .. } => Some(false),
        Statement::Set(Set::SetTransaction { modes, .. }) => Some(gives_read_write(modes)),
        Statement::Set(set) if !is_global(set) => {
            let names = assigned_variables(set);
            if names
                .iter()
                .any(|name| READ_ONLY_VARIABLES.contains(&name.as_str()))
            {
                Some(true)
            } else {
                names
                    .iter()
                    .any(|name| name == "autocommit")
                    .then_some(false)
            }
        }
        _ => None,
    }
}

/// Do transaction modes give the `READ WRITE` access mode?
fn gives_read_write(modes: &[TransactionMode]) -> bool {
    modes.contains(&TransactionMode::AccessMode(
        TransactionAccessMode::ReadWrite,
    ))
}

/// Names of the variables a `SET` assigns, in lowercase, without their
/// session scope (`@@session.autocommit` gives `autocommit`); none for the
/// other forms of `SET`.
fn assigned_variables(set: &Set) -> Vec<String> {
    let bare = |name: String| {
        let name = name.to_ascii_lowercase();
        let name = name.strip_prefix("@@").unwrap_or(&name);
        ["session.", "local."]
            .iter()
            .find_map(|scope| name.strip_prefix(scope))
            .unwrap_or(name)
            .to_string()
    };
    match set {
        Set::SingleAssignment { variable, .. } => vec![bare(variable.to_string())],
        Set::MultipleAssignments { assignments } => assignments
            .iter()
            .map(|assignment| bare(assignment.name.to_string()))
            .collect(),
        _ => Vec::new(),
    }
}

/// Is a `SET` a setting of the session that is always allowed? Not global;
/// the controls of the transaction are told apart before
/// (`transaction_control`).
fn is_session_setting(set: &Set) -> bool {
    match set {
        Set::SingleAssignment { .. } | Set::MultipleAssignments { .. } => !is_global(set),
        Set::SetNames { .. } | Set::SetNamesDefault {} | Set::SetTimeZone { .. } => true,
        _ => false,
    }
}

/// Do the names designate temporary tables only? Never for an empty list.
fn all_temporary<'a>(
    names: impl IntoIterator<Item = &'a ObjectName>,
    temporary: &dyn Fn(&TableName) -> bool,
) -> bool {
    let mut names = names.into_iter().peekable();
    names.peek().is_some()
        && names.all(|name| table_name(name).is_some_and(|table| temporary(&table)))
}

/// Kind of a write, according to whether it targets temporary tables only.
fn on(temporary: bool, kind: WriteKind, temporary_kind: WriteKind) -> WriteKind {
    if temporary { temporary_kind } else { kind }
}

/// Kinds of a write of `sqlparser`, `None` if no switch allows it;
/// `temporary` tells the temporary tables of the connection.
fn write_kinds(
    statement: &Statement,
    temporary: &dyn Fn(&TableName) -> bool,
) -> Option<Vec<WriteKind>> {
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
                return write_kinds(statement, temporary);
            }
            _ => return None,
        },
        // EXPLAIN ANALYZE runs the query.
        Statement::Explain { statement, .. } => return write_kinds(statement, temporary),
        Statement::CreateTable(create) => {
            vec![on(create.temporary, CreateTable, CreateTemporaryTable)]
        }
        Statement::AlterTable(alter) => vec![on(
            all_temporary([&alter.name], temporary),
            AlterTable,
            AlterTemporaryTable,
        )],
        Statement::CreateIndex(index) => vec![on(
            all_temporary([&index.table_name], temporary),
            AlterTable,
            AlterTemporaryTable,
        )],
        Statement::RenameTable(renames) => vec![on(
            all_temporary(renames.iter().map(|rename| &rename.old_name), temporary),
            RenameTable,
            RenameTemporaryTable,
        )],
        Statement::Truncate(_) => vec![TruncateTable],
        Statement::CreateView(_) | Statement::AlterView { .. } => vec![CreateView],
        Statement::Drop {
            object_type,
            names,
            temporary: drop_temporary,
            table,
            ..
        } => match object_type {
            // DROP TEMPORARY TABLE only drops temporary tables.
            ObjectType::Table => vec![on(
                *drop_temporary || all_temporary(names, temporary),
                DropTable,
                DropTemporaryTable,
            )],
            ObjectType::View => vec![DropView],
            ObjectType::Index => vec![on(
                all_temporary(table, temporary),
                AlterTable,
                AlterTemporaryTable,
            )],
            _ => return None,
        },
        Statement::DropFunction(_) => vec![DropFunction],
        Statement::DropProcedure { .. } => vec![DropProcedure],
        _ => return None,
    };
    Some(kinds)
}

/// Kind of a write that `sqlparser` does not parse, `None` for a statement
/// that writes nothing (a control of the transaction); `temporary` tells the
/// temporary tables of the connection.
fn mysql_write_kind(
    statement: &MySqlStatement,
    temporary: &dyn Fn(&TableName) -> bool,
) -> Option<WriteKind> {
    Some(match statement {
        MySqlStatement::CreateProcedure { .. } => WriteKind::CreateProcedure,
        MySqlStatement::CreateFunction { .. } => WriteKind::CreateFunction,
        MySqlStatement::CreateEvent { .. } => WriteKind::CreateEvent,
        MySqlStatement::AlterEvent { .. } => WriteKind::AlterEvent,
        MySqlStatement::DropEvent { .. } => WriteKind::DropEvent,
        MySqlStatement::AlterTable { name } => on(
            all_temporary([name], temporary),
            WriteKind::AlterTable,
            WriteKind::AlterTemporaryTable,
        ),
        MySqlStatement::SetTransaction { .. } | MySqlStatement::StartTransaction { .. } => {
            return None;
        }
    })
}

/// Does the text `sql` end a transaction with `COMMIT` or `ROLLBACK` (not
/// to a savepoint)? The server may close the connection then
/// (`completion_type = 2`).
pub fn ends_transaction(sql: &str, dialect: Dialect) -> bool {
    parse_statements(sql, dialect).is_ok_and(|statements| {
        statements.iter().any(|statement| {
            matches!(
                statement,
                Parsed::Sql(
                    Statement::Commit { .. }
                        | Statement::Rollback {
                            savepoint: None,
                            ..
                        }
                )
            )
        })
    })
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
            Parsed::MySql(MySqlStatement::SetTransaction { global: true, .. }) => {
                return Some(SettingRefusal::Global);
            }
            Parsed::Sql(Statement::Set(_))
            | Parsed::MySql(MySqlStatement::SetTransaction { .. }) => {}
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
        assert!(!refused(
            "SET SESSION TRANSACTION ISOLATION LEVEL READ COMMITTED"
        ));
        assert!(refused("SET GLOBAL max_connections = 10"));
        assert!(refused("SET @@global.sql_mode = ''"));
        assert_eq!(
            session_setting_refusal("SET GLOBAL TRANSACTION READ WRITE", Dialect::MariaDb),
            Some(SettingRefusal::Global)
        );
        assert!(refused("START TRANSACTION"));
        assert!(refused("DELETE FROM t"));
        assert!(refused("SELECT 1"));
        assert!(refused("SET whatever you like"));
    }

    #[test]
    fn tells_the_queries_that_end_a_transaction() {
        let ends = |sql| ends_transaction(sql, Dialect::MySql);
        assert!(ends("COMMIT"));
        assert!(ends("SELECT 1; ROLLBACK"));
        assert!(!ends("ROLLBACK TO SAVEPOINT s"));
        assert!(!ends("START TRANSACTION"));
        assert!(!ends("SELECT 1 FROM"));
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
            (
                "CREATE TEMPORARY TABLE t (a INT)",
                vec![CreateTemporaryTable],
            ),
            ("CREATE TABLE t AS SELECT 1", vec![CreateTable]),
            ("ALTER TABLE t ADD b INT", vec![AlterTable]),
            ("ALTER TABLE t ALTER INDEX i INVISIBLE", vec![AlterTable]),
            ("ALTER TABLE t RENAME KEY i TO j", vec![AlterTable]),
            ("CREATE INDEX i ON t (a)", vec![AlterTable]),
            ("DROP INDEX i ON t", vec![AlterTable]),
            ("RENAME TABLE t TO u", vec![RenameTable]),
            ("TRUNCATE TABLE t", vec![TruncateTable]),
            ("DROP TABLE t", vec![DropTable]),
            ("DROP TEMPORARY TABLE IF EXISTS t", vec![DropTemporaryTable]),
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
    fn changes_of_temporary_tables_only_need_their_switches() {
        use WriteKind::*;
        // `tmp` and `shop.tmp2` are temporary tables of the connection.
        let temporary = |table: &TableName| {
            matches!(
                (table.schema.as_deref(), table.name.as_str()),
                (None, "tmp") | (Some("shop"), "tmp2")
            )
        };
        let kind = |sql| classify_with(sql, Dialect::MySql, &temporary);
        for (sql, kinds) in [
            // The data of the temporary tables: the switches of the data.
            ("INSERT INTO tmp SELECT * FROM t", vec![Insert]),
            ("REPLACE INTO shop.tmp2 VALUES (1)", vec![Insert, Delete]),
            ("UPDATE tmp SET a = 1", vec![Update]),
            ("DELETE FROM tmp", vec![Delete]),
            ("TRUNCATE TABLE tmp", vec![TruncateTable]),
            ("ALTER TABLE tmp ADD b INT", vec![AlterTemporaryTable]),
            (
                "ALTER TABLE tmp ALTER INDEX i INVISIBLE",
                vec![AlterTemporaryTable],
            ),
            ("CREATE INDEX i ON tmp (a)", vec![AlterTemporaryTable]),
            ("DROP INDEX i ON tmp", vec![AlterTemporaryTable]),
            ("RENAME TABLE tmp TO tmp3", vec![RenameTemporaryTable]),
            ("RENAME TABLE tmp TO a, t TO b", vec![RenameTable]),
            ("DROP TABLE tmp, shop.tmp2", vec![DropTemporaryTable]),
            // A permanent table among the tables: the switch of the tables.
            ("DROP TABLE tmp, t", vec![DropTable]),
            ("DROP TEMPORARY TABLE t", vec![DropTemporaryTable]),
            // Another database, or another case: another table.
            ("DROP TABLE shop.tmp", vec![DropTable]),
            ("DROP TABLE TMP", vec![DropTable]),
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
            "SET GLOBAL TRANSACTION READ WRITE",
            "SET GLOBAL TRANSACTION ISOLATION LEVEL READ COMMITTED",
            "SET GLOBAL autocommit = 0",
            "SELECT * INTO t2 FROM t",
            "DELETE FROM t; CALL p()",
            "COMMIT; CALL p()",
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
    fn allows_the_control_of_transactions() {
        for sql in [
            "START TRANSACTION",
            "START TRANSACTION READ ONLY",
            "START TRANSACTION WITH CONSISTENT SNAPSHOT",
            "BEGIN",
            "BEGIN WORK",
            "COMMIT",
            "COMMIT AND CHAIN",
            "ROLLBACK",
            "ROLLBACK TO SAVEPOINT s",
            "SAVEPOINT s",
            "RELEASE SAVEPOINT s",
            "SET TRANSACTION ISOLATION LEVEL READ COMMITTED",
            "SET SESSION TRANSACTION READ ONLY",
            "SET autocommit = 0",
            "SET @a = 1, @@session.autocommit = 0",
            "START TRANSACTION; SELECT 1; COMMIT",
        ] {
            assert_eq!(kind(sql), StatementKind::Transaction, "query: {sql}");
        }
        // The forms that can lift the read-only mode of the session.
        for sql in [
            "START TRANSACTION READ WRITE",
            "START TRANSACTION WITH CONSISTENT SNAPSHOT, READ WRITE",
            "SET TRANSACTION READ WRITE",
            "SET SESSION TRANSACTION ISOLATION LEVEL SERIALIZABLE, READ WRITE",
            "SET SESSION transaction_read_only = OFF",
            "SET @@tx_read_only = 0",
            "BEGIN; SET TRANSACTION READ WRITE",
        ] {
            assert_eq!(
                kind(sql),
                StatementKind::ReadWriteTransaction,
                "query: {sql}"
            );
        }
        // A write in the text needs its switch.
        assert_eq!(
            kind("START TRANSACTION; DELETE FROM t"),
            StatementKind::Write([WriteKind::Delete].into_iter().collect())
        );
        // `transaction_isolation` is a setting of the session.
        assert_eq!(
            kind("SET SESSION transaction_isolation = 'READ-COMMITTED'"),
            StatementKind::Setting
        );
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
