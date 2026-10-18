//! National strings (`N'...'`), hexadecimal and bit literals (`X'41'`,
//! `0x41`, `b'1000001'`, `0b1000001`) and the character set introducers
//! (`_latin1'abc'`, `_utf8mb4 X'C3A9'`, `_binary b'1'`), read alike in both
//! dialects, measured on MySQL 8.0.11 and 8.4.11, MariaDB 10.6.28 and 11.8.9
//! (`crates/probe/cases/literals-introducers.txt`, `literals-hex.txt`).
//!
//! A national string and an introduced string take the strings that follow
//! (`N'a' 'b'` is `N'ab'`), not another introducer nor a national string. An
//! introducer is a word `_` followed by a character set of the dialect
//! (`charsets` module), in any case, then spaces or comments; it introduces a
//! string, a hexadecimal or a bit literal, and nothing else (`_latin1 5` is
//! a syntax error). The digits of a hexadecimal or bit literal are padded
//! on the left to whole bytes (`0x4` is the byte `04`, `b'1'` the byte
//! `01`).

use super::{ParseError, Parser};
use crate::charsets::is_charset;
use crate::lexer::TokenKind;
use crate::sql::ast::{BytesForm, Expr, ExprKind, Literal, Strings};

impl Parser<'_> {
    /// Tells whether the next token is a character set introducer.
    pub(super) fn at_introducer(&self) -> bool {
        self.peek().is_some_and(|token| {
            token.kind == TokenKind::Word
                && token
                    .text(self.text)
                    .strip_prefix('_')
                    .is_some_and(|name| is_charset(self.options.dialect, name))
        })
    }

    /// Character set introducer and the literal it introduces.
    pub(super) fn introduced(&mut self) -> Result<Expr, ParseError> {
        let start = self.here().start;
        let word = self.advance().expect("an introducer comes next");
        let charset = word.text(self.text)[1..].to_ascii_lowercase();
        let literal = match self.peek().map(|token| &token.kind) {
            Some(TokenKind::String { .. }) => Literal::String(self.strings(None)),
            Some(kind) if let Some(literal) = bytes(kind) => {
                self.advance();
                literal
            }
            _ => return Err(self.expected("a string, a hexadecimal or a bit literal")),
        };
        Ok(leaf(
            Literal::Introduced {
                charset,
                literal: Box::new(literal),
            },
            self.span_from(start),
        ))
    }

    /// National string, at its `N'...'`, and the strings that follow.
    pub(super) fn national(&mut self) -> Expr {
        let start = self.here().start;
        let Some(TokenKind::NationalString(first)) = self.advance().map(|token| token.kind) else {
            unreachable!("a national string comes next");
        };
        let strings = self.strings(Some(first));
        leaf(Literal::National(strings), self.span_from(start))
    }

    /// Hexadecimal or bit literal, at its token.
    pub(super) fn bytes_literal(&mut self) -> Expr {
        let token = self.advance().expect("a literal comes next");
        let literal = bytes(&token.kind).expect("a hexadecimal or bit literal comes next");
        leaf(literal, token.span)
    }

    /// The string `first`, when read already, and the strings that follow,
    /// joined.
    pub(super) fn strings(&mut self, first: Option<String>) -> Strings {
        let mut parts: Vec<String> = first.into_iter().collect();
        while let Some(TokenKind::String { value: part, .. }) = self.peek().map(|token| &token.kind)
        {
            parts.push(part.clone());
            self.pos += 1;
        }
        Strings {
            first: parts.first().map_or(0, String::len),
            value: parts.concat(),
        }
    }
}

/// Literal of a hexadecimal or bit token, `None` for another token.
fn bytes(kind: &TokenKind) -> Option<Literal> {
    let (digits, form) = match kind {
        TokenKind::HexString(digits) => (digits, BytesForm::HexString),
        TokenKind::HexNumber(digits) => (digits, BytesForm::HexNumber),
        TokenKind::BitString(digits) => (digits, BytesForm::BitString),
        TokenKind::BitNumber(digits) => (digits, BytesForm::BitNumber),
        _ => return None,
    };
    let bytes = match form {
        BytesForm::HexString | BytesForm::HexNumber => from_digits(digits, 2, 16),
        BytesForm::BitString | BytesForm::BitNumber => from_digits(digits, 8, 2),
    };
    Some(Literal::Bytes { bytes, form })
}

/// Bytes of `digits` in base `radix`, `per_byte` digits a byte, padded on
/// the left to whole bytes. The lexer checked the digits.
fn from_digits(digits: &str, per_byte: usize, radix: u32) -> Vec<u8> {
    let padding = (per_byte - digits.len() % per_byte) % per_byte;
    let padded = "0".repeat(padding) + digits;
    padded
        .as_bytes()
        .chunks(per_byte)
        .map(|chunk| {
            let chunk = std::str::from_utf8(chunk).expect("ASCII digits");
            u8::from_str_radix(chunk, radix).expect("digits checked by the lexer")
        })
        .collect()
}

/// Literal node.
fn leaf(literal: Literal, span: prisme_plan::Span) -> Expr {
    Expr {
        kind: ExprKind::Literal(literal),
        span,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digits_are_padded_to_whole_bytes() {
        assert_eq!(from_digits("4", 2, 16), [0x04]);
        assert_eq!(from_digits("C3A9", 2, 16), [0xC3, 0xA9]);
        assert_eq!(from_digits("1", 8, 2), [0x01]);
        assert_eq!(from_digits("101000001", 8, 2), [0x01, 0x41]);
        assert!(from_digits("", 2, 16).is_empty());
    }
}
