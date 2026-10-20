//! Classification of a query: read or write, from its AST.
//!
//! Serves the write switches (see `docs/V1.md`, section 2.4). Classification
//! works on the AST, never on the text: `WITH ... DELETE` or
//! `SELECT ... INTO` are writes, even though they start like a read. An allow
//! list decides what a read is: everything else is a write.
//!
//! Also tells whether a query can change the current database of the
//! session (`changes_database`): the session reads it again only then.
//!
//! The queries run when a connection opens (`SET NAMES`,
//! `SET SESSION sql_mode`...) are checked separately
//! (`session_setting_refusal`): only settings of the session itself are
//! allowed.

use sqlparser::ast::{ContextModifier, ObjectType, Set, Statement};
use sqlparser::parser::Parser;

use crate::Dialect;
use crate::limit::returns_rows;

/// Nature of a query.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatementKind {
    /// Read: `SELECT` that returns rows, `SHOW`, `DESCRIBE`, `EXPLAIN`, `USE`.
    Read,
    /// Write of data or schema, procedure call, server setting...
    Write,
    /// Query that cannot be parsed: its nature is unknown.
    Unparsed,
}

/// Classifies the text `sql`. If it contains several queries, a single write is enough to classify it as a write.
pub fn classify(sql: &str, dialect: Dialect) -> StatementKind {
    let Ok(statements) = Parser::parse_sql(dialect.parser_dialect().as_ref(), sql) else {
        return StatementKind::Unparsed;
    };
    if statements.iter().all(is_read) {
        StatementKind::Read
    } else {
        StatementKind::Write
    }
}

/// Can the text `sql` change the current database of the session? `USE`,
/// `DROP DATABASE` / `DROP SCHEMA` (the current database may disappear), and
/// a text Prisme cannot parse, whose effect is unknown.
pub fn changes_database(sql: &str, dialect: Dialect) -> bool {
    let Ok(statements) = Parser::parse_sql(dialect.parser_dialect().as_ref(), sql) else {
        return true;
    };
    statements.iter().any(|statement| {
        matches!(
            statement,
            Statement::Use(_)
                | Statement::Drop {
                    object_type: ObjectType::Database | ObjectType::Schema,
                    ..
                }
        )
    })
}

/// Refusal of a query to run when a connection opens, `None` if it is
/// allowed: only session settings (`SET NAMES`, `SET SESSION x = ...`,
/// `SET @v = ...`, `SET TIME ZONE`...) are. A global setting (`SET GLOBAL`,
/// `SET @@global.x`), which would change the whole server, and any other
/// query are refused, like a query Prisme does not understand.
pub fn session_setting_refusal(sql: &str, dialect: Dialect) -> Option<String> {
    let Ok(statements) = Parser::parse_sql(dialect.parser_dialect().as_ref(), sql) else {
        return Some(format!("\"{sql}\": query not understood by Prisme"));
    };
    for statement in &statements {
        match statement {
            Statement::Set(set) if is_global(set) => {
                return Some(format!(
                    "\"{sql}\": a global setting would change the whole server; only session settings are allowed"
                ));
            }
            Statement::Set(_) => {}
            _ => {
                return Some(format!(
                    "\"{sql}\": only session settings (SET ...) are allowed"
                ));
            }
        }
    }
    None
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
    fn writes() {
        for sql in [
            "UPDATE t SET a = 1",
            "DELETE FROM t",
            "INSERT INTO t VALUES (1)",
            "REPLACE INTO t VALUES (1)",
            "CREATE TABLE t (a INT)",
            "DROP TABLE t",
            "ALTER TABLE t ADD b INT",
            "TRUNCATE TABLE t",
            "CALL p()",
            "SET GLOBAL max_connections = 10",
            "GRANT SELECT ON t TO u",
            "SELECT * INTO t2 FROM t",
            "EXPLAIN ANALYZE DELETE FROM t",
            "SELECT 1; DELETE FROM t",
        ] {
            assert_eq!(kind(sql), StatementKind::Write, "query: {sql}");
        }
    }

    #[test]
    fn unparsable() {
        assert_eq!(
            kind("SELECT * FROM t PROCEDURE ANALYSE()"),
            StatementKind::Unparsed
        );
    }
}
