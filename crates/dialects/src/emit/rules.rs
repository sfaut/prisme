//! Rules of Prisme that write a built-in function of the input in the
//! output dialect, named by the reference files (`builtins` module): the
//! function of the output when the arguments allow it, an emulation that
//! keeps the result, or an explicit error. Each rule was measured on MySQL
//! 8.0.11 and 8.4, MariaDB 10.6 and 11.8.

use prisme_plan::{Expr, ExprKind, FunctionCall};
use prisme_types::{DataType, Value};

use super::Emitter;
use super::builtin::NameForm;
use super::expr::MULTIPLY;
use crate::Dialect;
use crate::builtins::Rule;
use crate::report::StepKind;
use crate::versions::Construct;

impl Emitter<'_> {
    /// Writes the call `expr` of the built-in function `call` by `rule`, at
    /// level `min` at least.
    pub(super) fn builtin_rule(&mut self, rule: Rule, expr: &Expr, call: &FunctionCall, min: u8) {
        match rule {
            Rule::Now => self.now(rule, expr, call),
            Rule::Pad => self.pad(rule, expr, call),
            Rule::TimeToSec => self.time_to_sec(rule, expr, call, min),
            Rule::Uuid => self.uuid(rule, expr, call),
            _ => {
                self.rule_not_written(rule, &call.name, expr.span);
                self.input_call(expr, call);
            }
        }
    }

    /// `TIME_TO_SEC`: MySQL drops the fraction of the seconds, MariaDB keeps
    /// it (`TIME_TO_SEC('00:39:38.5')` gives 2378, 2378.5 in MariaDB).
    /// Towards MariaDB, `DIV 1` drops it; towards MySQL, a value with a
    /// fraction is an error.
    fn time_to_sec(&mut self, rule: Rule, expr: &Expr, call: &FunctionCall, min: u8) {
        let whole = match call.args.as_slice() {
            [arg] => whole_seconds(&arg.ty.data_type),
            // The server reports the number of arguments.
            _ => true,
        };
        if whole {
            self.input_call(expr, call);
            return;
        }
        match self.output.dialect {
            Dialect::MariaDb => {
                let message = format!(
                    "{} keeps the fraction of the seconds in `TIME_TO_SEC`: `DIV 1` drops it, as MySQL does",
                    self.target()
                );
                self.step(rule.id(), StepKind::Emulation, message, expr.span);
                let parenthesized = MULTIPLY < min;
                if parenthesized {
                    self.write("(");
                }
                self.input_call(expr, call);
                self.write(" DIV 1");
                if parenthesized {
                    self.write(")");
                }
            }
            Dialect::MySql => {
                let message = format!(
                    "`TIME_TO_SEC` of a value that may have a fraction of seconds: {} drops it, MariaDB keeps it",
                    self.target()
                );
                self.error(rule.id(), &message, expr.span);
                self.input_call(expr, call);
            }
        }
    }

    /// `NOW`, `CURRENT_TIMESTAMP`, `SYSDATE` and `LOCALTIME`: the date and
    /// time of a `DATETIME` in MySQL. In MariaDB, the first three give a
    /// `TIMESTAMP` from 11.7.1, of the same value in the time zone of the
    /// session, and `LOCALTIME` the time of a `TIME` from 11.6.2. Towards
    /// those versions, `NOW`, `CURRENT_TIMESTAMP` and `LOCALTIME` become
    /// `LOCALTIMESTAMP`, the same instant of the statement as a `DATETIME`,
    /// and `SYSDATE` a `DATETIME` by `CAST`. From MariaDB, `LOCALTIME` of
    /// those versions becomes `CURRENT_TIME`.
    fn now(&mut self, rule: Rule, expr: &Expr, call: &FunctionCall) {
        let upper = call.name.value.to_ascii_uppercase();
        let localtime = upper == "LOCALTIME";
        let target = self.target();
        if self.output.dialect == Dialect::MySql {
            if localtime && self.input_has(Construct::TimeLocaltime) {
                let message = format!(
                    "`LOCALTIME` of MariaDB is the time of day: `CURRENT_TIME` in {target}"
                );
                self.step(rule.id(), StepKind::Rewrite, message, expr.span);
                self.call_text("CURRENT_TIME", NameForm::Plain, &call.args);
            } else {
                self.input_call(expr, call);
            }
            return;
        }
        let construct = if localtime {
            Construct::TimeLocaltime
        } else {
            Construct::TimestampNow
        };
        if !self.has(construct) {
            self.input_call(expr, call);
            return;
        }
        if upper != "SYSDATE" {
            let message = format!(
                "{target} gives `{upper}` another type: `LOCALTIMESTAMP` gives the same instant as a `DATETIME`"
            );
            self.step(rule.id(), StepKind::Rewrite, message, expr.span);
            self.call_text("LOCALTIMESTAMP", NameForm::Plain, &call.args);
            return;
        }
        let digits = match call.args.as_slice() {
            [] => Some(String::new()),
            [arg] => match &arg.kind {
                ExprKind::Literal(Value::Integer(digits)) => Some(format!("({digits})")),
                _ => None,
            },
            _ => None,
        };
        let Some(digits) = digits else {
            let message = format!(
                "`SYSDATE` with digits that are not an integer: {target} gives a `TIMESTAMP`, cast to a `DATETIME` of these digits"
            );
            self.error(rule.id(), &message, expr.span);
            self.input_call(expr, call);
            return;
        };
        let message = format!(
            "{target} gives `SYSDATE` the type `TIMESTAMP`: cast to `DATETIME`, of the same value"
        );
        self.step(rule.id(), StepKind::Emulation, message, expr.span);
        self.write("CAST(");
        self.input_call(expr, call);
        self.write(&format!(" AS DATETIME{digits})"));
    }

    /// `LPAD` and `RPAD`: MariaDB pads with spaces without a pad, MySQL
    /// needs it. With an empty pad, beyond the length of the string, they
    /// give NULL in MariaDB and MySQL before 8.0.13, an empty string in MySQL
    /// from 8.0.13, the string unchanged from 9.3.0. A literal pad other than
    /// the empty string gives the same result everywhere; another pad is an
    /// error when the two servers differ.
    fn pad(&mut self, rule: Rule, expr: &Expr, call: &FunctionCall) {
        let upper = call.name.value.to_ascii_uppercase();
        let target = self.target();
        let pad = match call.args.as_slice() {
            [_, _] if self.output.dialect == Dialect::MySql => {
                let message = format!(
                    "`{upper}` without a pad pads with spaces: {target} needs the pad `' '`"
                );
                self.step(rule.id(), StepKind::Rewrite, message, expr.span);
                let form = self.form(&call.name, expr.span);
                self.call_name(&call.name.value, form);
                self.expr_list(&call.args);
                self.write(", ' ')");
                return;
            }
            [_, _, pad] => pad,
            // The server reports the number of arguments.
            _ => {
                self.input_call(expr, call);
                return;
            }
        };
        let input = empty_pad(
            self.input_has(Construct::PadEmptyString),
            self.input_has(Construct::PadUnchanged),
        );
        let output = empty_pad(
            self.has(Construct::PadEmptyString),
            self.has(Construct::PadUnchanged),
        );
        let literal = matches!(&pad.kind, ExprKind::Literal(value) if !empty(value));
        if input != output && !literal {
            let message = format!(
                "`{upper}` with a pad that may be empty: beyond the length of the string, {} {} gives {input}, {target} gives {output}",
                self.source.dialect.label(),
                self.source.version
            );
            self.error(rule.id(), &message, expr.span);
        }
        self.input_call(expr, call);
    }

    /// `UUID()`: a `VARCHAR(36)` in MySQL, a value of type `UUID` in MariaDB
    /// from 10.7.1, of the same text, which compares only with UUIDs. Towards
    /// those versions, a `CAST` to `CHAR(36)` keeps the string of MySQL;
    /// towards MySQL, the string of the same text.
    fn uuid(&mut self, rule: Rule, expr: &Expr, call: &FunctionCall) {
        if self.output.dialect == Dialect::MySql || !self.has(Construct::UuidType) {
            self.input_call(expr, call);
            return;
        }
        let message = format!(
            "{} gives `UUID()` the type `UUID`: cast to `CHAR(36)`, the string of the same text",
            self.target()
        );
        self.step(rule.id(), StepKind::Emulation, message, expr.span);
        self.write("CAST(");
        self.input_call(expr, call);
        self.write(" AS CHAR(36))");
    }

    /// Writes the call `call` with its name and form in the input.
    pub(super) fn input_call(&mut self, expr: &Expr, call: &FunctionCall) {
        let form = self.form(&call.name, expr.span);
        self.call_text(&call.name.value, form, &call.args);
    }
}

/// Result of `LPAD` and `RPAD` with an empty pad, beyond the length of the
/// string, from the behaviours of the server.
fn empty_pad(empty_string: bool, unchanged: bool) -> &'static str {
    if unchanged {
        "the string unchanged"
    } else if empty_string {
        "an empty string"
    } else {
        "NULL"
    }
}

/// Tells whether `value` is an empty string.
fn empty(value: &Value) -> bool {
    match value {
        Value::Text(text) => text.is_empty(),
        Value::Binary(bytes) => bytes.is_empty(),
        _ => false,
    }
}

/// Tells whether a value of `data_type` is a whole number of seconds.
fn whole_seconds(data_type: &DataType) -> bool {
    matches!(
        data_type,
        DataType::Null
            | DataType::Integer(_)
            | DataType::Date
            | DataType::Year
            | DataType::Time { fraction: 0 }
            | DataType::DateTime { fraction: 0 }
            | DataType::Timestamp { fraction: 0 }
    )
}
