//! Functions of strings whose arguments have a syntax of their own, as the
//! servers read them (`crates/probe/cases/trim-position.txt`, measured on
//! MySQL 8.0.11 and 8.4.11, MariaDB 10.6.28 and 11.8.9):
//!
//! - `TRIM(x)`, `TRIM(side FROM x)`, `TRIM(pattern FROM x)` and
//!   `TRIM(side pattern FROM x)`, `side` one of `BOTH`, `LEADING` and
//!   `TRAILING`, the pattern and the string whole expressions
//!   (`TRIM('a' = 'a' FROM s)`);
//! - `POSITION(substring IN string)`, the substring read below the
//!   comparisons (`POSITION('a' = 'a' IN s)` is a syntax error), the string
//!   a whole expression.
//!
//! A space after the name calls a stored function (`TRIM (s)`), as on the
//! servers (`call` module).

use prisme_plan::TrimSide;

use super::{ParseError, Parser};
use crate::lexer::Punct;
use crate::sql::ast::{Expr, ExprKind, PositionCall, TrimCall};

impl Parser<'_> {
    /// `TRIM(...)`, from the `(` after its name written from `start`.
    pub(super) fn trim(&mut self, start: usize) -> Result<Expr, ParseError> {
        self.expect_punct(Punct::LeftParen)?;
        let side = [
            ("BOTH", TrimSide::Both),
            ("LEADING", TrimSide::Leading),
            ("TRAILING", TrimSide::Trailing),
        ]
        .into_iter()
        .find_map(|(keyword, side)| self.eat_keyword(keyword).map(|_| side));
        let (pattern, operand) = if side.is_some() {
            let pattern = if self.eat_keyword("FROM").is_some() {
                None
            } else {
                let pattern = self.expr()?;
                self.expect_keyword("FROM")?;
                Some(pattern)
            };
            (pattern, self.expr()?)
        } else {
            let first = self.expr()?;
            if self.eat_keyword("FROM").is_some() {
                (Some(first), self.expr()?)
            } else {
                (None, first)
            }
        };
        self.expect_punct(Punct::RightParen)?;
        Ok(Expr {
            kind: ExprKind::Trim(Box::new(TrimCall {
                side,
                pattern,
                operand,
            })),
            span: self.span_from(start),
        })
    }

    /// `POSITION(substring IN string)`, from the `(` after its name written
    /// from `start`.
    pub(super) fn position(&mut self, start: usize) -> Result<Expr, ParseError> {
        self.expect_punct(Punct::LeftParen)?;
        let substring = self.bit_or()?;
        self.expect_keyword("IN")?;
        let string = self.expr()?;
        self.expect_punct(Punct::RightParen)?;
        Ok(Expr {
            kind: ExprKind::Position(Box::new(PositionCall { substring, string })),
            span: self.span_from(start),
        })
    }
}
