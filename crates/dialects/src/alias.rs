//! Beautifier rule: `AS` before each column or table alias.
//!
//! `SELECT count(*) n FROM video v` becomes `SELECT count(*) AS n FROM video
//! AS v`. The meaning does not change: in MySQL and MariaDB, `AS` is optional
//! before an alias, and it makes reading safe (`SELECT a b` looks like a
//! forgotten comma).
//!
//! Aliases are found by `sqlparser`'s parser, not by the text: it knows that
//! `b` is an alias in `SELECT a b`, but not `DAY` in `INTERVAL 1 DAY`. Its
//! position in the original text (`Ident::span`) tells where to insert `AS`.
//! In a part the parser does not understand, nothing is inserted; parsing
//! then resumes (`parse` module).

use std::collections::HashMap;
use std::ops::ControlFlow;

use sqlparser::ast::{SelectItem, TableAlias, TableFactor, Visit, Visitor};
use sqlparser::keywords::Keyword;

use sqlparser::tokenizer::{Location, Token, Tokenizer};

use crate::Dialect;
use crate::offsets::OffsetIndex;
use crate::parse::parse_each;

/// Adds `AS` before the aliases that lack it. See the module header.
pub(crate) fn add_alias_keyword(sql: &str, dialect: Dialect) -> String {
    let parser_dialect = dialect.parser_dialect();
    let Ok(tokens) = Tokenizer::new(parser_dialect.as_ref(), sql).tokenize_with_location() else {
        return sql.to_string();
    };

    let mut finder = AliasFinder::default();
    for statement in parse_each(sql, dialect) {
        let _ = statement.visit(&mut finder);
    }

    let index = OffsetIndex::new(sql);
    let positions: HashMap<Location, usize> = tokens
        .iter()
        .enumerate()
        .map(|(position, token)| (token.span.start, position))
        .collect();
    // Insertions from the end to the start: positions stay correct.
    let mut offsets: Vec<usize> = finder
        .starts
        .into_iter()
        .filter(|start| !preceded_by_as(&tokens, &positions, *start))
        .filter_map(|start| index.offset(start))
        .collect();
    offsets.sort_unstable();
    offsets.dedup();
    let mut result = sql.to_string();
    for offset in offsets.into_iter().rev() {
        result.insert_str(offset, "AS ");
    }
    result
}

/// Is the token before the one starting at `start`, whitespace and comments
/// skipped, `AS`? An alias not found among the tokens counts as preceded by
/// `AS`: nothing is inserted at a doubtful position.
fn preceded_by_as(
    tokens: &[sqlparser::tokenizer::TokenWithSpan],
    positions: &HashMap<Location, usize>,
    start: Location,
) -> bool {
    let Some(&index) = positions.get(&start) else {
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

/// Collects the position of each column or table alias.
#[derive(Default)]
struct AliasFinder {
    /// Start of each alias in the original text.
    starts: Vec<Location>,
}

impl AliasFinder {
    /// Collects a table alias that was not written with `AS`.
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
        // No alias: nothing changes.
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
            with_as("SELECT a /* note */ `my alias` FROM t"),
            "SELECT a /* note */ AS `my alias` FROM t"
        );
    }

    #[test]
    fn handles_the_statements_of_a_procedure_body() {
        assert_eq!(
            with_as(
                "CREATE PROCEDURE p() BEGIN SELECT a b FROM t; IF x THEN SELECT c d FROM u; END IF; END"
            ),
            "CREATE PROCEDURE p() BEGIN SELECT a AS b FROM t; IF x THEN SELECT c AS d FROM u; END IF; END"
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
