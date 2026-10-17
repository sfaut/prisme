//! Rules of Prisme that write a built-in function of the input in the
//! output dialect, named by the reference files (`builtins` module): the
//! function of the output when the arguments allow it, an emulation that
//! keeps the result, or an explicit error. Each rule was measured on MySQL
//! 8.0.11 and 8.4, MariaDB 10.6 and 11.8.

use prisme_plan::{Expr, FunctionCall};
use prisme_types::DataType;

use super::Emitter;
use super::expr::MULTIPLY;
use crate::Dialect;
use crate::builtins::Rule;
use crate::report::StepKind;

impl Emitter<'_> {
    /// Writes the call `expr` of the built-in function `call` by `rule`, at
    /// level `min` at least.
    pub(super) fn builtin_rule(&mut self, rule: Rule, expr: &Expr, call: &FunctionCall, min: u8) {
        match rule {
            Rule::TimeToSec => self.time_to_sec(rule, expr, call, min),
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

    /// Writes the call `call` with its name and form in the input.
    pub(super) fn input_call(&mut self, expr: &Expr, call: &FunctionCall) {
        let form = self.form(&call.name, expr.span);
        self.call_text(&call.name.value, form, &call.args);
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
