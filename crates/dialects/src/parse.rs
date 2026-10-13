//! Analyse d'un texte de plusieurs requêtes, requête par requête.
//!
//! Plusieurs règles de l'enjoliveur s'appuient sur l'AST (alias, noms de
//! fonctions). Un texte de l'éditeur contient souvent une requête que
//! l'analyseur ne comprend pas, au milieu d'autres qu'il comprend. Ainsi,
//! `sqlparser` ne connaît pas le corps des procédures de MySQL
//! (`CREATE PROCEDURE p() BEGIN ... END`). L'analyse reprend alors au point
//! d'échec, jeton après jeton : les requêtes du corps (`SELECT`, `DELETE`...)
//! sont analysées comme des requêtes ordinaires.

use sqlparser::ast::Statement;
use sqlparser::parser::Parser;
use sqlparser::tokenizer::Token;

use crate::Dialect;

/// Analyse les requêtes du texte une à une ; après une requête incomprise,
/// l'analyse reprend au point d'échec, au moins un jeton plus loin.
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
        // L'en-tête est incompris ; le corps, lui, est analysé.
        assert!(statements.contains(&"DELETE FROM logs".to_string()));
        assert!(statements.contains(&"IF x THEN SELECT 1; END IF".to_string()));
    }
}
