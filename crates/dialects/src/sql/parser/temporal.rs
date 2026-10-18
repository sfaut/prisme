//! Typed literals: `DATE '...'`, `TIME '...'`, `TIMESTAMP '...'`, and their
//! ODBC form, `{d '...'}`, `{t '...'}`, `{ts '...'}` (`temporal` module).
//!
//! A time zone offset (`TIMESTAMP '2024-05-06 01:00:00+02:00'`) follows the
//! version of the dialect. The ODBC form gives its string, of its strings
//! joined (`{d '2024-05' '-06'}`), when the dialect reads no date or time
//! in it, measured in the four servers: another letter than `d`, `t` or
//! `ts` (`{D '...'}`, `{x '...'}`), an invalid string, a zero date the SQL
//! mode refuses, an offset the version lacks.

use super::{ParseError, Parser};
use crate::lexer::{Punct, TokenKind};
use crate::sql::ast::{Expr, ExprKind, Literal, TemporalKind};
use crate::temporal::{self, Reading};
use crate::versions::Construct;

impl Parser<'_> {
    /// `DATE`, `TIME` or `TIMESTAMP` followed by a string.
    pub(super) fn typed_literal(&mut self, kind: TemporalKind) -> Result<Expr, ParseError> {
        let start = self.here().start;
        self.advance();
        let span = self.here();
        let Some(TokenKind::String { value, .. }) = self.peek().map(|token| token.kind.clone())
        else {
            return Err(self.expected("a string"));
        };
        self.advance();
        if let Reading::Valid(read) = temporal::read(kind, &value, self.options.dialect)
            && read.offset.is_some()
        {
            self.check(Construct::DatetimeOffset, span)?;
        }
        Ok(leaf(
            Literal::Temporal { kind, value },
            self.span_from(start),
        ))
    }

    /// `{d '...'}`, `{t '...'}`, `{ts '...'}`, or the string of another
    /// letter, at its brace.
    pub(super) fn odbc_literal(&mut self) -> Result<Expr, ParseError> {
        let start = self.here();
        self.expect_punct(Punct::LeftBrace)?;
        let letter = match self.peek() {
            Some(token) if token.kind == TokenKind::Word => token.text(self.text).to_string(),
            _ => String::new(),
        };
        let string_next = matches!(
            self.peek_at(1).map(|token| &token.kind),
            Some(TokenKind::String { .. })
        );
        if letter.is_empty() || !string_next {
            return Err(self.unsupported(
                "ODBC escapes other than `{d '...'}`, `{t '...'}` and `{ts '...'}`",
                start,
            ));
        }
        self.advance();
        let strings_start = self.here().start;
        let strings = self.strings(None);
        let value = strings.value.clone();
        let span = self.span_from(strings_start);
        self.expect_punct(Punct::RightBrace)?;
        let kind = match letter.as_str() {
            "d" => Some(TemporalKind::Date),
            "t" => Some(TemporalKind::Time),
            "ts" => Some(TemporalKind::Timestamp),
            _ => None,
        };
        let date = match kind.map(|kind| (kind, temporal::read(kind, &value, self.options.dialect)))
        {
            Some((kind, Reading::Valid(read))) => {
                let mode = self.options.mode;
                let refused = (read.zero_date() && mode.no_zero_date)
                    || (read.zero_in_date() && mode.no_zero_in_date)
                    || (read.offset.is_some()
                        && self.check(Construct::DatetimeOffset, start).is_err());
                (!refused).then_some(kind)
            }
            Some((_, Reading::Unknown)) => {
                return Err(self.unsupported(
                    "an ODBC date or time of a form Prisme does not read",
                    self.span_from(start.start),
                ));
            }
            Some((_, Reading::Invalid)) | None => None,
        };
        // The string alone: the same value and type in both dialects.
        Ok(match date {
            Some(kind) => leaf(
                Literal::Temporal { kind, value },
                self.span_from(start.start),
            ),
            None => leaf(Literal::String(strings), span),
        })
    }
}

/// Literal node.
fn leaf(literal: Literal, span: prisme_plan::Span) -> Expr {
    Expr {
        kind: ExprKind::Literal(literal),
        span,
    }
}
