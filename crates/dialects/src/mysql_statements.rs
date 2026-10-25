//! MySQL and MariaDB statements that `sqlparser` (0.63) does not parse:
//! stored routines (`CREATE PROCEDURE`, `CREATE FUNCTION`), scheduled events
//! (`CREATE EVENT`, `ALTER EVENT`, `DROP EVENT`), the operations of
//! `ALTER TABLE` on an existing index (`ALTER INDEX i VISIBLE`, MariaDB's
//! `IGNORED`, `RENAME KEY i TO j`, `DROP KEY i`), and
//! `START TRANSACTION WITH CONSISTENT SNAPSHOT`. Also
//! `SET [GLOBAL | SESSION | LOCAL] TRANSACTION`, which `sqlparser` parses
//! without its scope: `SET GLOBAL TRANSACTION READ WRITE`, a setting of the
//! whole server, would read as `SET TRANSACTION READ WRITE`.
//!
//! Prisme refuses a query it cannot parse, as nothing proves what it does:
//! these statements need their own tree for the write switches (see
//! `docs/V1.md`, section 2.4) to tell them apart. They are parsed with the
//! primitives of `sqlparser` (names, data types, expressions), on its tokens.
//!
//! The header of a routine or an event is parsed to the end: definer, name,
//! parameters and their types, return type, characteristics, schedule. Its
//! body (a statement, or a `BEGIN ... END` block) only runs when the routine
//! is called or the event scheduled; the server checks it at creation. Prisme
//! keeps it as is, and only checks that a body that opens with `BEGIN` closes
//! with `END`: cut at its first `;`, a block says that the `DELIMITER`
//! command is missing before it. A block runs to the end of the text, which
//! `DELIMITER` cuts after it.
//!
//! A loadable function (`CREATE FUNCTION f RETURNS STRING SONAME 'lib.so'`),
//! which installs a library on the server, is not a stored routine: it stays
//! refused, like a query Prisme does not understand.
//!
//! Workaround: `sqlparser` does not parse these statements (`docs/DEVELOPER.md`, section "Workarounds").

use sqlparser::ast::{ObjectName, TransactionAccessMode, TransactionMode};
use sqlparser::parser::{Parser, ParserError};
use sqlparser::tokenizer::{Token, TokenWithSpan};

/// Time units of an event's `EVERY` (simple and compound, as in `INTERVAL`).
const TIME_UNITS: [&str; 20] = [
    "MICROSECOND",
    "SECOND",
    "MINUTE",
    "HOUR",
    "DAY",
    "WEEK",
    "MONTH",
    "QUARTER",
    "YEAR",
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

/// Statement of MySQL or MariaDB that `sqlparser` does not parse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MySqlStatement {
    /// `CREATE [OR REPLACE] PROCEDURE`.
    CreateProcedure {
        /// Name of the procedure.
        name: ObjectName,
    },
    /// `CREATE [OR REPLACE] [AGGREGATE] FUNCTION`, a stored function.
    CreateFunction {
        /// Name of the function.
        name: ObjectName,
    },
    /// `CREATE [OR REPLACE] EVENT`.
    CreateEvent {
        /// Name of the event.
        name: ObjectName,
    },
    /// `ALTER EVENT`.
    AlterEvent {
        /// Name of the event.
        name: ObjectName,
    },
    /// `DROP EVENT`.
    DropEvent {
        /// Name of the event.
        name: ObjectName,
    },
    /// `ALTER TABLE` with at least one operation on an existing index that
    /// `sqlparser` does not know.
    AlterTable {
        /// Name of the table.
        name: ObjectName,
    },
    /// `SET [GLOBAL | SESSION | LOCAL] TRANSACTION characteristic, ...`.
    SetTransaction {
        /// `GLOBAL`: the default of the sessions that open later, on the
        /// whole server.
        global: bool,
        /// Does it give the `READ WRITE` access mode?
        read_write: bool,
    },
    /// `START TRANSACTION characteristic, ...` with `WITH CONSISTENT SNAPSHOT`.
    StartTransaction {
        /// Does it give the `READ WRITE` access mode?
        read_write: bool,
    },
}

/// Parses a routine or an event at the position of the parser: `None`
/// (nothing consumed) if the statement is none of them.
pub(crate) fn parse_routine_or_event(
    parser: &mut Parser,
) -> Option<Result<MySqlStatement, ParserError>> {
    // The header up to the object, given back if it does not lead to one.
    let header = parser.try_parse(header).ok()?;
    Some(match header {
        Header::Create { kind, aggregate } => created(parser, kind, aggregate),
        Header::AlterEvent => altered_event(parser),
        Header::DropEvent => drop_event(parser),
    })
}

/// Parses an `ALTER TABLE` at the position of the parser, its operations
/// left to `sqlparser` except those on an existing index. Called after
/// `sqlparser` failed: `None` (nothing consumed) if the statement is not an
/// `ALTER TABLE`.
pub(crate) fn parse_alter_table(
    parser: &mut Parser,
) -> Option<Result<MySqlStatement, ParserError>> {
    parser
        .try_parse(|parser| {
            expect_word(parser, "ALTER")?;
            // `ALTER ONLINE TABLE` (MariaDB), `ALTER IGNORE TABLE` (MariaDB, MySQL 5).
            let _ = parse_word(parser, "ONLINE") || parse_word(parser, "IGNORE");
            expect_word(parser, "TABLE")
        })
        .ok()?;
    Some(alter_table(parser))
}

/// Parses a `SET [GLOBAL | SESSION | LOCAL] TRANSACTION` at the position of
/// the parser, before `sqlparser`, which reads it without its scope: `None`
/// (nothing consumed) if the statement is not one.
///
/// Workaround: `sqlparser` drops the scope of `SET TRANSACTION`
/// (`docs/DEVELOPER.md`, section "Workarounds").
pub(crate) fn parse_set_transaction(
    parser: &mut Parser,
) -> Option<Result<MySqlStatement, ParserError>> {
    let global = parser
        .try_parse(|parser| {
            expect_word(parser, "SET")?;
            let global = parse_word(parser, "GLOBAL");
            if !global {
                let _ = parse_word(parser, "SESSION") || parse_word(parser, "LOCAL");
            }
            expect_word(parser, "TRANSACTION")?;
            Ok(global)
        })
        .ok()?;
    Some(set_transaction(parser, global))
}

/// `SET ... TRANSACTION` after its keywords: `ISOLATION LEVEL level`,
/// `READ WRITE` or `READ ONLY`, separated by commas.
fn set_transaction(parser: &mut Parser, global: bool) -> Result<MySqlStatement, ParserError> {
    let modes = parser.parse_transaction_modes()?;
    expect_end(parser)?;
    let read_write = modes.contains(&TransactionMode::AccessMode(
        TransactionAccessMode::ReadWrite,
    ));
    Ok(MySqlStatement::SetTransaction { global, read_write })
}

/// Parses a `START TRANSACTION` at the position of the parser, with the
/// characteristics `sqlparser` does not know (`WITH CONSISTENT SNAPSHOT`).
/// Called after `sqlparser` failed: `None` (nothing consumed) if the
/// statement is not a `START TRANSACTION`.
///
/// Workaround: `sqlparser` does not parse `WITH CONSISTENT SNAPSHOT`
/// (`docs/DEVELOPER.md`, section "Workarounds").
pub(crate) fn parse_start_transaction(
    parser: &mut Parser,
) -> Option<Result<MySqlStatement, ParserError>> {
    parser
        .try_parse(|parser| expect_words(parser, &["START", "TRANSACTION"]))
        .ok()?;
    Some(start_transaction(parser))
}

/// `START TRANSACTION` after its keywords: `WITH CONSISTENT SNAPSHOT`,
/// `READ WRITE` or `READ ONLY`, separated by commas.
fn start_transaction(parser: &mut Parser) -> Result<MySqlStatement, ParserError> {
    let mut read_write = false;
    loop {
        if parse_words(parser, &["READ", "WRITE"]) {
            read_write = true;
        } else if !(parse_words(parser, &["READ", "ONLY"])
            || parse_words(parser, &["WITH", "CONSISTENT", "SNAPSHOT"]))
        {
            return expected(parser, "WITH CONSISTENT SNAPSHOT, READ WRITE or READ ONLY");
        }
        if !parser.consume_token(&Token::Comma) {
            break;
        }
    }
    expect_end(parser)?;
    Ok(MySqlStatement::StartTransaction { read_write })
}

/// `ALTER TABLE` after its keywords: `[IF EXISTS] name operation, ...`.
fn alter_table(parser: &mut Parser) -> Result<MySqlStatement, ParserError> {
    parse_words(parser, &["IF", "EXISTS"]);
    let name = parser.parse_object_name(false)?;
    loop {
        index_operation(parser)?;
        if !parser.consume_token(&Token::Comma) {
            break;
        }
    }
    expect_end(parser)?;
    Ok(MySqlStatement::AlterTable { name })
}

/// One operation of `ALTER TABLE`: on an existing index, or any other, left
/// to `sqlparser`.
fn index_operation(parser: &mut Parser) -> Result<(), ParserError> {
    let on_index = |parser: &Parser| word_is(parser, 1, "INDEX") || word_is(parser, 1, "KEY");
    if word_is(parser, 0, "ALTER") && on_index(parser) {
        // ALTER {INDEX | KEY} [IF EXISTS] name {VISIBLE | INVISIBLE | IGNORED | NOT IGNORED}
        parser.advance_token();
        parser.advance_token();
        parse_words(parser, &["IF", "EXISTS"]);
        parser.parse_identifier()?;
        if !(parse_word(parser, "VISIBLE")
            || parse_word(parser, "INVISIBLE")
            || parse_word(parser, "IGNORED")
            || parse_words(parser, &["NOT", "IGNORED"]))
        {
            return expected(parser, "VISIBLE, INVISIBLE, IGNORED or NOT IGNORED");
        }
    } else if word_is(parser, 0, "RENAME") && on_index(parser) {
        // RENAME {INDEX | KEY} old TO new
        parser.advance_token();
        parser.advance_token();
        parser.parse_identifier()?;
        expect_word(parser, "TO")?;
        parser.parse_identifier()?;
    } else if word_is(parser, 0, "DROP") && on_index(parser) {
        // DROP {INDEX | KEY} [IF EXISTS] name
        parser.advance_token();
        parser.advance_token();
        parse_words(parser, &["IF", "EXISTS"]);
        parser.parse_identifier()?;
    } else {
        parser.parse_alter_table_operation()?;
    }
    Ok(())
}

/// Beginning of a routine or an event, up to its object.
enum Header {
    /// `CREATE [OR REPLACE] [DEFINER = ...] [AGGREGATE] {PROCEDURE | FUNCTION | EVENT}`.
    Create {
        /// `PROCEDURE`, `FUNCTION` or `EVENT`.
        kind: &'static str,
        /// `AGGREGATE` written (MariaDB, functions only).
        aggregate: bool,
    },
    /// `ALTER [DEFINER = ...] EVENT`.
    AlterEvent,
    /// `DROP EVENT`.
    DropEvent,
}

/// Header of a routine or an event; fails on another statement (`DEFINER`
/// also precedes `VIEW` and `TRIGGER`), left to `sqlparser`.
fn header(parser: &mut Parser) -> Result<Header, ParserError> {
    if parse_word(parser, "CREATE") {
        parse_words(parser, &["OR", "REPLACE"]);
        optional_definer(parser)?;
        let aggregate = parse_word(parser, "AGGREGATE");
        let Some(kind) = ["PROCEDURE", "FUNCTION", "EVENT"]
            .into_iter()
            .find(|kind| parse_word(parser, kind))
        else {
            return expected(parser, "PROCEDURE, FUNCTION or EVENT");
        };
        Ok(Header::Create { kind, aggregate })
    } else if parse_word(parser, "ALTER") {
        optional_definer(parser)?;
        expect_word(parser, "EVENT")?;
        Ok(Header::AlterEvent)
    } else {
        expect_words(parser, &["DROP", "EVENT"])?;
        Ok(Header::DropEvent)
    }
}

/// `DEFINER = account`, if written.
fn optional_definer(parser: &mut Parser) -> Result<(), ParserError> {
    if parse_word(parser, "DEFINER") {
        parser.expect_token(&Token::Eq)?;
        definer(parser)?;
    }
    Ok(())
}

/// Routine or event created, after its keyword (`PROCEDURE`, `FUNCTION`,
/// `EVENT`).
fn created(
    parser: &mut Parser,
    kind: &str,
    aggregate: bool,
) -> Result<MySqlStatement, ParserError> {
    if aggregate && kind != "FUNCTION" {
        return expected(parser, "FUNCTION after AGGREGATE");
    }
    parse_words(parser, &["IF", "NOT", "EXISTS"]);
    let name = parser.parse_object_name(false)?;
    match kind {
        "PROCEDURE" => {
            parameters(parser)?;
            characteristics(parser)?;
            body(parser)?;
            Ok(MySqlStatement::CreateProcedure { name })
        }
        "FUNCTION" => {
            parameters(parser)?;
            expect_word(parser, "RETURNS")?;
            data_type(parser)?;
            characteristics(parser)?;
            body(parser)?;
            Ok(MySqlStatement::CreateFunction { name })
        }
        _ => {
            expect_words(parser, &["ON", "SCHEDULE"])?;
            schedule(parser)?;
            event_options(parser);
            expect_word(parser, "DO")?;
            body(parser)?;
            Ok(MySqlStatement::CreateEvent { name })
        }
    }
}

/// Changes of an event, after `ALTER [DEFINER = ...] EVENT`: at least one.
fn altered_event(parser: &mut Parser) -> Result<MySqlStatement, ParserError> {
    let name = parser.parse_object_name(false)?;
    let mut changes = 0;
    if parse_words(parser, &["ON", "SCHEDULE"]) {
        schedule(parser)?;
        changes += 1;
    }
    changes += event_options(parser);
    if parse_words(parser, &["RENAME", "TO"]) {
        parser.parse_object_name(false)?;
        changes += 1;
    }
    changes += event_options(parser);
    if parse_word(parser, "DO") {
        body(parser)?;
    } else if changes == 0 {
        return expected(
            parser,
            "ON SCHEDULE, ON COMPLETION, RENAME TO, ENABLE, DISABLE, COMMENT or DO",
        );
    } else {
        expect_end(parser)?;
    }
    Ok(MySqlStatement::AlterEvent { name })
}

/// `DROP EVENT`, after its keywords: `[IF EXISTS] name`.
fn drop_event(parser: &mut Parser) -> Result<MySqlStatement, ParserError> {
    parse_words(parser, &["IF", "EXISTS"]);
    let name = parser.parse_object_name(false)?;
    expect_end(parser)?;
    Ok(MySqlStatement::DropEvent { name })
}

/// Definer of a routine or an event, after `DEFINER =`: `user`, `'user'@'host'`,
/// `CURRENT_USER`, `CURRENT_USER()`, or MariaDB's `CURRENT_ROLE` and role.
fn definer(parser: &mut Parser) -> Result<(), ParserError> {
    if parse_word(parser, "CURRENT_USER") || parse_word(parser, "CURRENT_ROLE") {
        if parser.consume_token(&Token::LParen) {
            parser.expect_token(&Token::RParen)?;
        }
        return Ok(());
    }
    account_part(parser)?;
    if parser.consume_token(&Token::AtSign) {
        account_part(parser)?;
    }
    Ok(())
}

/// User or host of an account: a name, quoted or not.
fn account_part(parser: &mut Parser) -> Result<(), ParserError> {
    match parser.next_token().token {
        Token::Word(_) | Token::SingleQuotedString(_) | Token::DoubleQuotedString(_) => Ok(()),
        _ => {
            parser.prev_token();
            expected(parser, "a user name")
        }
    }
}

/// Parameters of a routine, between parentheses:
/// `[IN | OUT | INOUT | IN OUT] name type, ...`.
fn parameters(parser: &mut Parser) -> Result<(), ParserError> {
    parser.expect_token(&Token::LParen)?;
    if parser.consume_token(&Token::RParen) {
        return Ok(());
    }
    loop {
        // At most one mode: `IN OUT` (MariaDB) before `IN`.
        if !parse_words(parser, &["IN", "OUT"])
            && !parse_word(parser, "IN")
            && !parse_word(parser, "OUT")
        {
            parse_word(parser, "INOUT");
        }
        parser.parse_identifier()?;
        data_type(parser)?;
        if !parser.consume_token(&Token::Comma) {
            break;
        }
    }
    parser.expect_token(&Token::RParen)?;
    Ok(())
}

/// Data type of a parameter or a result, with its character set and
/// collation (`VARCHAR(20) CHARACTER SET utf8mb4 COLLATE utf8mb4_bin`).
fn data_type(parser: &mut Parser) -> Result<(), ParserError> {
    parser.parse_data_type()?;
    while parse_words(parser, &["CHARACTER", "SET"])
        || parse_word(parser, "CHARSET")
        || parse_word(parser, "COLLATE")
    {
        parser.parse_identifier()?;
    }
    Ok(())
}

/// Characteristics of a routine, in any order: `COMMENT 'text'`,
/// `LANGUAGE SQL`, `[NOT] DETERMINISTIC`, `CONTAINS SQL`, `NO SQL`,
/// `READS SQL DATA`, `MODIFIES SQL DATA`, `SQL SECURITY {DEFINER | INVOKER}`.
fn characteristics(parser: &mut Parser) -> Result<(), ParserError> {
    loop {
        if parse_word(parser, "COMMENT") {
            parser.parse_literal_string()?;
        } else if parse_word(parser, "LANGUAGE") {
            expect_word(parser, "SQL")?;
        } else if parse_words(parser, &["SQL", "SECURITY"]) {
            if !(parse_word(parser, "DEFINER") || parse_word(parser, "INVOKER")) {
                return expected(parser, "DEFINER or INVOKER");
            }
        } else if !(parse_word(parser, "DETERMINISTIC")
            || parse_words(parser, &["NOT", "DETERMINISTIC"])
            || parse_words(parser, &["CONTAINS", "SQL"])
            || parse_words(parser, &["NO", "SQL"])
            || parse_words(parser, &["READS", "SQL", "DATA"])
            || parse_words(parser, &["MODIFIES", "SQL", "DATA"]))
        {
            return Ok(());
        }
    }
}

/// Schedule of an event, after `ON SCHEDULE`: `AT instant`, or
/// `EVERY quantity unit [STARTS instant] [ENDS instant]`.
fn schedule(parser: &mut Parser) -> Result<(), ParserError> {
    if parse_word(parser, "AT") {
        parser.parse_expr()?;
        return Ok(());
    }
    expect_word(parser, "EVERY")?;
    parser.parse_expr()?;
    if !TIME_UNITS.iter().any(|unit| parse_word(parser, unit)) {
        return expected(parser, "a time unit (DAY, HOUR, ...)");
    }
    if parse_word(parser, "STARTS") {
        parser.parse_expr()?;
    }
    if parse_word(parser, "ENDS") {
        parser.parse_expr()?;
    }
    Ok(())
}

/// Options of an event, in any order: `ON COMPLETION [NOT] PRESERVE`,
/// `ENABLE`, `DISABLE [ON {SLAVE | REPLICA}]`, `COMMENT 'text'`. Returns the
/// number of options read.
fn event_options(parser: &mut Parser) -> usize {
    let mut read = 0;
    loop {
        let option = if parse_words(parser, &["ON", "COMPLETION"]) {
            parse_word(parser, "NOT");
            parse_word(parser, "PRESERVE")
        } else if parse_word(parser, "ENABLE") {
            true
        } else if parse_word(parser, "DISABLE") {
            if !parse_words(parser, &["ON", "SLAVE"]) {
                parse_words(parser, &["ON", "REPLICA"]);
            }
            true
        } else if parse_word(parser, "COMMENT") {
            parser.parse_literal_string().is_ok()
        } else {
            false
        };
        if !option {
            return read;
        }
        read += 1;
    }
}

/// Body of a routine or an event: a statement, up to its `;`, or a block
/// `[label:] BEGIN ... END [label]`, up to the end of the text.
fn body(parser: &mut Parser) -> Result<(), ParserError> {
    let label = (parser.peek_nth_token_ref(1).token == Token::Colon)
        .then(|| parser.peek_token_ref().token.to_string());
    let block = word_is(parser, if label.is_some() { 2 } else { 0 }, "BEGIN");
    let mut tokens: Vec<TokenWithSpan> = Vec::new();
    loop {
        let token = &parser.peek_token_ref().token;
        if *token == Token::EOF || (!block && *token == Token::SemiColon) {
            break;
        }
        tokens.push(parser.next_token());
    }
    if !block {
        return if tokens.is_empty() {
            expected(parser, "a body (a statement, or BEGIN ... END)")
        } else {
            Ok(())
        };
    }
    // Separators at the end belong to the text, not to the block.
    while tokens
        .last()
        .is_some_and(|token| token.token == Token::SemiColon)
    {
        tokens.pop();
    }
    let is_end = |token: Option<&TokenWithSpan>| {
        token.is_some_and(|token| matches!(&token.token,
            Token::Word(word) if word.quote_style.is_none() && word.value.eq_ignore_ascii_case("END")))
    };
    let last = tokens.len() - 1;
    let closed = match &label {
        // `END label`, or `END` alone (MySQL and MariaDB accept both).
        Some(label) => {
            is_end(tokens.get(last))
                || (tokens[last].token.to_string().eq_ignore_ascii_case(label)
                    && is_end(last.checked_sub(1).and_then(|n| tokens.get(n))))
        }
        None => is_end(tokens.get(last)),
    };
    if closed {
        Ok(())
    } else {
        parser.expected(
            "END at the end of the BEGIN block (a DELIMITER command keeps the ; of the body)",
            tokens[last].clone(),
        )
    }
}

/// Fails if the statement goes on (neither `;` nor the end of the text).
pub(crate) fn expect_end(parser: &Parser) -> Result<(), ParserError> {
    match parser.peek_token_ref().token {
        Token::SemiColon | Token::EOF => Ok(()),
        _ => expected(parser, "end of statement"),
    }
}

/// Is the `n`th token from the position of the parser the unquoted word
/// `expected` (in any case)?
fn word_is(parser: &Parser, n: usize, expected: &str) -> bool {
    matches!(&parser.peek_nth_token_ref(n).token,
        Token::Word(word) if word.quote_style.is_none() && word.value.eq_ignore_ascii_case(expected))
}

/// Consumes the word `expected` if it comes next.
fn parse_word(parser: &mut Parser, expected: &str) -> bool {
    parse_words(parser, &[expected])
}

/// Consumes the words `expected` if they all come next, in order; consumes
/// nothing otherwise.
fn parse_words(parser: &mut Parser, expected: &[&str]) -> bool {
    if expected
        .iter()
        .enumerate()
        .all(|(n, word)| word_is(parser, n, word))
    {
        for _ in expected {
            parser.advance_token();
        }
        true
    } else {
        false
    }
}

/// Consumes the word `expected`, or fails.
fn expect_word(parser: &mut Parser, expected: &str) -> Result<(), ParserError> {
    expect_words(parser, &[expected])
}

/// Consumes the words `expected`, or fails.
fn expect_words(parser: &mut Parser, words: &[&str]) -> Result<(), ParserError> {
    if parse_words(parser, words) {
        Ok(())
    } else {
        expected(parser, &words.join(" "))
    }
}

/// Error "Expected: `what`, found: ..." at the position of the parser.
fn expected<T>(parser: &Parser, what: &str) -> Result<T, ParserError> {
    parser.expected_ref(what, parser.peek_token_ref())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Dialect;
    use crate::parse::{Parsed, parse_statements};

    /// Statement of Prisme parsed from `sql`, with its name; panics otherwise.
    fn parsed(sql: &str) -> MySqlStatement {
        match parse_statements(sql, Dialect::MySql).as_deref() {
            Ok([Parsed::MySql(statement)]) => statement.clone(),
            other => panic!("{sql}: {other:?}"),
        }
    }

    /// Name of a statement of Prisme, as written.
    fn name_of(statement: &MySqlStatement) -> String {
        match statement {
            MySqlStatement::CreateProcedure { name }
            | MySqlStatement::CreateFunction { name }
            | MySqlStatement::CreateEvent { name }
            | MySqlStatement::AlterEvent { name }
            | MySqlStatement::DropEvent { name }
            | MySqlStatement::AlterTable { name } => name.to_string(),
            MySqlStatement::SetTransaction { .. } | MySqlStatement::StartTransaction { .. } => {
                panic!("a statement without a name: {statement:?}")
            }
        }
    }

    /// Error of `sqlparser` or of Prisme on `sql`, which must not parse.
    fn error_of(sql: &str) -> String {
        match parse_statements(sql, Dialect::MariaDb) {
            Err(error) => error.to_string(),
            Ok(statements) => panic!("{sql}: {statements:?}"),
        }
    }

    #[test]
    fn parses_procedures() {
        for sql in [
            "CREATE PROCEDURE p() BEGIN SELECT 1; END",
            "create procedure p() select 1",
            "CREATE PROCEDURE p() SELECT 1;",
            "CREATE DEFINER = 'root'@'localhost' PROCEDURE p(IN a INT, OUT b VARCHAR(20) CHARACTER SET utf8mb4 COLLATE utf8mb4_bin, INOUT c DECIMAL(10, 2)) COMMENT 'x' LANGUAGE SQL NOT DETERMINISTIC SQL SECURITY INVOKER MODIFIES SQL DATA BEGIN UPDATE t SET a = a + 1; END",
            "CREATE DEFINER=CURRENT_USER() PROCEDURE p() SELECT 1",
            "CREATE DEFINER=CURRENT_USER PROCEDURE p() SELECT 1",
            "CREATE DEFINER=`root`@`%` PROCEDURE p() SELECT 1",
            "CREATE DEFINER=root@localhost PROCEDURE p() SELECT 1",
            "CREATE OR REPLACE PROCEDURE p(IN OUT a INT) SELECT 1",
            "CREATE PROCEDURE IF NOT EXISTS p() SELECT 1",
            "CREATE PROCEDURE p() l1: BEGIN LEAVE l1; END l1",
            "CREATE PROCEDURE p() BEGIN IF a THEN SELECT 1; END IF; WHILE b DO SET b = 0; END WHILE; END",
        ] {
            let statement = parsed(sql);
            assert!(
                matches!(statement, MySqlStatement::CreateProcedure { .. }),
                "{sql}"
            );
            assert_eq!(name_of(&statement), "p", "{sql}");
        }
        assert_eq!(
            name_of(&parsed("CREATE PROCEDURE shop.`my p`() SELECT 1")),
            "shop.`my p`"
        );
    }

    #[test]
    fn parses_stored_functions() {
        for sql in [
            "CREATE FUNCTION f(x INT) RETURNS INT DETERMINISTIC RETURN x + 1",
            "CREATE FUNCTION f(x INT) RETURNS VARCHAR(10) CHARSET utf8mb4 NO SQL BEGIN DECLARE y INT DEFAULT 0; RETURN 'a'; END",
            "CREATE OR REPLACE AGGREGATE FUNCTION f(x INT) RETURNS INT BEGIN DECLARE CONTINUE HANDLER FOR NOT FOUND RETURN 0; LOOP FETCH GROUP NEXT ROW; END LOOP; END",
            "CREATE DEFINER = 'a'@'%' FUNCTION IF NOT EXISTS f() RETURNS DATETIME READS SQL DATA RETURN NOW()",
        ] {
            assert!(
                matches!(parsed(sql), MySqlStatement::CreateFunction { .. }),
                "{sql}"
            );
        }
    }

    #[test]
    fn parses_events() {
        for sql in [
            "CREATE EVENT e ON SCHEDULE EVERY 1 DAY DO DELETE FROM t",
            "CREATE EVENT IF NOT EXISTS e ON SCHEDULE AT CURRENT_TIMESTAMP + INTERVAL 1 HOUR ON COMPLETION NOT PRESERVE DISABLE COMMENT 'x' DO BEGIN DELETE FROM t; END",
            "CREATE DEFINER = CURRENT_USER EVENT e ON SCHEDULE EVERY '1:30' HOUR_MINUTE STARTS '2026-01-01' ENDS '2027-01-01' DISABLE ON SLAVE DO SELECT 1",
        ] {
            assert!(
                matches!(parsed(sql), MySqlStatement::CreateEvent { .. }),
                "{sql}"
            );
        }
        for sql in [
            "ALTER EVENT e DISABLE",
            "ALTER EVENT e ON SCHEDULE EVERY 2 DAY",
            "ALTER EVENT e ON COMPLETION PRESERVE RENAME TO f ENABLE",
            "ALTER DEFINER = CURRENT_USER EVENT e COMMENT 'y'",
            "ALTER EVENT e DO DELETE FROM t",
        ] {
            assert!(
                matches!(parsed(sql), MySqlStatement::AlterEvent { .. }),
                "{sql}"
            );
        }
        assert_eq!(name_of(&parsed("DROP EVENT IF EXISTS shop.e")), "shop.e");
        assert!(matches!(
            parsed("DROP EVENT e"),
            MySqlStatement::DropEvent { .. }
        ));
    }

    #[test]
    fn parses_the_operations_on_an_existing_index() {
        for sql in [
            "ALTER TABLE t ALTER INDEX i INVISIBLE",
            "ALTER TABLE t ALTER INDEX i VISIBLE",
            "ALTER TABLE t ALTER KEY `i` IGNORED",
            "ALTER TABLE t ALTER INDEX IF EXISTS i NOT IGNORED",
            "ALTER TABLE shop.t RENAME KEY i TO j, DROP KEY k",
            "ALTER TABLE t ADD KEY (a), DROP INDEX IF EXISTS i, ALGORITHM = INPLACE",
            "ALTER ONLINE TABLE t DROP KEY i",
        ] {
            assert!(
                matches!(parsed(sql), MySqlStatement::AlterTable { .. }),
                "{sql}"
            );
        }
        // What `sqlparser` parses stays its own.
        assert!(matches!(
            parse_statements("ALTER TABLE t ADD KEY (a)", Dialect::MySql).as_deref(),
            Ok([Parsed::Sql(_)])
        ));
    }

    #[test]
    fn leaves_the_other_statements_to_sqlparser() {
        for sql in [
            "CREATE TABLE event (a INT)",
            "ALTER TABLE event ADD COLUMN b INT",
            "DROP PROCEDURE p",
            "DROP FUNCTION f",
            "CREATE INDEX event ON t (a)",
        ] {
            assert!(
                matches!(
                    parse_statements(sql, Dialect::MySql).as_deref(),
                    Ok([Parsed::Sql(_)])
                ),
                "{sql}"
            );
        }
        // A definer before a view: not a routine.
        assert!(!matches!(
            parse_statements(
                "CREATE DEFINER = root@localhost VIEW v AS SELECT 1",
                Dialect::MySql
            )
            .as_deref(),
            Ok([Parsed::MySql(_)])
        ));
        // Statements around a statement of Prisme.
        assert!(matches!(
            parse_statements("DROP EVENT e; SELECT 1", Dialect::MySql).as_deref(),
            Ok([Parsed::MySql(_), Parsed::Sql(_)])
        ));
    }

    #[test]
    fn refuses_what_the_servers_refuse_or_prisme_cannot_check() {
        assert!(error_of("CREATE PROCEDURE p() BEGIN SELECT 1").contains("DELIMITER"));
        assert!(error_of("CREATE PROCEDURE p() l: BEGIN SELECT 1; END x").contains("DELIMITER"));
        error_of("CREATE PROCEDURE p()");
        error_of("CREATE FUNCTION f RETURNS STRING SONAME 'udf.so'");
        error_of("CREATE AGGREGATE PROCEDURE p() SELECT 1");
        error_of("CREATE FUNCTION f(x INT) SELECT 1");
        error_of("CREATE PROCEDURE p() SQL SECURITY NOBODY SELECT 1");
        error_of("CREATE EVENT e ON SCHEDULE EVERY 1 FORTNIGHT DO SELECT 1");
        error_of("CREATE EVENT e ON SCHEDULE EVERY 1 DAY");
        error_of("ALTER EVENT e");
        error_of("DROP EVENT e f");
        error_of("ALTER TABLE t ALTER INDEX i HIDDEN");
        error_of("ALTER TABLE t RENAME KEY i");
        error_of("SET GLOBAL TRANSACTION READ SOMETHING");
        error_of("START TRANSACTION WITH CONSISTENT");
        error_of("START TRANSACTION WITH CONSISTENT SNAPSHOT,");
    }

    #[test]
    fn parses_set_transaction_with_its_scope() {
        for (sql, global, read_write) in [
            ("SET TRANSACTION READ WRITE", false, true),
            ("SET SESSION TRANSACTION READ ONLY", false, false),
            ("set local transaction read write", false, true),
            ("SET GLOBAL TRANSACTION READ WRITE", true, true),
            (
                "SET GLOBAL TRANSACTION ISOLATION LEVEL SERIALIZABLE",
                true,
                false,
            ),
            (
                "SET TRANSACTION ISOLATION LEVEL READ COMMITTED, READ WRITE",
                false,
                true,
            ),
        ] {
            assert_eq!(
                parsed(sql),
                MySqlStatement::SetTransaction { global, read_write },
                "{sql}"
            );
        }
        // Another `SET` stays with `sqlparser`.
        assert!(matches!(
            parse_statements("SET SESSION sql_mode = ''", Dialect::MySql).as_deref(),
            Ok([Parsed::Sql(_)])
        ));
    }

    #[test]
    fn parses_start_transaction_with_a_consistent_snapshot() {
        for (sql, read_write) in [
            ("START TRANSACTION WITH CONSISTENT SNAPSHOT", false),
            (
                "START TRANSACTION WITH CONSISTENT SNAPSHOT, READ WRITE",
                true,
            ),
            (
                "START TRANSACTION READ ONLY, WITH CONSISTENT SNAPSHOT",
                false,
            ),
        ] {
            assert_eq!(
                parsed(sql),
                MySqlStatement::StartTransaction { read_write },
                "{sql}"
            );
        }
        // Without `WITH CONSISTENT SNAPSHOT`, `sqlparser` parses it.
        assert!(matches!(
            parse_statements("START TRANSACTION READ WRITE", Dialect::MySql).as_deref(),
            Ok([Parsed::Sql(_)])
        ));
    }
}
