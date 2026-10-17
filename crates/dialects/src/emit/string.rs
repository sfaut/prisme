//! Functions of strings whose arguments have a syntax of their own, written
//! in their form: `TRIM` and `POSITION`, of the same values in both
//! dialects for character strings (`crates/probe/cases/trim-position.txt`).
//! With a binary pattern or substring and a character string, their string
//! is read as binary, or the call refused, between the dialects (rule
//! `function.binary-argument`, `charset` module).

use prisme_plan::{Expr, Position, Trim};

use super::Emitter;
use super::expr::PREDICATE;
use crate::versions::Construct;

impl Emitter<'_> {
    /// `TRIM([[side] [pattern] FROM] x)`.
    pub(super) fn trim(&mut self, expr: &Expr, trim: &Trim) {
        let pattern: Vec<&Expr> = trim.pattern.iter().collect();
        let binary = self.source.dialect != self.output.dialect
            && self.string_as_binary(
                "TRIM",
                Construct::StringCharset,
                &trim.operand,
                &pattern,
                false,
                expr,
            );
        self.write("TRIM(");
        if let Some(side) = trim.side {
            self.write(side.keyword());
            self.write(" ");
        }
        if let Some(pattern) = &trim.pattern {
            self.expr(pattern, 0);
            self.write(" ");
        }
        if trim.side.is_some() || trim.pattern.is_some() {
            self.write("FROM ");
        }
        self.string_operand(&trim.operand, binary);
        self.write(")");
    }

    /// `POSITION(substring IN string)`.
    pub(super) fn position(&mut self, expr: &Expr, position: &Position) {
        let binary = self.source.dialect != self.output.dialect
            && self.string_as_binary(
                "POSITION",
                Construct::LocateCharset,
                &position.string,
                &[&position.substring],
                false,
                expr,
            );
        self.write("POSITION(");
        // The substring is read below the comparisons.
        self.expr(&position.substring, PREDICATE + 1);
        self.write(" IN ");
        self.string_operand(&position.string, binary);
        self.write(")");
    }

    /// The string of a call, read as binary when `binary`.
    fn string_operand(&mut self, string: &Expr, binary: bool) {
        if binary {
            self.binary_string(string);
        } else {
            self.expr(string, 0);
        }
    }
}
