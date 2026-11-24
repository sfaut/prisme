//! Table options of `ALTER TABLE` in MySQL and MariaDB (`ENGINE = InnoDB`,
//! `COMMENT = '...'`, `ROW_FORMAT = DYNAMIC`...), which `sqlparser` (0.64)
//! does not parse there, apart from `AUTO_INCREMENT`.
//!
//! The options differ between the two engines, as their grammars give them
//! and as measured on their development servers (MySQL 8.0.11, 9.7.4,
//! 26.10.0; MariaDB 10.6.3, 12.3.3, 13.0.2; `docs/SQL-COMPARISON.md`,
//! section 3.17):
//!
//! - MySQL knows a closed list: any other name is a syntax error.
//! - MariaDB knows its own list, and reads any other `name [=] value` as an
//!   option defined by a storage engine (`TABLE_TYPE = CSV` of `CONNECT`):
//!   the server, not its grammar, refuses an option its engine does not
//!   know ("Unknown option", a warning under `IGNORE_BAD_TABLE_OPTIONS`).
//!
//! The options follow each other with or without a comma
//! (`table_option [[,] table_option] ...`). An option known to the version
//! of the server only (`AUTOEXTEND_SIZE`, MySQL 8.0.23) is parsed whatever the
//! version: the server refuses it if it is too old.
//!
//! Sources: <https://dev.mysql.com/doc/refman/8.4/en/alter-table.html>,
//! <https://dev.mysql.com/doc/refman/8.4/en/create-table.html>,
//! <https://mariadb.com/kb/en/create-table/#table-options>,
//! <https://mariadb.com/kb/en/alter-table/>.
//!
//! Workaround: `sqlparser` does not parse these options (`docs/DEVELOPER.md`,
//! section "Workarounds").

use sqlparser::parser::{Parser, ParserError};
use sqlparser::tokenizer::Token;

use crate::Dialect;
use crate::words::{expected, parse_word, parse_words, word_is};

/// Form of the value of a table option.
#[derive(Debug, Clone, Copy)]
enum Value {
    /// A number, or a size with its unit (`4M`).
    Number,
    /// A number or `DEFAULT`.
    NumberOrDefault,
    /// A string between quotes.
    Text,
    /// A name, written bare or between quotes (`InnoDB`, `'utf8mb4'`).
    Name,
    /// One of these words or numbers, bare (`DYNAMIC`, `0`).
    Choice(&'static [&'static str]),
    /// One of these strings, between quotes (`'ZLIB'`).
    Quoted(&'static [&'static str]),
    /// A list of tables between parentheses (`UNION = (t1, t2)`).
    Tables,
}

/// `0` or `1`.
const BOOLEAN: Value = Value::Choice(&["0", "1"]);

/// `DEFAULT`, `0` or `1`.
const DEFAULT_BOOLEAN: Value = Value::Choice(&["DEFAULT", "0", "1"]);

/// Table options of MySQL written as a single word, then `[=] value`.
const MYSQL_OPTIONS: &[(&str, Value)] = &[
    ("AUTOEXTEND_SIZE", Value::Number),
    ("AUTO_INCREMENT", Value::Number),
    ("AVG_ROW_LENGTH", Value::Number),
    ("CHECKSUM", BOOLEAN),
    ("TABLE_CHECKSUM", BOOLEAN),
    ("COMMENT", Value::Text),
    ("COMPRESSION", Value::Quoted(&["ZLIB", "LZ4", "NONE"])),
    ("CONNECTION", Value::Text),
    ("DELAY_KEY_WRITE", BOOLEAN),
    ("ENCRYPTION", Value::Quoted(&["Y", "N"])),
    ("ENGINE", Value::Name),
    ("ENGINE_ATTRIBUTE", Value::Text),
    ("INSERT_METHOD", Value::Choice(&["NO", "FIRST", "LAST"])),
    ("KEY_BLOCK_SIZE", Value::Number),
    ("MAX_ROWS", Value::Number),
    ("MIN_ROWS", Value::Number),
    ("PACK_KEYS", DEFAULT_BOOLEAN),
    ("PASSWORD", Value::Text),
    (
        "ROW_FORMAT",
        Value::Choice(&[
            "DEFAULT",
            "DYNAMIC",
            "FIXED",
            "COMPRESSED",
            "REDUNDANT",
            "COMPACT",
        ]),
    ),
    ("SECONDARY_ENGINE_ATTRIBUTE", Value::Text),
    ("STATS_AUTO_RECALC", DEFAULT_BOOLEAN),
    ("STATS_PERSISTENT", DEFAULT_BOOLEAN),
    ("STATS_SAMPLE_PAGES", Value::NumberOrDefault),
    ("UNION", Value::Tables),
];

/// Table options of MariaDB written as a single word, then `[=] value`.
const MARIADB_OPTIONS: &[(&str, Value)] = &[
    (
        "ADAPTIVE_HASH_INDEX",
        Value::Choice(&["DEFAULT", "YES", "NO"]),
    ),
    ("AUTO_INCREMENT", Value::Number),
    ("AVG_ROW_LENGTH", Value::Number),
    ("CHECKSUM", BOOLEAN),
    ("TABLE_CHECKSUM", BOOLEAN),
    ("COMMENT", Value::Text),
    ("CONNECTION", Value::Text),
    ("DELAY_KEY_WRITE", BOOLEAN),
    ("ENCRYPTED", Value::Choice(&["YES", "NO"])),
    ("ENCRYPTION_KEY_ID", Value::Number),
    ("ENGINE", Value::Name),
    ("IETF_QUOTES", Value::Choice(&["YES", "NO"])),
    ("INSERT_METHOD", Value::Choice(&["NO", "FIRST", "LAST"])),
    ("KEY_BLOCK_SIZE", Value::Number),
    ("MAX_ROWS", Value::Number),
    ("MIN_ROWS", Value::Number),
    ("PACK_KEYS", DEFAULT_BOOLEAN),
    ("PAGE_CHECKSUM", BOOLEAN),
    ("PAGE_COMPRESSED", BOOLEAN),
    ("PAGE_COMPRESSION_LEVEL", Value::Number),
    ("PASSWORD", Value::Text),
    (
        "ROW_FORMAT",
        Value::Choice(&[
            "DEFAULT",
            "DYNAMIC",
            "FIXED",
            "COMPRESSED",
            "REDUNDANT",
            "COMPACT",
            "PAGE",
        ]),
    ),
    ("SEQUENCE", BOOLEAN),
    ("STATS_AUTO_RECALC", DEFAULT_BOOLEAN),
    ("STATS_PERSISTENT", DEFAULT_BOOLEAN),
    ("STATS_SAMPLE_PAGES", Value::NumberOrDefault),
    ("TRANSACTIONAL", BOOLEAN),
    ("UNION", Value::Tables),
];

/// Parses the table options at the position of the parser, one or several
/// in a row separated by spaces, up to the next comma: `Ok(false)` (nothing
/// consumed) if no option starts there. The options of the storage engines
/// of MariaDB (any `name [=] value`) count only with `engine_options`: the
/// caller first gives their chance to the other operations of `ALTER TABLE`,
/// as `ADD c INT` would read as such an option.
///
/// # Errors
///
/// Returns the error of an option whose value is not one it takes.
pub(crate) fn parse_table_options(
    parser: &mut Parser,
    dialect: Dialect,
    engine_options: bool,
) -> Result<bool, ParserError> {
    if !table_option(parser, dialect, engine_options)? {
        return Ok(false);
    }
    // After a comma, the next operation may be of another kind: it is left
    // to the caller.
    while table_option(parser, dialect, true)? {}
    Ok(true)
}

/// Parses one table option, an option of a storage engine of MariaDB only
/// with `engine_options`: `Ok(false)` (nothing consumed) if none starts at
/// the position of the parser.
fn table_option(
    parser: &mut Parser,
    dialect: Dialect,
    engine_options: bool,
) -> Result<bool, ParserError> {
    let mariadb = dialect == Dialect::MariaDb;
    if charset_or_collation(parser)? {
        return Ok(true);
    }
    if parse_words(parser, &["CONVERT", "TO"]) {
        // CONVERT TO {CHARACTER SET | CHARSET} name [COLLATE name]
        if !charset_keyword(parser) {
            return expected(parser, "CHARACTER SET");
        }
        name(parser)?;
        if parse_word(parser, "COLLATE") {
            name(parser)?;
        }
        return Ok(true);
    }
    if word_is(parser, 1, "DIRECTORY")
        && (word_is(parser, 0, "DATA") || word_is(parser, 0, "INDEX"))
    {
        // {DATA | INDEX} DIRECTORY [=] 'path'
        parser.advance_token();
        parser.advance_token();
        value(parser, Value::Text)?;
        return Ok(true);
    }
    if parse_word(parser, "TABLESPACE") {
        // TABLESPACE name [STORAGE {DISK | MEMORY}] (MySQL)
        parser.parse_identifier()?;
        if !mariadb && parse_word(parser, "STORAGE") && !parse_choice(parser, &["DISK", "MEMORY"]) {
            return expected(parser, "DISK or MEMORY");
        }
        return Ok(true);
    }
    if mariadb
        && (parse_words(parser, &["WITH", "SYSTEM", "VERSIONING"])
            || parse_words(parser, &["ADD", "SYSTEM", "VERSIONING"])
            || parse_words(parser, &["DROP", "SYSTEM", "VERSIONING"]))
    {
        return Ok(true);
    }
    let options = if mariadb {
        MARIADB_OPTIONS
    } else {
        MYSQL_OPTIONS
    };
    if let Some(&(_, form)) = options.iter().find(|(name, _)| word_is(parser, 0, name)) {
        parser.advance_token();
        value(parser, form)?;
        return Ok(true);
    }
    if mariadb && engine_options {
        return engine_option(parser);
    }
    Ok(false)
}

/// `[DEFAULT] {CHARACTER SET | CHARSET} [=] name [COLLATE [=] name]` or
/// `[DEFAULT] COLLATE [=] name`: `Ok(false)` (nothing consumed) if neither
/// starts there.
fn charset_or_collation(parser: &mut Parser) -> Result<bool, ParserError> {
    let start = parser.index();
    let _ = parse_word(parser, "DEFAULT");
    if charset_keyword(parser) {
        value(parser, Value::Name)?;
        if parse_word(parser, "COLLATE") {
            value(parser, Value::Name)?;
        }
        return Ok(true);
    }
    if parse_word(parser, "COLLATE") {
        value(parser, Value::Name)?;
        return Ok(true);
    }
    while parser.index() > start {
        parser.prev_token();
    }
    Ok(false)
}

/// Consumes `CHARACTER SET` or `CHARSET` if it comes next.
fn charset_keyword(parser: &mut Parser) -> bool {
    parse_words(parser, &["CHARACTER", "SET"]) || parse_word(parser, "CHARSET")
}

/// Option defined by a storage engine of MariaDB, `name [=] value`, the
/// value a number, a string or a word: `Ok(false)` (nothing consumed) if no
/// such option starts there.
fn engine_option(parser: &mut Parser) -> Result<bool, ParserError> {
    let is_name = matches!(&parser.peek_token_ref().token,
        Token::Word(word) if word.quote_style.is_none());
    let has_value = matches!(
        parser.peek_nth_token_ref(1).token,
        Token::Eq
            | Token::Number(..)
            | Token::SingleQuotedString(_)
            | Token::DoubleQuotedString(_)
            | Token::Word(_)
    );
    if !(is_name && has_value) {
        return Ok(false);
    }
    parser.advance_token();
    let _ = parser.consume_token(&Token::Eq);
    match parser.next_token().token {
        Token::Number(..)
        | Token::SingleQuotedString(_)
        | Token::DoubleQuotedString(_)
        | Token::Word(_) => Ok(true),
        _ => {
            parser.prev_token();
            expected(parser, "a value")
        }
    }
}

/// Parses `[=] value` of the form `form`.
fn value(parser: &mut Parser, form: Value) -> Result<(), ParserError> {
    let _ = parser.consume_token(&Token::Eq);
    match form {
        Value::Number => number(parser),
        Value::NumberOrDefault => {
            if parse_word(parser, "DEFAULT") {
                Ok(())
            } else {
                number(parser)
            }
        }
        Value::Text => text(parser).map(|_| ()),
        Value::Name => name(parser),
        Value::Choice(choices) => {
            if parse_choice(parser, choices) {
                Ok(())
            } else {
                expected(parser, &choices.join(", "))
            }
        }
        Value::Quoted(choices) => {
            let start = parser.index();
            let found = text(parser)?;
            if choices
                .iter()
                .any(|choice| choice.eq_ignore_ascii_case(&found))
            {
                return Ok(());
            }
            while parser.index() > start {
                parser.prev_token();
            }
            expected(parser, &choices.join(", "))
        }
        Value::Tables => {
            parser.expect_token(&Token::LParen)?;
            parser.parse_comma_separated(|parser| parser.parse_object_name(false))?;
            parser.expect_token(&Token::RParen).map(|_| ())
        }
    }
}

/// Consumes one of the words or numbers `choices` (in any case) if it comes
/// next.
fn parse_choice(parser: &mut Parser, choices: &[&str]) -> bool {
    let matches = match &parser.peek_token_ref().token {
        Token::Word(word) if word.quote_style.is_none() => choices
            .iter()
            .any(|choice| word.value.eq_ignore_ascii_case(choice)),
        Token::Number(number, _) => choices.contains(&number.as_str()),
        _ => false,
    };
    if matches {
        parser.advance_token();
    }
    matches
}

/// A number, or a size with its unit, read by the MySQL dialect as a word
/// that starts with a digit (`4M`).
fn number(parser: &mut Parser) -> Result<(), ParserError> {
    match &parser.peek_token_ref().token {
        Token::Number(..) => {}
        Token::Word(word)
            if word.quote_style.is_none()
                && word.value.starts_with(|c: char| c.is_ascii_digit()) => {}
        _ => return expected(parser, "a number"),
    }
    parser.advance_token();
    Ok(())
}

/// A string between quotes; returns its content.
fn text(parser: &mut Parser) -> Result<String, ParserError> {
    match parser.peek_token_ref().token.clone() {
        Token::SingleQuotedString(text) | Token::DoubleQuotedString(text) => {
            parser.advance_token();
            Ok(text)
        }
        _ => expected(parser, "a string"),
    }
}

/// A name, bare, between backticks or between quotes.
fn name(parser: &mut Parser) -> Result<(), ParserError> {
    match parser.peek_token_ref().token {
        Token::SingleQuotedString(_) | Token::DoubleQuotedString(_) => {
            parser.advance_token();
            Ok(())
        }
        _ => parser.parse_identifier().map(|_| ()),
    }
}

#[cfg(test)]
mod tests {
    use crate::Dialect;
    use crate::parse::parse_statements;

    /// Is the text understood in the dialect?
    fn parses(sql: &str, dialect: Dialect) -> bool {
        parse_statements(sql, dialect).is_ok()
    }

    #[test]
    fn parses_the_options_both_engines_share() {
        for sql in [
            "ALTER TABLE t COMMENT = 'My comment'",
            "ALTER TABLE t COMMENT 'x'",
            "ALTER TABLE t ENGINE = InnoDB",
            "ALTER TABLE t ENGINE = 'InnoDB'",
            "ALTER TABLE t CHARSET = utf8mb4",
            "ALTER TABLE t DEFAULT CHARSET 'utf8mb4'",
            "ALTER TABLE t CHARACTER SET utf8mb4 COLLATE utf8mb4_bin",
            "ALTER TABLE t DEFAULT COLLATE = 'utf8mb4_bin'",
            "ALTER TABLE t CONVERT TO CHARACTER SET utf8mb4 COLLATE utf8mb4_bin",
            "ALTER TABLE t CONVERT TO CHARSET utf8mb4",
            "ALTER TABLE t ENGINE = InnoDB COMMENT = 'y'",
            "ALTER TABLE t ENGINE = InnoDB, COMMENT = 'y'",
            "ALTER TABLE t ADD b INT, COMMENT = 'z'",
            "ALTER TABLE t COMMENT = 'z', ADD c INT",
            "ALTER TABLE t COMMENT = 'z', ALTER INDEX i INVISIBLE",
            "ALTER TABLE t ROW_FORMAT = DYNAMIC",
            "ALTER TABLE t STATS_SAMPLE_PAGES = DEFAULT, STATS_PERSISTENT = 0",
            "ALTER TABLE t KEY_BLOCK_SIZE = 8 MAX_ROWS = 100 MIN_ROWS 1",
            "ALTER TABLE t CHECKSUM = 1 TABLE_CHECKSUM 0 PACK_KEYS = DEFAULT",
            "ALTER TABLE t TABLESPACE innodb_file_per_table",
            "ALTER TABLE t INSERT_METHOD = NO DELAY_KEY_WRITE = 0 AVG_ROW_LENGTH = 10",
            "ALTER TABLE t PASSWORD = 'x', CONNECTION = 'x', STATS_AUTO_RECALC = 1",
            "ALTER TABLE t UNION = (a, db.b)",
            "ALTER TABLE t DATA DIRECTORY = '/tmp' INDEX DIRECTORY '/tmp'",
        ] {
            assert!(parses(sql, Dialect::MySql), "MySQL: {sql}");
            assert!(parses(sql, Dialect::MariaDb), "MariaDB: {sql}");
        }
    }

    #[test]
    fn parses_the_options_of_each_engine() {
        for sql in [
            "ALTER TABLE t AUTOEXTEND_SIZE = 4M",
            "ALTER TABLE t COMPRESSION = 'NONE'",
            "ALTER TABLE t ENCRYPTION = 'N'",
            "ALTER TABLE t ENGINE_ATTRIBUTE = '{}' SECONDARY_ENGINE_ATTRIBUTE = '{}'",
            "ALTER TABLE t TABLESPACE ts STORAGE DISK",
        ] {
            assert!(parses(sql, Dialect::MySql), "MySQL: {sql}");
        }
        for sql in [
            "ALTER TABLE t PAGE_CHECKSUM = 1 TRANSACTIONAL = 0",
            "ALTER TABLE t ENCRYPTED = NO ENCRYPTION_KEY_ID = 2",
            "ALTER TABLE t PAGE_COMPRESSED = 1, PAGE_COMPRESSION_LEVEL = 9",
            "ALTER TABLE t IETF_QUOTES = YES SEQUENCE = 0",
            "ALTER TABLE t ADAPTIVE_HASH_INDEX = DEFAULT",
            "ALTER TABLE t ROW_FORMAT = PAGE",
            "ALTER TABLE t WITH SYSTEM VERSIONING",
            "ALTER TABLE t DROP SYSTEM VERSIONING",
            // Options of the storage engines, checked by the server.
            "ALTER TABLE t ENGINE = CONNECT TABLE_TYPE = CSV FILE_NAME = 'a.csv'",
            "ALTER TABLE t UNKNOWN_OPTION 1",
        ] {
            assert!(parses(sql, Dialect::MariaDb), "MariaDB: {sql}");
            assert!(!parses(sql, Dialect::MySql), "MySQL: {sql}");
        }
    }

    #[test]
    fn refuses_what_the_servers_refuse() {
        for sql in [
            "ALTER TABLE t STORAGE ENGINE = InnoDB",
            "ALTER TABLE t ROW_FORMAT = 'DYNAMIC'",
            "ALTER TABLE t KEY_BLOCK_SIZE = '8'",
            "ALTER TABLE t COMMENT = 1",
            "ALTER TABLE t CHECKSUM = 2",
            "ALTER TABLE t COMMENT = 'x' FOO",
        ] {
            assert!(!parses(sql, Dialect::MySql), "MySQL: {sql}");
        }
        for sql in [
            "ALTER TABLE t STORAGE ENGINE = InnoDB",
            "ALTER TABLE t ROW_FORMAT = 'DYNAMIC'",
            "ALTER TABLE t KEY_BLOCK_SIZE = '8'",
            "ALTER TABLE t COMPRESSION =",
        ] {
            assert!(!parses(sql, Dialect::MariaDb), "MariaDB: {sql}");
        }
        // The typing error of the ticket: an option MySQL does not know.
        assert!(!parses("ALTER TABLE t COMMMENT = 'x'", Dialect::MySql));
    }
}
