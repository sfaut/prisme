//! Tests written with words, in their form (`crates/probe/cases/regexp-grammar.txt`):
//! `REGEXP` (`RLIKE` written `REGEXP`), `SOUNDS LIKE` and `MEMBER OF`, an
//! operand of each one read at the level of `|`, the array of `MEMBER OF`
//! a simple operand.
//!
//! `SOUNDS LIKE` compares the `SOUNDEX` of its values, the same in both
//! dialects, as `=` compares them. `MEMBER OF`, MySQL from 8.0.17, is an
//! error towards MariaDB and the earlier versions (rule `json.member-of`).
//! `REGEXP` reads its pattern by the library of the server, ICU in MySQL,
//! PCRE2 in MariaDB: between the dialects, an error (rule
//! `regexp.pattern`). `REGEXP` and `SOUNDS LIKE` read the text of a
//! `JSON`, `MEMBER OF` its value: a `JSON` is written as is in each one.
//!
//! `REGEXP` gives an integer, a `DOUBLE` in MySQL 8.0.11 (construct
//! `regexp_integer`), which arithmetic carries (`(s REGEXP 'b') / 3` is a
//! `DOUBLE` of 4 fractional digits there, a `DECIMAL` elsewhere); `NOT
//! REGEXP`, the `NOT` of a `REGEXP` there, an integer everywhere. Read as a
//! number towards 8.0.11, it is compared with 0, an integer; from 8.0.11, an
//! error (rule `regexp.type`): no expression of the other servers gives
//! that `DOUBLE` of no fractional digit.

use prisme_plan::{Expr, ExprKind};

use super::Emitter;
use super::expr::{PREDICATE, PREFIX};
use crate::Dialect;
use crate::report::StepKind;
use crate::versions::Construct;

impl Emitter<'_> {
    /// `operand [NOT] REGEXP pattern`, its operands at levels `left` and
    /// `right` at least.
    pub(super) fn regexp(&mut self, expr: &Expr, left: u8, right: u8) {
        let ExprKind::Regexp {
            operand,
            pattern,
            negated,
        } = &expr.kind
        else {
            return;
        };
        if self.source.dialect != self.output.dialect {
            let message = format!(
                "{} reads a regular expression by ICU, {} by PCRE2: not supported yet by Prisme",
                self.source_name(),
                self.target()
            );
            self.error("regexp.pattern", &message, expr.span);
        }
        self.json_as_is = true;
        self.expr(operand, left);
        self.write(if *negated { " NOT REGEXP " } else { " REGEXP " });
        self.json_as_is = true;
        self.expr(pattern, right);
    }

    /// `expr` read as a number at level `min`, a `REGEXP` that the input and
    /// the output give as an integer and as a `DOUBLE` (rule `regexp.type`);
    /// true when written.
    pub(super) fn regexp_number(&mut self, expr: &Expr, min: u8) -> bool {
        if !matches!(expr.kind, ExprKind::Regexp { negated: false, .. }) {
            return false;
        }
        let integer =
            self.source.dialect == Dialect::MariaDb || self.input_has(Construct::RegexpInteger);
        if integer
            == (self.output.dialect == Dialect::MariaDb || self.has(Construct::RegexpInteger))
        {
            return false;
        }
        let reading = |integer: bool| if integer { "an integer" } else { "a `DOUBLE`" };
        let message = format!(
            "{} gives the result of `REGEXP` as {}, {} as {}",
            self.source_name(),
            reading(integer),
            self.target(),
            reading(!integer)
        );
        if !integer {
            self.error("regexp.type", &message, expr.span);
            return false;
        }
        let message = format!("{message}: compared with 0, an integer");
        self.step("regexp.type", StepKind::Emulation, message, expr.span);
        let parenthesized = PREDICATE < min;
        if parenthesized {
            self.write("(");
        }
        self.expr(expr, PREDICATE);
        self.write(" <> 0");
        if parenthesized {
            self.write(")");
        }
        true
    }

    /// `left SOUNDS LIKE right`, its operands at levels `left` and `right`
    /// at least.
    pub(super) fn sounds_like(&mut self, expr: &Expr, left: u8, right: u8) {
        let ExprKind::SoundsLike {
            left: first,
            right: second,
        } = &expr.kind
        else {
            return;
        };
        self.json_as_is = true;
        self.expr(first, left);
        self.write(" SOUNDS LIKE ");
        self.json_as_is = true;
        self.expr(second, right);
    }

    /// `value MEMBER OF (array)`, the value at level `left` at least.
    pub(super) fn member_of(&mut self, expr: &Expr, left: u8) {
        let ExprKind::MemberOf { value, array } = &expr.kind else {
            return;
        };
        if !self.has(Construct::MemberOf) {
            let message = format!(
                "{} has no `MEMBER OF`: not supported yet by Prisme",
                self.target()
            );
            self.error("json.member-of", &message, expr.span);
        }
        self.json_as_is = true;
        self.expr(value, left);
        self.write(" MEMBER OF (");
        self.json_as_is = true;
        self.expr(array, PREFIX);
        self.write(")");
    }
}
