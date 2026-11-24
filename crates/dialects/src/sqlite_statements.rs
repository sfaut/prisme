//! SQLite statements that `sqlparser` (0.64) does not parse, or misreads:
//! `PRAGMA` (its value may be a name, `PRAGMA journal_mode = WAL`, or the
//! argument of a pragma that reads, `PRAGMA table_info(t)`), `DETACH`,
//! `VACUUM` (with `INTO`) and `REINDEX`; and `REPLACE`, which it only
//! parses for MySQL, read as the `INSERT OR REPLACE` it stands for.
//!
//! Prisme refuses a query it cannot parse, as nothing proves what it does:
//! these statements need their own tree for the write switches (see
//! `docs/V1.md`, section 2.4). They are parsed with the primitives of
//! `sqlparser` (names, expressions), on its tokens, as the grammar of SQLite
//! writes them (<https://sqlite.org/lang.html>).
//!
//! Workaround: `sqlparser` does not parse these statements (`docs/DEVELOPER.md`, section "Workarounds").

use sqlparser::ast::{Insert, SqliteOnConflict, Statement};
use sqlparser::parser::{Parser, ParserError};
use sqlparser::tokenizer::Token;

use crate::words::{expect_end, expected, parse_word, word_is};

/// Statement of SQLite that `sqlparser` does not parse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SqliteStatement {
    /// `PRAGMA [schema.]name [= value | (value)]`.
    Pragma {
        /// Name of the pragma, in lowercase, without its schema.
        name: String,
        /// How the value is written.
        form: PragmaForm,
    },
    /// `DETACH [DATABASE] schema`.
    Detach,
    /// `VACUUM [schema] [INTO file]`.
    Vacuum {
        /// `INTO`: writes a copy of the database into a new file.
        into: bool,
    },
    /// `REINDEX [collation | [schema.]table | [schema.]index]`.
    Reindex,
}

/// How the value of a pragma is written. SQLite reads `name = value` and
/// `name(value)` alike for a pragma that changes a setting; the parentheses
/// give the argument of a pragma that reads (`table_info(t)`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PragmaForm {
    /// Without a value: `PRAGMA journal_mode`.
    Bare,
    /// `PRAGMA name = value`.
    Equals,
    /// `PRAGMA name(value)`.
    Parentheses,
}

/// Parses a SQLite statement of this module at the position of the parser:
/// `None` (nothing consumed) if the statement starts with none of their
/// verbs.
pub(crate) fn parse_sqlite_statement(
    parser: &mut Parser,
) -> Option<Result<SqliteStatement, ParserError>> {
    let parse: fn(&mut Parser) -> Result<SqliteStatement, ParserError> =
        if word_is(parser, 0, "PRAGMA") {
            pragma
        } else if word_is(parser, 0, "DETACH") {
            detach
        } else if word_is(parser, 0, "VACUUM") {
            vacuum
        } else if word_is(parser, 0, "REINDEX") {
            reindex
        } else {
            return None;
        };
    parser.advance_token();
    Some(parse(parser).and_then(|statement| {
        expect_end(parser)?;
        Ok(statement)
    }))
}

/// Parses a `REPLACE` of SQLite at the position of the parser, as the
/// `INSERT OR REPLACE` it stands for: `None` (nothing consumed) if the
/// statement does not start with `REPLACE`.
pub(crate) fn parse_replace(parser: &mut Parser) -> Option<Result<Statement, ParserError>> {
    if !word_is(parser, 0, "REPLACE") {
        return None;
    }
    let replace = parser.next_token();
    Some(parser.parse_insert(replace).map(|mut statement| {
        if let Statement::Insert(Insert { or, .. }) = &mut statement {
            *or = Some(SqliteOnConflict::Replace);
        }
        statement
    }))
}

/// `PRAGMA` after its keyword.
fn pragma(parser: &mut Parser) -> Result<SqliteStatement, ParserError> {
    let name = parser.parse_object_name(false)?;
    let Some(last) = name.0.last().and_then(|part| part.as_ident()) else {
        return expected(parser, "a pragma name");
    };
    let name = last.value.to_ascii_lowercase();
    let form = if parser.consume_token(&Token::Eq) {
        pragma_value(parser)?;
        PragmaForm::Equals
    } else if parser.consume_token(&Token::LParen) {
        pragma_value(parser)?;
        parser.expect_token(&Token::RParen)?;
        PragmaForm::Parentheses
    } else {
        PragmaForm::Bare
    };
    Ok(SqliteStatement::Pragma { name, form })
}

/// Value of a pragma: a number with its sign (decimal or hexadecimal), a
/// name, a keyword (`ON`, `WAL`) or a string.
fn pragma_value(parser: &mut Parser) -> Result<(), ParserError> {
    let signed = matches!(parser.peek_token_ref().token, Token::Plus | Token::Minus);
    if signed {
        parser.advance_token();
    }
    match parser.peek_token_ref().token {
        // `0x10002` is read as a hexadecimal literal.
        Token::Number(..) | Token::HexStringLiteral(_) => {}
        Token::Word(_) | Token::SingleQuotedString(_) | Token::DoubleQuotedString(_) if !signed => {
        }
        _ => return expected(parser, "a pragma value"),
    }
    parser.advance_token();
    Ok(())
}

/// `DETACH` after its keyword: `[DATABASE] schema`, an expression for SQLite.
fn detach(parser: &mut Parser) -> Result<SqliteStatement, ParserError> {
    parse_word(parser, "DATABASE");
    parser.parse_expr()?;
    Ok(SqliteStatement::Detach)
}

/// `VACUUM` after its keyword: `[schema] [INTO file]`.
fn vacuum(parser: &mut Parser) -> Result<SqliteStatement, ParserError> {
    if matches!(parser.peek_token_ref().token, Token::Word(_)) && !word_is(parser, 0, "INTO") {
        parser.parse_identifier()?;
    }
    let into = parse_word(parser, "INTO");
    if into {
        parser.parse_expr()?;
    }
    Ok(SqliteStatement::Vacuum { into })
}

/// `REINDEX` after its keyword: `[name]`, a collation, a table or an index.
fn reindex(parser: &mut Parser) -> Result<SqliteStatement, ParserError> {
    if !matches!(parser.peek_token_ref().token, Token::SemiColon | Token::EOF) {
        parser.parse_object_name(false)?;
    }
    Ok(SqliteStatement::Reindex)
}

#[cfg(test)]
mod tests {
    use sqlparser::dialect::SQLiteDialect;

    use super::*;

    /// Parses `sql` as a statement of this module.
    fn parse(sql: &str) -> Option<Result<SqliteStatement, ParserError>> {
        let dialect = SQLiteDialect {};
        let mut parser = Parser::new(&dialect).try_with_sql(sql).unwrap();
        parse_sqlite_statement(&mut parser)
    }

    /// Parses `sql`, which must be understood.
    fn parsed(sql: &str) -> SqliteStatement {
        parse(sql).unwrap().unwrap()
    }

    /// Pragma of `sql`: its name and its form.
    fn pragma_of(sql: &str) -> (String, PragmaForm) {
        let SqliteStatement::Pragma { name, form } = parsed(sql) else {
            panic!("not a pragma: {sql}");
        };
        (name, form)
    }

    #[test]
    fn reads_the_three_forms_of_a_pragma() {
        let pragma = |name: &str, form| (name.to_string(), form);
        assert_eq!(
            pragma_of("PRAGMA journal_mode"),
            pragma("journal_mode", PragmaForm::Bare)
        );
        assert_eq!(
            pragma_of("pragma Main.Journal_Mode = WAL"),
            pragma("journal_mode", PragmaForm::Equals)
        );
        assert_eq!(
            pragma_of("PRAGMA optimize(-0x10002)"),
            pragma("optimize", PragmaForm::Parentheses)
        );
        assert_eq!(
            pragma_of("PRAGMA cache_size = -2000"),
            pragma("cache_size", PragmaForm::Equals)
        );
        assert_eq!(
            pragma_of("PRAGMA foreign_keys = ON"),
            pragma("foreign_keys", PragmaForm::Equals)
        );
        assert_eq!(
            pragma_of("PRAGMA table_info('t')"),
            pragma("table_info", PragmaForm::Parentheses)
        );
        assert_eq!(
            pragma_of("PRAGMA aux.index_list(orders);"),
            pragma("index_list", PragmaForm::Parentheses)
        );
    }

    #[test]
    fn refuses_a_malformed_pragma() {
        assert!(parse("PRAGMA").unwrap().is_err());
        assert!(parse("PRAGMA a = ").unwrap().is_err());
        assert!(parse("PRAGMA a = - b").unwrap().is_err());
        assert!(parse("PRAGMA table_info(t").unwrap().is_err());
        assert!(parse("PRAGMA a = 1 2").unwrap().is_err());
    }

    #[test]
    fn reads_detach_vacuum_and_reindex() {
        assert_eq!(parsed("DETACH aux"), SqliteStatement::Detach);
        assert_eq!(parsed("DETACH DATABASE 'aux'"), SqliteStatement::Detach);
        assert_eq!(parsed("VACUUM"), SqliteStatement::Vacuum { into: false });
        assert_eq!(
            parsed("VACUUM main"),
            SqliteStatement::Vacuum { into: false }
        );
        assert_eq!(
            parsed("VACUUM INTO 'copy.db'"),
            SqliteStatement::Vacuum { into: true }
        );
        assert_eq!(
            parsed("vacuum aux into 'c:/copy.db'"),
            SqliteStatement::Vacuum { into: true }
        );
        assert_eq!(parsed("REINDEX"), SqliteStatement::Reindex);
        assert_eq!(parsed("REINDEX main.orders"), SqliteStatement::Reindex);
        assert!(parse("VACUUM INTO").unwrap().is_err());
        assert!(parse("DETACH").unwrap().is_err());
    }

    #[test]
    fn leaves_the_other_statements_to_sqlparser() {
        assert!(parse("SELECT 1").is_none());
        assert!(parse("ATTACH 'a.db' AS a").is_none());
        assert!(parse("ANALYZE").is_none());
    }
}
