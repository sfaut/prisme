//! Règle de l'enjoliveur : `AS` devant chaque alias de colonne ou de table.
//!
//! `SELECT count(*) n FROM video v` devient `SELECT count(*) AS n FROM video
//! AS v`. Le sens ne change pas : en MySQL et MariaDB, `AS` est facultatif
//! devant un alias, et il rend la lecture sûre (`SELECT a b` ressemble à
//! un oubli de virgule).
//!
//! Les alias sont trouvés par l'analyseur de `sqlparser`, pas par le texte :
//! c'est lui qui sait que `b` est un alias dans `SELECT a b`, mais pas
//! `DAY` dans `INTERVAL 1 DAY`. Sa position dans le texte d'origine
//! (`Ident::span`) dit où insérer `AS`. Une requête que l'analyseur ne
//! comprend pas reste telle quelle ; les autres requêtes du texte sont
//! traitées.

use std::ops::ControlFlow;

use sqlparser::ast::{SelectItem, Statement, TableAlias, TableFactor, Visit, Visitor};
use sqlparser::keywords::Keyword;
use sqlparser::parser::Parser;
use sqlparser::tokenizer::{Location, Token, Tokenizer};

use crate::Dialect;
use crate::limit::byte_offset;

/// Ajoute `AS` devant les alias qui n'en ont pas. Voir l'en-tête du module.
pub(crate) fn add_alias_keyword(sql: &str, dialect: Dialect) -> String {
    let parser_dialect = dialect.parser_dialect();
    let Ok(tokens) = Tokenizer::new(parser_dialect.as_ref(), sql).tokenize_with_location() else {
        return sql.to_string();
    };

    let mut finder = AliasFinder::default();
    for statement in parse_each(sql, dialect) {
        let _ = statement.visit(&mut finder);
    }

    // Insertions de la fin vers le début : les positions restent justes.
    let mut offsets: Vec<usize> = finder
        .starts
        .into_iter()
        .filter(|start| !preceded_by_as(&tokens, *start))
        .filter_map(|start| byte_offset(sql, start))
        .collect();
    offsets.sort_unstable();
    offsets.dedup();
    let mut result = sql.to_string();
    for offset in offsets.into_iter().rev() {
        result.insert_str(offset, "AS ");
    }
    result
}

/// Analyse les requêtes du texte une à une ; une requête incomprise est
/// sautée jusqu'au `;` suivant.
fn parse_each(sql: &str, dialect: Dialect) -> Vec<Statement> {
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

/// Le jeton qui précède celui commençant à `start`, blancs et commentaires
/// sautés, est-il `AS` ? Un alias introuvable parmi les jetons compte comme
/// précédé de `AS` : rien n'est inséré à une position douteuse.
fn preceded_by_as(tokens: &[sqlparser::tokenizer::TokenWithSpan], start: Location) -> bool {
    let Some(index) = tokens.iter().position(|token| token.span.start == start) else {
        return true;
    };
    tokens[..index]
        .iter()
        .rev()
        .find(|token| !matches!(token.token, Token::Whitespace(_)))
        .is_some_and(
            |token| matches!(&token.token, Token::Word(word) if word.keyword == Keyword::AS),
        )
}

/// Relève la position de chaque alias de colonne ou de table.
#[derive(Default)]
struct AliasFinder {
    /// Début de chaque alias dans le texte d'origine.
    starts: Vec<Location>,
}

impl AliasFinder {
    /// Relève un alias de table qui n'a pas été écrit avec `AS`.
    fn table_alias(&mut self, alias: Option<&TableAlias>) {
        if let Some(alias) = alias.filter(|alias| !alias.explicit) {
            self.starts.push(alias.name.span.start);
        }
    }
}

impl Visitor for AliasFinder {
    type Break = ();

    fn pre_visit_select(&mut self, select: &sqlparser::ast::Select) -> ControlFlow<()> {
        for item in &select.projection {
            if let SelectItem::ExprWithAlias { alias, .. } = item {
                self.starts.push(alias.span.start);
            }
        }
        ControlFlow::Continue(())
    }

    fn pre_visit_table_factor(&mut self, factor: &TableFactor) -> ControlFlow<()> {
        let alias = match factor {
            TableFactor::Table { alias, .. }
            | TableFactor::Derived { alias, .. }
            | TableFactor::TableFunction { alias, .. }
            | TableFactor::Function { alias, .. }
            | TableFactor::JsonTable { alias, .. }
            | TableFactor::NestedJoin { alias, .. } => alias.as_ref(),
            _ => None,
        };
        self.table_alias(alias);
        ControlFlow::Continue(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_as(sql: &str) -> String {
        add_alias_keyword(sql, Dialect::MySql)
    }

    #[test]
    fn adds_as_before_column_and_table_aliases() {
        assert_eq!(
            with_as("SELECT count(*) n, v.id ident FROM video v JOIN person p ON p.id = v.pid"),
            "SELECT count(*) AS n, v.id AS ident FROM video AS v JOIN person AS p ON p.id = v.pid"
        );
    }

    #[test]
    fn keeps_existing_as_and_non_aliases() {
        let sql = "SELECT a AS b, now() - INTERVAL 1 DAY FROM t AS x WHERE c = 1";
        assert_eq!(with_as(sql), sql);
        // Aucun alias : rien ne change.
        assert_eq!(with_as("SELECT a, b FROM t"), "SELECT a, b FROM t");
    }

    #[test]
    fn handles_subqueries_and_derived_tables() {
        assert_eq!(
            with_as("SELECT d.n FROM (SELECT count(*) n FROM t) d WHERE x IN (SELECT y z FROM u)"),
            "SELECT d.n FROM (SELECT count(*) AS n FROM t) AS d WHERE x IN (SELECT y AS z FROM u)"
        );
    }

    #[test]
    fn keeps_comments_and_quoted_aliases() {
        assert_eq!(
            with_as("SELECT a /* note */ `mon alias` FROM t"),
            "SELECT a /* note */ AS `mon alias` FROM t"
        );
    }

    #[test]
    fn skips_statements_it_cannot_parse() {
        assert_eq!(
            with_as("SELECT a b FROM t; FOO BAR; SELECT c d FROM u"),
            "SELECT a AS b FROM t; FOO BAR; SELECT c AS d FROM u"
        );
    }
}
