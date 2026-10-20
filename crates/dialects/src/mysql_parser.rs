//! `sqlparser` dialect for MySQL and MariaDB: that of `sqlparser`, plus the
//! compound time units.
//!
//! `sqlparser` (0.63) knows the simple units (`DAY`, `MONTH`...), not the
//! compound units of MySQL and MariaDB (`YEAR_MONTH`, `DAY_HOUR`...): it
//! fails on `EXTRACT(YEAR_MONTH FROM d)` and `INTERVAL '1-2' YEAR_MONTH`, and
//! Prisme then refuses the query, as nothing proves it is a read. This dialect
//! wraps `MySqlDialect`, as `sqlparser` allows: it passes for it (`dialect()`),
//! hands it everything it changes, and parses these two constructs itself,
//! with the unit as written (`DateTimeField::Custom`).
//!
//! The delegated methods are those `MySqlDialect` overrides in `sqlparser`
//! 0.63 (`src/dialect/mysql.rs`); the version is pinned in `Cargo.toml`. On
//! an upgrade, compare that list, or drop this dialect if `sqlparser` knows
//! the compound units: reported upstream in
//! <https://github.com/apache/datafusion-sqlparser-rs/issues/2693>.

use std::any::TypeId;

use sqlparser::ast::{DateTimeField, Expr, ExtractSyntax, Ident, Interval, Statement};
use sqlparser::dialect::{Dialect, MySqlDialect};
use sqlparser::keywords::Keyword;
use sqlparser::parser::{Parser, ParserError};
use sqlparser::tokenizer::Token;

/// Compound time units of `EXTRACT` and `INTERVAL`, the same in MySQL and
/// MariaDB.
const COMPOUND_UNITS: [&str; 11] = [
    "SECOND_MICROSECOND",
    "MINUTE_MICROSECOND",
    "MINUTE_SECOND",
    "HOUR_MICROSECOND",
    "HOUR_SECOND",
    "HOUR_MINUTE",
    "DAY_MICROSECOND",
    "DAY_SECOND",
    "DAY_MINUTE",
    "DAY_HOUR",
    "YEAR_MONTH",
];

/// MySQL dialect of `sqlparser`, with the compound time units.
#[derive(Debug, Default)]
pub(crate) struct MySqlParserDialect(MySqlDialect);

impl Dialect for MySqlParserDialect {
    fn dialect(&self) -> TypeId {
        self.0.dialect()
    }

    fn parse_prefix(&self, parser: &mut Parser) -> Option<Result<Expr, ParserError>> {
        match &parser.peek_token_ref().token {
            Token::Word(word) if word.keyword == Keyword::EXTRACT => parse_extract(parser),
            Token::Word(word) if word.keyword == Keyword::INTERVAL => parse_interval(parser),
            _ => self.0.parse_prefix(parser),
        }
    }

    fn is_identifier_start(&self, ch: char) -> bool {
        self.0.is_identifier_start(ch)
    }

    fn is_identifier_part(&self, ch: char) -> bool {
        self.0.is_identifier_part(ch)
    }

    fn is_delimited_identifier_start(&self, ch: char) -> bool {
        self.0.is_delimited_identifier_start(ch)
    }

    fn identifier_quote_style(&self, identifier: &str) -> Option<char> {
        self.0.identifier_quote_style(identifier)
    }

    fn supports_string_literal_backslash_escape(&self) -> bool {
        self.0.supports_string_literal_backslash_escape()
    }

    fn supports_string_literal_concatenation(&self) -> bool {
        self.0.supports_string_literal_concatenation()
    }

    fn ignores_wildcard_escapes(&self) -> bool {
        self.0.ignores_wildcard_escapes()
    }

    fn supports_numeric_prefix(&self) -> bool {
        self.0.supports_numeric_prefix()
    }

    fn supports_bitwise_shift_operators(&self) -> bool {
        self.0.supports_bitwise_shift_operators()
    }

    fn supports_multiline_comment_hints(&self) -> bool {
        self.0.supports_multiline_comment_hints()
    }

    fn parse_infix(
        &self,
        parser: &mut Parser,
        expr: &Expr,
        precedence: u8,
    ) -> Option<Result<Expr, ParserError>> {
        self.0.parse_infix(parser, expr, precedence)
    }

    fn parse_statement(&self, parser: &mut Parser) -> Option<Result<Statement, ParserError>> {
        self.0.parse_statement(parser)
    }

    fn require_interval_qualifier(&self) -> bool {
        self.0.require_interval_qualifier()
    }

    fn supports_limit_comma(&self) -> bool {
        self.0.supports_limit_comma()
    }

    fn supports_create_table_select(&self) -> bool {
        self.0.supports_create_table_select()
    }

    fn supports_insert_set(&self) -> bool {
        self.0.supports_insert_set()
    }

    fn supports_user_host_grantee(&self) -> bool {
        self.0.supports_user_host_grantee()
    }

    fn is_table_factor_alias(&self, explicit: bool, kw: &Keyword, parser: &mut Parser) -> bool {
        self.0.is_table_factor_alias(explicit, kw, parser)
    }

    fn supports_table_hints(&self) -> bool {
        self.0.supports_table_hints()
    }

    fn requires_single_line_comment_whitespace(&self) -> bool {
        self.0.requires_single_line_comment_whitespace()
    }

    fn supports_match_against(&self) -> bool {
        self.0.supports_match_against()
    }

    fn supports_select_modifiers(&self) -> bool {
        self.0.supports_select_modifiers()
    }

    fn supports_set_names(&self) -> bool {
        self.0.supports_set_names()
    }

    fn supports_comma_separated_set_assignments(&self) -> bool {
        self.0.supports_comma_separated_set_assignments()
    }

    fn supports_update_order_by(&self) -> bool {
        self.0.supports_update_order_by()
    }

    fn supports_data_type_signed_suffix(&self) -> bool {
        self.0.supports_data_type_signed_suffix()
    }

    fn supports_cross_join_constraint(&self) -> bool {
        self.0.supports_cross_join_constraint()
    }

    fn supports_double_ampersand_operator(&self) -> bool {
        self.0.supports_double_ampersand_operator()
    }

    fn supports_binary_kw_as_cast(&self) -> bool {
        self.0.supports_binary_kw_as_cast()
    }

    fn supports_comment_optimizer_hint(&self) -> bool {
        self.0.supports_comment_optimizer_hint()
    }

    fn supports_constraint_keyword_without_name(&self) -> bool {
        self.0.supports_constraint_keyword_without_name()
    }

    fn supports_key_column_option(&self) -> bool {
        self.0.supports_key_column_option()
    }

    fn supports_group_by_with_modifier(&self) -> bool {
        self.0.supports_group_by_with_modifier()
    }

    fn supports_left_associative_joins_without_parens(&self) -> bool {
        self.0.supports_left_associative_joins_without_parens()
    }
}

/// Compound unit written by the token, if it is one (in any case).
///
/// The identifier has no position: the analysis of the query
/// (`analysis.rs`) then does not take the unit for a name.
fn compound_unit(token: &Token) -> Option<Ident> {
    match token {
        Token::Word(word)
            if word.quote_style.is_none()
                && COMPOUND_UNITS
                    .iter()
                    .any(|unit| word.value.eq_ignore_ascii_case(unit)) =>
        {
            Some(Ident::new(word.value.clone()))
        }
        _ => None,
    }
}

/// `EXTRACT(<compound unit> FROM <expression>)`; `None` for another
/// `EXTRACT`, left to `sqlparser`.
fn parse_extract(parser: &mut Parser) -> Option<Result<Expr, ParserError>> {
    if parser.peek_nth_token_ref(1).token != Token::LParen {
        return None;
    }
    let field = compound_unit(&parser.peek_nth_token_ref(2).token)?;
    if !matches!(&parser.peek_nth_token_ref(3).token, Token::Word(word) if word.keyword == Keyword::FROM)
    {
        return None;
    }
    for _ in 0..4 {
        parser.advance_token();
    }
    Some(parser.parse_expr().and_then(|expr| {
        parser.expect_token(&Token::RParen)?;
        Ok(Expr::Extract {
            field: DateTimeField::Custom(field),
            syntax: ExtractSyntax::From,
            expr: Box::new(expr),
        })
    }))
}

/// `INTERVAL <expression> <compound unit>`; `None` for another `INTERVAL`,
/// left to `sqlparser`.
///
/// The value is an expression, as in `sqlparser` for MySQL
/// (`INTERVAL 1 + 1 DAY`): the unit is only known after it. Without a
/// compound unit there, the parser goes back to `INTERVAL`, and `sqlparser`
/// parses it again.
fn parse_interval(parser: &mut Parser) -> Option<Result<Expr, ParserError>> {
    parser.advance_token();
    let keyword = parser.get_current_index();
    if let Ok(value) = parser.parse_expr()
        && let Some(field) = compound_unit(&parser.peek_token_ref().token)
    {
        parser.advance_token();
        return Some(Ok(Expr::Interval(Interval {
            value: Box::new(value),
            leading_field: Some(DateTimeField::Custom(field)),
            leading_precision: None,
            last_field: None,
            fractional_seconds_precision: None,
        })));
    }
    // `prev_token` stops on each token that is not a blank: back to
    // `INTERVAL`, never before it.
    while parser.index() > keyword {
        parser.prev_token();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parses with the dialect of Prisme.
    fn parse(sql: &str) -> Result<Vec<Statement>, ParserError> {
        Parser::parse_sql(&MySqlParserDialect::default(), sql)
    }

    /// Parses with the dialect of Prisme and prints the tree back.
    fn printed(sql: &str) -> String {
        parse(sql).unwrap()[0].to_string()
    }

    #[test]
    fn parses_extract_with_a_compound_unit() {
        assert_eq!(
            printed("select extract(year_month from operation_date) from accounts.operations"),
            "SELECT EXTRACT(year_month FROM operation_date) FROM accounts.operations"
        );
        for unit in COMPOUND_UNITS {
            assert!(
                parse(&format!("SELECT EXTRACT({unit} FROM d)")).is_ok(),
                "{unit}"
            );
        }
    }

    #[test]
    fn parses_interval_with_a_compound_unit() {
        assert_eq!(
            printed("SELECT DATE_ADD(d, INTERVAL '1-2' YEAR_MONTH), d + INTERVAL 1 + 1 DAY_HOUR"),
            "SELECT DATE_ADD(d, INTERVAL '1-2' YEAR_MONTH), d + INTERVAL 1 + 1 DAY_HOUR"
        );
        assert_eq!(
            printed("SELECT d - INTERVAL -5 MINUTE_SECOND, INTERVAL x HOUR_MINUTE + d"),
            "SELECT d - INTERVAL -5 MINUTE_SECOND, INTERVAL x HOUR_MINUTE + d"
        );
    }

    #[test]
    fn keeps_refusing_an_unknown_unit() {
        assert!(parse("SELECT EXTRACT(MONTH_YEAR FROM d)").is_err());
        assert!(parse("SELECT INTERVAL 1 MONTH_YEAR").is_err());
        assert!(parse("SELECT EXTRACT(`YEAR_MONTH` FROM d)").is_err());
    }

    #[test]
    fn parses_everything_else_as_the_mysql_dialect() {
        // A construct per method handed to `MySqlDialect`, and the simple
        // units, which go back to `sqlparser` (whitespace included).
        for sql in [
            "SELECT `a b`, 7 DIV 2, 1 << 2, a && b, 'it\\'s' 'x' FROM t LIMIT 1, 2",
            "SELECT HIGH_PRIORITY SQL_NO_CACHE * FROM t USE INDEX (i) WHERE MATCH (a) AGAINST ('x')",
            "SELECT /*+ BKA(t) */ CAST(a AS SIGNED INTEGER), BINARY a FROM t1 CROSS JOIN t2 ON t1.a = t2.a",
            "SELECT a, COUNT(*) FROM t GROUP BY a WITH ROLLUP",
            "SELECT EXTRACT(DAY FROM d), d +  INTERVAL   1   DAY, INTERVAL '1' MONTH + d FROM t",
            "SELECT DATE_ADD(d, INTERVAL 1 + 1 DAY), TIMESTAMPDIFF(MONTH, a, b) FROM t AS x",
            "SET NAMES utf8mb4",
            "SET @a = 1, @b = 2",
            "LOCK TABLES t READ",
            "UNLOCK TABLES",
            "UPDATE t SET a = 1 ORDER BY b LIMIT 2",
            "INSERT INTO t SET a = 1",
            "CREATE TABLE t (a INT, CONSTRAINT UNIQUE KEY (a)) AS SELECT 1",
            "GRANT SELECT ON db.* TO 'user'@'host'",
            "SELECT 1 -- comment\nFROM t",
        ] {
            let expected = Parser::parse_sql(&MySqlDialect {}, sql);
            assert_eq!(parse(sql), expected, "{sql}");
            assert!(expected.is_ok(), "{sql}");
        }
    }
}
