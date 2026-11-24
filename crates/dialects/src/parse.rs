//! Parsing of a text of several queries.
//!
//! `parse_statements` parses like `Parser::parse_sql`, plus the statements
//! of the dialect that `sqlparser` does not know (`mysql_statements.rs`,
//! `sqlite_statements.rs`): the classification, the syntax errors and the
//! default limit rely on it.
//!
//! `parse_each` parses query by query, for the beautifier. Several of its rules rely on the AST (aliases, function names). An
//! editor text often contains a query that the parser does not understand,
//! among others that it does. For instance, `sqlparser` does not know the body
//! of MySQL procedures (`CREATE PROCEDURE p() BEGIN ... END`). Parsing then
//! resumes at the point of failure, token after token: the queries of the
//! body (`SELECT`, `DELETE`...) are parsed as ordinary queries.

use sqlparser::ast::Statement;
use sqlparser::parser::{Parser, ParserError};
use sqlparser::tokenizer::Token;

use crate::mysql_statements::{self, MySqlStatement};
use crate::sqlite_statements::{SqliteStatement, parse_replace, parse_sqlite_statement};
use crate::words::expect_end;
use crate::{Dialect, SessionDialect, Statements};

/// Statement parsed by `sqlparser`, or a MySQL / MariaDB statement it does
/// not know.
// As large as the `Statement`s that `Parser::parse_sql` returns: short-lived.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Parsed {
    /// Statement of `sqlparser`.
    Sql(Statement),
    /// Routine, event, `ALTER TABLE` on an existing index, or transaction
    /// statement whose syntax `sqlparser` misses.
    MySql(MySqlStatement),
    /// `PRAGMA`, `DETACH`, `VACUUM` or `REINDEX` of SQLite.
    Sqlite(SqliteStatement),
}

/// Parses the queries of the text, separated by `;`, as `Parser::parse_sql`
/// does, with the statements of `mysql_statements.rs`, in the lexical
/// settings of the session (`ANSI_QUOTES`, `NO_BACKSLASH_ESCAPES`).
///
/// # Errors
///
/// Returns the error of the first query not understood.
pub(crate) fn parse_statements(
    sql: &str,
    dialect: impl Into<SessionDialect>,
) -> Result<Vec<Parsed>, ParserError> {
    let session: SessionDialect = dialect.into();
    let parser_dialect = session.parser_dialect();
    let mut parser = Parser::new(parser_dialect.as_ref()).try_with_sql(sql)?;
    let mut statements = Vec::new();
    loop {
        while parser.consume_token(&Token::SemiColon) {}
        if parser.peek_token_ref().token == Token::EOF {
            return Ok(statements);
        }
        statements.push(match session.dialect.spec().statements {
            Statements::MySql => parse_mysql_statement(&mut parser, session.dialect)?,
            Statements::Sqlite => parse_sqlite(&mut parser)?,
        });
    }
}

/// Parses the MySQL or MariaDB statement at the position of the parser, up
/// to its `;` or the end of the text: a routine, an event or a `SET ... TRANSACTION` (whose
/// scope `sqlparser` drops), else a statement of `sqlparser`, else an
/// `ALTER TABLE` with an operation on an existing index (`sqlparser` reads
/// `DROP INDEX IF EXISTS i` as the index `IF`, then stops) or table options
/// of the dialect (`COMMENT = '...'`), a
/// `START TRANSACTION WITH CONSISTENT SNAPSHOT`, a `COMMIT` or `ROLLBACK`
/// with `RELEASE`, or a statement of an XA transaction.
///
/// Workaround: the bugs of `sqlparser` on `DROP INDEX IF EXISTS` and on the
/// scope of `SET TRANSACTION`, and the statements and table options it does
/// not parse
/// (`docs/DEVELOPER.md`, section "Workarounds").
fn parse_mysql_statement(parser: &mut Parser, dialect: Dialect) -> Result<Parsed, ParserError> {
    if let Some(statement) = mysql_statements::parse_routine_or_event(parser)
        .or_else(|| mysql_statements::parse_set_transaction(parser))
    {
        return statement.map(Parsed::MySql);
    }
    parser
        .try_parse(|parser| {
            let statement = parser.parse_statement()?;
            expect_end(parser)?;
            Ok(statement)
        })
        .map(Parsed::Sql)
        .or_else(|error| {
            mysql_statements::parse_alter_table(parser, dialect)
                .or_else(|| mysql_statements::parse_start_transaction(parser))
                .or_else(|| mysql_statements::parse_end_transaction(parser))
                .or_else(|| mysql_statements::parse_xa(parser))
                .map_or(Err(error), |statement| statement.map(Parsed::MySql))
        })
}

/// Parses the SQLite statement at the position of the parser, up to its `;`
/// or the end of the text: a `PRAGMA`, `DETACH`, `VACUUM` or `REINDEX`
/// (`sqlite_statements.rs`), a `REPLACE` read as `INSERT OR REPLACE`, else a
/// statement of `sqlparser`.
///
/// Workaround: the statements `sqlparser` does not parse (`docs/DEVELOPER.md`,
/// section "Workarounds").
fn parse_sqlite(parser: &mut Parser) -> Result<Parsed, ParserError> {
    if let Some(statement) = parse_sqlite_statement(parser) {
        return statement.map(Parsed::Sqlite);
    }
    let statement = match parse_replace(parser) {
        Some(statement) => statement?,
        None => parser.parse_statement()?,
    };
    expect_end(parser)?;
    Ok(Parsed::Sql(statement))
}

/// Parses the queries of the text one by one; after a query not understood,
/// parsing resumes at the point of failure, at least one token further.
pub(crate) fn parse_each(sql: &str, dialect: Dialect) -> Vec<Statement> {
    let parser_dialect = dialect.parser_dialect();
    let Ok(mut parser) = Parser::new(parser_dialect.as_ref()).try_with_sql(sql) else {
        return Vec::new();
    };
    let mut statements = Vec::new();
    loop {
        while parser.consume_token(&Token::SemiColon) {}
        if parser.peek_token().token == Token::EOF {
            return statements;
        }
        let before = parser.index();
        match parser.parse_statement() {
            Ok(statement) => statements.push(statement),
            Err(_) => {
                if parser.index() <= before {
                    parser.advance_token();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_statements_of_sqlite() {
        for sql in [
            "PRAGMA table_info(t)",
            "PRAGMA main.journal_mode = WAL",
            "PRAGMA foreign_keys = ON",
            "ATTACH DATABASE 'a.db' AS a",
            "DETACH a",
            "VACUUM INTO 'copy.db'",
            "REINDEX t",
            "ANALYZE main",
            "INSERT OR REPLACE INTO t VALUES (1) RETURNING *",
            "INSERT INTO t VALUES (1) ON CONFLICT (a) DO UPDATE SET b = 2",
            "UPDATE OR IGNORE t SET a = 1 FROM u WHERE t.id = u.id",
            "CREATE TEMP TRIGGER tr AFTER INSERT ON t BEGIN SELECT RAISE(ABORT, 'no'); END",
            "CREATE TABLE temp.x (a INTEGER PRIMARY KEY) STRICT, WITHOUT ROWID",
            "CREATE VIRTUAL TABLE f USING fts5(a)",
            "BEGIN IMMEDIATE; SAVEPOINT s; RELEASE s; END",
            "SELECT [a b], `c`, \"d\" FROM t INDEXED BY i LIMIT -1",
            "SELECT a ->> '$.b' FROM t WHERE a GLOB 'x*'",
        ] {
            assert!(parse_statements(sql, Dialect::Sqlite).is_ok(), "{sql}");
        }
        // `#` opens no comment in SQLite.
        assert!(parse_statements("SELECT 1 # comment", Dialect::Sqlite).is_err());
        // The statements of SQLite are none of MySQL.
        assert!(parse_statements("PRAGMA table_info(t)", Dialect::MySql).is_err());
    }

    #[test]
    fn analyses_the_statements_of_a_procedure_body() {
        let sql = "CREATE PROCEDURE p() BEGIN DELETE FROM logs; IF x THEN SELECT 1; END IF; END";
        let statements: Vec<String> = parse_each(sql, Dialect::MySql)
            .iter()
            .map(ToString::to_string)
            .collect();
        // The header is not understood; the body is parsed.
        assert!(statements.contains(&"DELETE FROM logs".to_string()));
        assert!(statements.contains(&"IF x THEN SELECT 1; END IF".to_string()));
    }
}
