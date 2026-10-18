//! National strings (`N'...'`) and the character set introducers
//! (`_latin1'abc'`), read alike in both dialects, measured on MySQL 8.0.11
//! and 8.4.11, MariaDB 10.6.28 and 11.8.9
//! (`crates/probe/cases/literals-introducers.txt`).
//!
//! A national string and an introduced string take the strings that follow
//! (`N'a' 'b'` is `N'ab'`), not another introducer nor a national string. An
//! introducer is a word `_` followed by a character set of the dialect
//! (`charsets` module), in any case, then spaces or comments; it introduces a
//! string, a hexadecimal or a bit literal, and nothing else (`_latin1 5` is
//! a syntax error). Hexadecimal and bit literals are not read yet.

use super::{ParseError, Parser};
use crate::charsets::is_charset;
use crate::lexer::TokenKind;
use crate::sql::ast::{Expr, ExprKind, Literal, Strings};

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
            Some(
                TokenKind::HexString(_)
                | TokenKind::HexNumber(_)
                | TokenKind::BitString(_)
                | TokenKind::BitNumber(_),
            ) => {
                return Err(self.unsupported("hexadecimal and bit literals", self.here()));
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

/// Literal node.
fn leaf(literal: Literal, span: prisme_plan::Span) -> Expr {
    Expr {
        kind: ExprKind::Literal(literal),
        span,
    }
}
