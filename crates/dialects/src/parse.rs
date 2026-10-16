//! Parsing of a text of several queries, query by query.
//!
//! Several beautifier rules rely on the AST (aliases, function names). An
//! editor text often contains a query that the parser does not understand,
//! among others that it does. For instance, `sqlparser` does not know the body
//! of MySQL procedures (`CREATE PROCEDURE p() BEGIN ... END`). Parsing then
//! resumes at the point of failure, token after token: the queries of the
//! body (`SELECT`, `DELETE`...) are parsed as ordinary queries.

use sqlparser::ast::Statement;
use sqlparser::parser::Parser;
use sqlparser::tokenizer::Token;

use crate::Dialect;

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
