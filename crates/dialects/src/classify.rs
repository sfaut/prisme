//! Classification d'une requête : lecture ou écriture, d'après son AST.
//!
//! Sert aux interrupteurs d'écriture (voir `docs/V1.md`, section 2.4). La
//! classification porte sur l'AST, jamais sur le texte : `WITH ... DELETE`
//! ou `SELECT ... INTO` sont des écritures, même s'ils commencent comme une
//! lecture. Une liste blanche décide de ce qui est une lecture : tout le
//! reste est une écriture.

use sqlparser::ast::Statement;
use sqlparser::parser::Parser;

use crate::Dialect;
use crate::limit::returns_rows;

/// Nature d'une requête.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatementKind {
    /// Lecture : `SELECT` qui renvoie des lignes, `SHOW`, `DESCRIBE`, `EXPLAIN`, `USE`.
    Read,
    /// Écriture de données ou de schéma, appel de procédure, réglage du serveur...
    Write,
    /// Requête non analysable : sa nature est inconnue.
    Unparsed,
}

/// Classe le texte `sql`. S'il contient plusieurs requêtes, une seule écriture suffit à le classer en écriture.
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

/// Indique si une requête est une lecture.
fn is_read(statement: &Statement) -> bool {
    match statement {
        Statement::Query(query) => returns_rows(&query.body),
        // EXPLAIN seul montre le plan sans exécuter ; EXPLAIN ANALYZE exécute la requête.
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
        ] {
            assert_eq!(kind(sql), StatementKind::Read, "requête : {sql}");
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
            assert_eq!(kind(sql), StatementKind::Write, "requête : {sql}");
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
