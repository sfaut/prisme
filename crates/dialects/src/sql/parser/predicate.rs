//! Tests written with words after a value, as the servers read them
//! (`crates/probe/cases/regexp-grammar.txt`, measured on MySQL 8.0.11 and
//! 8.4.11, MariaDB 10.6.28 and 11.8.9):
//!
//! - `[NOT] REGEXP pattern`, `RLIKE` its synonym, the pattern at the level
//!   of `|` in both dialects (`'2' REGEXP 1 + 1` is true);
//! - `SOUNDS LIKE value`, the value at the level of `|`, without `NOT`
//!   (`NOT SOUNDS LIKE` is a syntax error); `SOUNDS` after a value always
//!   opens it (`SELECT 1 sounds` is a syntax error);
//! - `MEMBER [OF] (array)`, MySQL from 8.0.17, the array a simple operand
//!   between parentheses (`MEMBER OF ('[1]' + 0)` is a syntax error), without
//!   `NOT`. There, `MEMBER` after a value always opens it; before, and in
//!   MariaDB, it is an alias (`SELECT 1 member`).
//!
//! As the other tests, MySQL reads one after a value, MariaDB chains them
//! (`expr` module).

use super::{ParseError, Parser};
use crate::lexer::{Punct, TokenKind};
use crate::sql::ast::{Expr, ExprKind};
use crate::versions::Construct;

impl Parser<'_> {
    /// `REGEXP` or `RLIKE` after `operand`, from its pattern.
    pub(super) fn regexp(&mut self, operand: Expr, negated: bool) -> Result<Expr, ParseError> {
        let pattern = self.bit_or()?;
        let span = self.span_from(operand.span.start);
        Ok(Expr {
            kind: ExprKind::Regexp {
                operand: Box::new(operand),
                pattern: Box::new(pattern),
                negated,
            },
            span,
        })
    }

    /// `SOUNDS LIKE` after `left`, from `SOUNDS`.
    pub(super) fn sounds_like(&mut self, left: Expr) -> Result<Expr, ParseError> {
        self.advance();
        self.expect_keyword("LIKE")?;
        let right = self.bit_or()?;
        let span = self.span_from(left.span.start);
        Ok(Expr {
            kind: ExprKind::SoundsLike {
                left: Box::new(left),
                right: Box::new(right),
            },
            span,
        })
    }

    /// Tells whether `MEMBER` that comes next opens `MEMBER OF`: always in a
    /// version that has it, when `OF` or `(` follows otherwise, refused then.
    pub(super) fn at_member_of(&self) -> bool {
        self.at_keyword("MEMBER")
            && (self.check(Construct::MemberOf, self.here()).is_ok()
                || self.is_word(self.peek_at(1), "OF")
                || self
                    .peek_at(1)
                    .is_some_and(|token| token.kind == TokenKind::Punct(Punct::LeftParen)))
    }

    /// `MEMBER [OF] (array)` after `value`, from `MEMBER`.
    pub(super) fn member_of(&mut self, value: Expr) -> Result<Expr, ParseError> {
        let keyword = self.here();
        self.check(Construct::MemberOf, keyword)?;
        self.advance();
        self.eat_keyword("OF");
        self.expect_punct(Punct::LeftParen)?;
        let array = self.nested(Self::unary)?;
        self.expect_punct(Punct::RightParen)?;
        let span = self.span_from(value.span.start);
        Ok(Expr {
            kind: ExprKind::MemberOf {
                value: Box::new(value),
                array: Box::new(array),
            },
            span,
        })
    }
}
