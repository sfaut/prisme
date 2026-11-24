//! Reading a statement word by word, on the tokens of `sqlparser`: the
//! primitives of the statements Prisme parses itself (`mysql_statements`,
//! `sqlite_statements`).

use sqlparser::parser::{Parser, ParserError};
use sqlparser::tokenizer::Token;

/// Fails if the statement goes on (neither `;` nor the end of the text).
pub(crate) fn expect_end(parser: &Parser) -> Result<(), ParserError> {
    match parser.peek_token_ref().token {
        Token::SemiColon | Token::EOF => Ok(()),
        _ => expected(parser, "end of statement"),
    }
}

/// Is the `n`th token from the position of the parser the unquoted word
/// `expected` (in any case)?
pub(crate) fn word_is(parser: &Parser, n: usize, expected: &str) -> bool {
    matches!(&parser.peek_nth_token_ref(n).token,
        Token::Word(word) if word.quote_style.is_none() && word.value.eq_ignore_ascii_case(expected))
}

/// Consumes the word `expected` if it comes next.
pub(crate) fn parse_word(parser: &mut Parser, expected: &str) -> bool {
    parse_words(parser, &[expected])
}

/// Consumes the words `expected` if they all come next, in order; consumes
/// nothing otherwise.
pub(crate) fn parse_words(parser: &mut Parser, expected: &[&str]) -> bool {
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
pub(crate) fn expect_word(parser: &mut Parser, expected: &str) -> Result<(), ParserError> {
    expect_words(parser, &[expected])
}

/// Consumes the words `expected`, or fails.
pub(crate) fn expect_words(parser: &mut Parser, words: &[&str]) -> Result<(), ParserError> {
    if parse_words(parser, words) {
        Ok(())
    } else {
        expected(parser, &words.join(" "))
    }
}

/// Error "Expected: `what`, found: ..." at the position of the parser.
pub(crate) fn expected<T>(parser: &Parser, what: &str) -> Result<T, ParserError> {
    parser.expected_ref(what, parser.peek_token_ref())
}
