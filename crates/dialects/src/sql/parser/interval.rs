//! Date and time arithmetic of the MySQL family, as the servers read it
//! (`crates/probe/cases/interval*.txt`):
//!
//! - `x + INTERVAL n unit` and `x - INTERVAL n unit`, read at the level of
//!   `+`; the sum closes at its unit, then takes the operators of a higher
//!   level (`d + INTERVAL 1 DAY * 2` multiplies the sum);
//! - `INTERVAL n unit + x`, an operand whose `x` runs to the next `AND`,
//!   `OR` or `XOR` (`INTERVAL 1 DAY + d = e` adds the interval to the
//!   comparison); MariaDB reads `- INTERVAL n unit + x` as `x - INTERVAL n
//!   unit`, and no other prefix operator before it;
//! - `DATE_ADD(x, INTERVAL n unit)`, `DATE_SUB`, `ADDDATE(x, INTERVAL n
//!   unit)` or `ADDDATE(x, n)` of `n` days, `SUBDATE`, and
//!   `TIMESTAMPADD(unit, n, x)` of a simple unit.
//!
//! The value `n` is a whole expression, closed by its unit
//! (`INTERVAL 1 + 1 DAY`). `INTERVAL(n, n1, ...)`, whose parentheses hold a
//! comma, is a function. `SQL_TSI_` before a simple unit other than
//! `MICROSECOND` names the same unit.

use prisme_plan::{DateArithmeticForm, Span, TimeUnit};

use super::{ParseError, Parser};
use crate::lexer::{Punct, TokenKind};
use crate::sql::ast::{DateArithmeticCall, Expr, ExprKind};

impl Parser<'_> {
    /// Tells whether an interval comes next: `INTERVAL`, other than the
    /// function `INTERVAL(n, n1, ...)`.
    pub(super) fn at_interval(&self) -> bool {
        if !self.at_keyword("INTERVAL") {
            return false;
        }
        if !matches!(
            self.peek_at(1).map(|token| &token.kind),
            Some(TokenKind::Punct(Punct::LeftParen))
        ) {
            return true;
        }
        // A comma between the parentheses that follow makes a function.
        let mut depth = 0_usize;
        let mut offset = 2;
        while let Some(token) = self.peek_at(offset) {
            match token.kind {
                TokenKind::Punct(Punct::LeftParen) => depth += 1,
                TokenKind::Punct(Punct::RightParen) if depth == 0 => return true,
                TokenKind::Punct(Punct::RightParen) => depth -= 1,
                TokenKind::Punct(Punct::Comma) if depth == 0 => return false,
                _ => {}
            }
            offset += 1;
        }
        true
    }

    /// `x + INTERVAL n unit` or, when `subtract`, `x - INTERVAL n unit`,
    /// after the operator.
    pub(super) fn interval_operand(
        &mut self,
        operand: Expr,
        subtract: bool,
    ) -> Result<Expr, ParseError> {
        let start = operand.span.start;
        let (value, unit) = self.interval()?;
        let form = if subtract {
            DateArithmeticForm::Minus
        } else {
            DateArithmeticForm::Plus
        };
        Ok(arithmetic(
            operand,
            value,
            unit,
            form,
            self.span_from(start),
        ))
    }

    /// `INTERVAL n unit + x`, from `INTERVAL`.
    pub(super) fn interval_first(&mut self) -> Result<Expr, ParseError> {
        let start = self.here().start;
        let (value, unit) = self.interval()?;
        self.interval_first_end(start, value, unit)
    }

    /// `DATE_ADD(x, INTERVAL n unit)`, `DATE_SUB`, `ADDDATE` or `SUBDATE`,
    /// named `upper`, after the name.
    pub(super) fn date_function(&mut self, start: usize, upper: &str) -> Result<Expr, ParseError> {
        let days = matches!(upper, "ADDDATE" | "SUBDATE");
        let subtract = matches!(upper, "DATE_SUB" | "SUBDATE");
        self.expect_punct(Punct::LeftParen)?;
        let operand = self.expr()?;
        self.expect_punct(Punct::Comma)?;
        let (value, unit, interval) = if self.at_interval() {
            let interval_start = self.here().start;
            let (value, unit) = self.interval()?;
            if days && self.at_punct(Punct::Plus) {
                // `ADDDATE(x, INTERVAL n unit + y)`: days, of the interval
                // added to `y`.
                let sum = self.interval_first_end(interval_start, value, unit)?;
                (sum, TimeUnit::Day, false)
            } else {
                (value, unit, true)
            }
        } else if days {
            (self.expr()?, TimeUnit::Day, false)
        } else {
            return Err(self.expected("`INTERVAL`"));
        };
        self.expect_punct(Punct::RightParen)?;
        let form = match (upper, interval) {
            ("DATE_ADD", _) => DateArithmeticForm::DateAdd,
            ("DATE_SUB", _) => DateArithmeticForm::DateSub,
            (_, true) if subtract => DateArithmeticForm::SubDate,
            (_, true) => DateArithmeticForm::AddDate,
            _ if subtract => DateArithmeticForm::SubDays,
            _ => DateArithmeticForm::AddDays,
        };
        Ok(arithmetic(
            operand,
            value,
            unit,
            form,
            self.span_from(start),
        ))
    }

    /// `TIMESTAMPADD(unit, n, x)`, after its name.
    pub(super) fn timestamp_add(&mut self, start: usize) -> Result<Expr, ParseError> {
        self.expect_punct(Punct::LeftParen)?;
        let unit = self.time_unit(true)?;
        self.expect_punct(Punct::Comma)?;
        let value = self.expr()?;
        self.expect_punct(Punct::Comma)?;
        let operand = self.expr()?;
        self.expect_punct(Punct::RightParen)?;
        Ok(arithmetic(
            operand,
            value,
            unit,
            DateArithmeticForm::TimestampAdd,
            self.span_from(start),
        ))
    }

    /// `INTERVAL n unit`, from `INTERVAL`: its value and its unit.
    fn interval(&mut self) -> Result<(Expr, TimeUnit), ParseError> {
        self.expect_keyword("INTERVAL")?;
        let value = self.expr()?;
        let unit = self.time_unit(false)?;
        Ok((value, unit))
    }

    /// `+ x` after `INTERVAL n unit` written from `start`: `x` runs to the
    /// next `AND`, `OR` or `XOR`.
    fn interval_first_end(
        &mut self,
        start: usize,
        value: Expr,
        unit: TimeUnit,
    ) -> Result<Expr, ParseError> {
        self.expect_punct(Punct::Plus)?;
        let operand = self.nested(Self::not_expr)?;
        Ok(arithmetic(
            operand,
            value,
            unit,
            DateArithmeticForm::IntervalFirst,
            self.span_from(start),
        ))
    }

    /// Unit of an interval, a simple one when `simple`.
    fn time_unit(&mut self, simple: bool) -> Result<TimeUnit, ParseError> {
        let what = if simple {
            "a unit of time (`SECOND`, `DAY`...)"
        } else {
            "a unit of time (`SECOND`, `DAY`, `HOUR_MINUTE`...)"
        };
        let Some(token) = self.peek().filter(|token| token.kind == TokenKind::Word) else {
            return Err(self.expected(what));
        };
        let upper = token.text(self.text).to_ascii_uppercase();
        let (name, prefixed) = match upper.strip_prefix("SQL_TSI_") {
            Some(name) => (name, true),
            None => (upper.as_str(), false),
        };
        let unit = TimeUnit::ALL.into_iter().find(|unit| {
            unit.keyword() == name
                && (!simple || unit.simple())
                && (!prefixed || (unit.simple() && *unit != TimeUnit::Microsecond))
        });
        let Some(unit) = unit else {
            return Err(self.expected(what));
        };
        self.advance();
        Ok(unit)
    }
}

/// Date and time arithmetic over `span`.
fn arithmetic(
    operand: Expr,
    value: Expr,
    unit: TimeUnit,
    form: DateArithmeticForm,
    span: Span,
) -> Expr {
    Expr {
        kind: ExprKind::DateArithmetic(Box::new(DateArithmeticCall {
            operand,
            value,
            unit,
            form,
        })),
        span,
    }
}
