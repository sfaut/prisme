//! Values read as a boolean, which the servers read their own way.
//!
//! MySQL from 8.0.17 reads a value `x` of `WHERE`, `HAVING`, `ON` and of the
//! operands of `NOT`, `AND`, `OR`, `XOR` and `IS TRUE` as `x <> 0`
//! (construct `condition_compared`, WL#12358).
//!
//! A `JSON` (`crates/probe/cases/json-truth.txt`, measured on MySQL 8.0.11
//! to 8.0.17 and 8.4.11, MariaDB 10.6.28 and 11.8.9):
//!
//! - MySQL from 8.0.17 reads it there compared with 0 by the rules of JSON,
//!   true unless it is the number 0 (`true`, `false`, `null`, an object, an
//!   array, a string);
//! - MySQL before 8.0.17 reads it there as a number, as `IF` and `CASE WHEN`
//!   do in every version: `true` and `"1"` are true, `false`, an object or
//!   an array false;
//! - MariaDB reads its `JSON`, a `LONGTEXT`, as a string: `1` and `0.5` are
//!   true, `true` and `"1"` false.
//!
//! Towards MySQL from MariaDB, the text of the `JSON` (`CAST(j AS CHAR)`)
//! gives the reading of MariaDB, in every context. A `JSON` is never an
//! index of its own: the text loses none. The other directions are an error
//! (rule `boolean.json`); towards MariaDB, only an expression is a `JSON`,
//! its columns being `LONGTEXT`.
//!
//! A fraction: a string, a float, a decimal or a time with a fraction
//! (`crates/probe/cases/fraction-truth.txt`, measured on MySQL 8.0.11,
//! 8.0.16, 8.0.17 and 8.4.11, MariaDB 10.6.28 to 11.8.9):
//!
//! - `WHERE`, `HAVING` and `ON` read it as an integer in MySQL before 8.0.17
//!   (`WHERE '0.5'` false), as compared with 0 otherwise (true);
//! - `XOR` reads its operands as integers in MySQL before 8.0.17 and in
//!   MariaDB before the fix of MDEV-40593 (construct `xor_compared`, from
//!   10.11.19, 11.4.13 and 11.8.9), as compared with 0 otherwise. These
//!   servers read `NOT (a XOR b)` as `(NOT a) XOR b`: `a` read as `NOT`
//!   reads it, alike by all of them;
//! - `NOT`, `AND`, `OR`, `IS TRUE`, `IF` and `CASE WHEN` read it as compared
//!   with 0 everywhere.
//!
//! Both dialects convert a string, a float and a decimal to the same
//! integer, the decimal rounded (`0.5` gives 1, `0.5e0` 0); a time, MySQL
//! rounds its fraction, MariaDB truncates it. Towards the reading as an
//! integer, the value compared with 0 (`x <> 0`) gives the reading of the
//! input; towards the comparison, the value converted to an integer
//! (`CAST(x AS SIGNED)`), except a time, or a value of unknown type that
//! may be one, of another dialect: an error (rule `boolean.fraction`). No
//! index serves a value read as a boolean, nor its emulation.

use prisme_plan::{Expr, ExprKind, FunctionCall, Logical};
use prisme_types::DataType;

use super::Emitter;
use super::expr::PREDICATE;
use crate::Dialect;
use crate::report::StepKind;
use crate::versions::Construct;

/// Context of a value read as a boolean, which reads a fraction as an
/// integer in some versions.
#[derive(Debug, Clone, Copy)]
pub(super) enum Context {
    /// `WHERE`, `HAVING` or `ON`, named.
    Clause(&'static str),
    /// Operand of `XOR`.
    Xor,
}

impl Context {
    /// Name of the context in the report.
    fn label(self) -> &'static str {
        match self {
            Self::Clause(clause) => clause,
            Self::Xor => "`XOR`",
        }
    }
}

/// Reading of a fraction as a boolean by a server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reading {
    /// Compared with 0: `0.5` is true.
    Compared,
    /// Converted to an integer by the dialect: `0.5` is false.
    Integer(Dialect),
}

/// Kind of a value read as a boolean, by its fraction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Fraction {
    /// Without a fraction: an integer, a date, a `JSON` (rule
    /// `boolean.json`)...
    None,
    /// A string, a float or a decimal, which both dialects convert to the
    /// same integer.
    Number,
    /// A time with a fraction, which MySQL rounds and MariaDB truncates, or
    /// a value of unknown type, which may be one.
    Time,
}

impl Fraction {
    /// Kind of a value of type `data_type`.
    fn of(data_type: &DataType) -> Self {
        match data_type {
            DataType::Text(_) | DataType::Binary(_) | DataType::Float(_) => Self::Number,
            DataType::Decimal(decimal) if decimal.scale() > 0 => Self::Number,
            DataType::Time { fraction }
            | DataType::DateTime { fraction }
            | DataType::Timestamp { fraction }
                if *fraction > 0 =>
            {
                Self::Time
            }
            DataType::Unknown | DataType::Other(_) => Self::Time,
            _ => Self::None,
        }
    }
}

impl Emitter<'_> {
    /// Writes `json`, a `JSON` read as a boolean at the level `min`: by a
    /// clause or an operator when `versioned`, otherwise by `IF` or `CASE
    /// WHEN`. As is where the input and the output read it alike, as its
    /// text towards MySQL from MariaDB, an error otherwise.
    pub(super) fn json_condition(&mut self, json: &Expr, min: u8, versioned: bool) {
        let reading = |dialect: Dialect, compared: bool| match dialect {
            Dialect::MySql if versioned && compared => "as true unless it is the number 0",
            Dialect::MySql => "as a number",
            Dialect::MariaDb => "as a string",
        };
        let input = reading(
            self.source.dialect,
            self.input_has(Construct::ConditionCompared),
        );
        let output = reading(self.output.dialect, self.has(Construct::ConditionCompared));
        if input == output {
            self.number_expr(json, min);
            return;
        }
        let message = format!(
            "{} reads a `JSON` read as a boolean {input}, {} {output}",
            self.source_name(),
            self.target()
        );
        if self.source.dialect == Dialect::MariaDb {
            let message = format!("{message}: read as its text");
            self.step("boolean.json", StepKind::Emulation, message, json.span);
            self.write("CAST(");
            self.json_as_is = true;
            self.expr(json, 0);
            self.write(" AS CHAR)");
        } else {
            self.error("boolean.json", &message, json.span);
            self.json_as_is = true;
            self.number_expr(json, min);
        }
    }

    /// `IF`: its condition, a `JSON`, read as by `CASE WHEN`.
    pub(super) fn if_call(&mut self, expr: &Expr, call: &FunctionCall) {
        if call
            .args
            .first()
            .is_none_or(|arg| arg.ty.data_type != DataType::Json)
        {
            self.input_call(expr, call);
            return;
        }
        let form = self.form(&call.name, expr.span);
        self.call_name(&call.name.value, form);
        for (index, arg) in call.args.iter().enumerate() {
            if index == 0 {
                self.json_condition(arg, 0, false);
            } else {
                self.write(", ");
                self.expr(arg, 0);
            }
        }
        self.write(")");
    }

    /// Writes `operand`, read as a boolean in `context` at the level `min`:
    /// a fraction as the input reads it (rule `boolean.fraction`).
    pub(super) fn truth_operand(&mut self, operand: &Expr, min: u8, context: Context) {
        let ExprKind::ToBoolean(value) = &operand.kind else {
            self.number_expr(operand, min);
            return;
        };
        let fraction = Fraction::of(&value.ty.data_type);
        let input = self.fraction_reading(context, false);
        let output = self.fraction_reading(context, true);
        let alike = match (input, output) {
            (Reading::Compared, Reading::Compared) => true,
            (Reading::Integer(a), Reading::Integer(b)) => a == b || fraction == Fraction::Number,
            _ => false,
        };
        if fraction == Fraction::None || alike {
            self.number_expr(operand, min);
            return;
        }
        let what = context.label();
        let source = self.source_name();
        let target = self.target();
        match (input, output) {
            (Reading::Compared, Reading::Integer(_)) => {
                let message = format!(
                    "{source} reads a fraction of {what} as compared with 0, {target} as an \
                     integer (`0.5` false): compared with 0"
                );
                self.step("boolean.fraction", StepKind::Emulation, message, value.span);
                let parenthesized = min > PREDICATE;
                if parenthesized {
                    self.write("(");
                }
                self.number_expr(value, PREDICATE);
                self.write(" <> 0");
                if parenthesized {
                    self.write(")");
                }
            }
            (Reading::Integer(dialect), Reading::Compared)
                if dialect == self.output.dialect || fraction == Fraction::Number =>
            {
                let message = format!(
                    "{source} reads a fraction of {what} as an integer (`0.5` false), {target} \
                     as compared with 0: converted to an integer"
                );
                self.step("boolean.fraction", StepKind::Emulation, message, value.span);
                self.write("CAST(");
                self.number_expr(value, 0);
                self.write(" AS SIGNED)");
            }
            _ => {
                let value_kind = if value.ty.data_type == DataType::Unknown {
                    "a value of unknown type, which may be a time"
                } else {
                    "a time"
                };
                let message = format!(
                    "{source} reads {value_kind} of {what} as an integer, {target} otherwise: \
                     MySQL rounds the fraction of a time, MariaDB truncates it"
                );
                self.error("boolean.fraction", &message, value.span);
                self.number_expr(operand, min);
            }
        }
    }

    /// Writes `a XOR b`, operands at the levels `left` and `right`;
    /// `negated` as the operand of `NOT`, which reads the left operand
    /// alike in every version.
    pub(super) fn xor(&mut self, a: &Expr, b: &Expr, left: u8, right: u8, negated: bool) {
        match &a.kind {
            ExprKind::Logical {
                op: Logical::Xor,
                left: first,
                right: second,
            } if negated => {
                self.at(a.span);
                self.xor(first, second, left, right, true);
            }
            _ if negated => self.number_expr(a, left),
            _ => self.truth_operand(a, left, Context::Xor),
        }
        self.write(" XOR ");
        self.truth_operand(b, right, Context::Xor);
    }

    /// Reading of a fraction in `context` by the output, or by the input.
    fn fraction_reading(&self, context: Context, output: bool) -> Reading {
        let dialect = if output {
            self.output.dialect
        } else {
            self.source.dialect
        };
        let construct = match (dialect, context) {
            (Dialect::MySql, _) => Construct::ConditionCompared,
            (Dialect::MariaDb, Context::Xor) => Construct::XorCompared,
            (Dialect::MariaDb, Context::Clause(_)) => return Reading::Compared,
        };
        let compared = if output {
            self.has(construct)
        } else {
            self.input_has(construct)
        };
        if compared {
            Reading::Compared
        } else {
            Reading::Integer(dialect)
        }
    }
}
