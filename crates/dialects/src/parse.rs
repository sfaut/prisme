//! Analyse d'un texte de plusieurs requêtes, requête par requête.
//!
//! Plusieurs règles de l'enjoliveur s'appuient sur l'AST (alias, noms de
//! fonctions). Un texte de l'éditeur contient souvent une requête que
//! l'analyseur ne comprend pas, au milieu d'autres qu'il comprend : cette
//! requête est sautée, les autres sont analysées.

use sqlparser::ast::Statement;
use sqlparser::parser::Parser;
use sqlparser::tokenizer::Token;

use crate::Dialect;

/// Analyse les requêtes du texte une à une ; une requête incomprise est
/// sautée jusqu'au `;` suivant.
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
        match parser.parse_statement() {
            Ok(statement) => statements.push(statement),
            Err(_) => loop {
                let token = parser.next_token().token;
                if token == Token::EOF || token == Token::SemiColon {
                    break;
                }
            },
        }
    }
}
