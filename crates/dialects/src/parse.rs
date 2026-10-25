//! Parsing of a text of several queries.
//!
//! `parse_statements` parses like `Parser::parse_sql`, plus the MySQL and
//! MariaDB statements that `sqlparser` does not know (`mysql_statements.rs`):
//! the classification, the syntax errors and the default limit rely on it.
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

use crate::Dialect;
use crate::mysql_statements::{self, MySqlStatement, expect_end};

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
}

/// Parses the queries of the text, separated by `;`, as `Parser::parse_sql`
/// does, with the statements of `mysql_statements.rs`.
///
/// # Errors
///
/// Returns the error of the first query not understood.
pub(crate) fn parse_statements(sql: &str, dialect: Dialect) -> Result<Vec<Parsed>, ParserError> {
    let parser_dialect = dialect.parser_dialect();
    let mut parser = Parser::new(parser_dialect.as_ref()).try_with_sql(sql)?;
    let mut statements = Vec::new();
    loop {
        while parser.consume_token(&Token::SemiColon) {}
        if parser.peek_token_ref().token == Token::EOF {
            return Ok(statements);
        }
        statements.push(parse_statement(&mut parser)?);
    }
}

/// Parses the statement at the position of the parser, up to its `;` or the
/// end of the text: a routine, an event or a `SET ... TRANSACTION` (whose
/// scope `sqlparser` drops), else a statement of `sqlparser`, else an
/// `ALTER TABLE` with an operation on an existing index (`sqlparser` reads
/// `DROP INDEX IF EXISTS i` as the index `IF`, then stops), a
/// `START TRANSACTION WITH CONSISTENT SNAPSHOT`, a `COMMIT` or `ROLLBACK`
/// with `RELEASE`, or a statement of an XA transaction.
///
/// Workaround: the bugs of `sqlparser` on `DROP INDEX IF EXISTS` and on the
/// scope of `SET TRANSACTION`, and the statements it does not parse
/// (`docs/DEVELOPER.md`, section "Workarounds").
fn parse_statement(parser: &mut Parser) -> Result<Parsed, ParserError> {
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
            mysql_statements::parse_alter_table(parser)
                .or_else(|| mysql_statements::parse_start_transaction(parser))
                .or_else(|| mysql_statements::parse_end_transaction(parser))
                .or_else(|| mysql_statements::parse_xa(parser))
                .map_or(Err(error), |statement| statement.map(Parsed::MySql))
        })
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
