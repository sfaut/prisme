//! `ABS`, `CEIL`, `CEILING`, `FLOOR`, `ROUND` and `TRUNCATE` of a date or a
//! time between MySQL and MariaDB (rule `function.rounding`). MySQL reads
//! the date or the time as a number, a `DOUBLE` (`CEIL(dt)` gives
//! 20240506070810), MariaDB as a temporal value: `ABS` gives a `DECIMAL` of
//! its digits, the others an integer for a `DATE`, a value of its type
//! rounded otherwise (`CEIL(dt)` gives 2024-05-06 07:08:10, `ROUND(TIME
//! '10:20:30.5')` 10:20:31 where MySQL rounds 102030.5 half to even).
//!
//! Towards MariaDB, `x + 0e0` reads the argument as MySQL does, of the same
//! value (MySQL writes the `DOUBLE` with the digits of the fraction of the
//! argument, `20240506070809.120` for `20240506070809.12`). Towards MySQL,
//! a `DATE` is written `x + 0`, an integer, and the argument of `ABS`
//! `CAST(x AS DECIMAL(p, f))`, of its digits; the date and time and the
//! time of the others are an error. An argument of a type unknown to
//! Prisme may be a date or a time: an error as well. Measured on each local
//! image (`crates/probe/cases/rounding.txt`, `rounding-writes.txt`).

use prisme_plan::{Expr, FunctionCall};
use prisme_types::DataType;

use super::Emitter;
use super::expr::ADDITIVE;
use crate::Dialect;
use crate::builtins::Rule;
use crate::report::StepKind;

/// Integer digits of a date read as a number (`20240506`).
const DATE_DIGITS: u8 = 8;
/// Integer digits of a date and time read as a number (`20240506070809`).
const DATETIME_DIGITS: u8 = 14;
/// Integer digits of a time read as a number, up to 838 hours (`8385959`).
const TIME_DIGITS: u8 = 7;

/// How the first argument is written in the output.
enum Writing {
    /// As in the input.
    Same,
    /// `x + 0e0`, a `DOUBLE`, as MySQL reads a date or a time.
    Double,
    /// `x + 0`, the integer of a date, as MariaDB reads it.
    Integer,
    /// `CAST(x AS DECIMAL(digits, fraction))`, the digits of a date or a
    /// time, as `ABS` of MariaDB reads it.
    Decimal { digits: u8, fraction: u8 },
    /// An error, of this message.
    Refused(String),
}

impl Emitter<'_> {
    /// Writes the call `expr` of `call`, a rounding function, its first
    /// argument read as the input reads it.
    pub(super) fn rounding(&mut self, rule: Rule, expr: &Expr, call: &FunctionCall) {
        let Some(first) = call.args.first() else {
            // The server reports the number of arguments.
            self.input_call(expr, call);
            return;
        };
        let name = call.name.value.to_ascii_uppercase();
        let target = self.target();
        let writing = self.rounding_writing(&name, &first.ty.data_type);
        let (message, before, after, min) = match writing {
            Writing::Same => {
                self.input_call(expr, call);
                return;
            }
            Writing::Refused(message) => {
                self.error(rule.id(), &message, first.span.or(expr.span));
                self.input_call(expr, call);
                return;
            }
            Writing::Double => (
                format!(
                    "{target} reads a date or a time as a temporal value in `{name}`: `+ 0e0` reads it as a `DOUBLE`, as MySQL does"
                ),
                "",
                " + 0e0".to_string(),
                ADDITIVE,
            ),
            Writing::Integer => (
                format!(
                    "MariaDB reads a date as an integer in `{name}`: `+ 0` gives that integer to {target}, which reads a `DOUBLE`"
                ),
                "",
                " + 0".to_string(),
                ADDITIVE,
            ),
            Writing::Decimal { digits, fraction } => (
                format!(
                    "MariaDB gives `ABS` of a date or a time as a `DECIMAL` of its digits: a `CAST` gives them to {target}, which reads a `DOUBLE`"
                ),
                "CAST(",
                match fraction {
                    0 => format!(" AS DECIMAL({digits}))"),
                    _ => format!(" AS DECIMAL({digits}, {fraction}))"),
                },
                0,
            ),
        };
        self.step(
            rule.id(),
            StepKind::Emulation,
            message,
            first.span.or(expr.span),
        );
        let form = self.form(&call.name, expr.span);
        self.call_name(&call.name.value, form);
        self.write(before);
        self.expr(first, min);
        self.write(&after);
        for arg in &call.args[1..] {
            self.write(", ");
            self.expr(arg, 0);
        }
        self.write(")");
    }

    /// How the output writes the first argument of `name`, of `data_type`.
    fn rounding_writing(&self, name: &str, data_type: &DataType) -> Writing {
        let abs = name == "ABS";
        let (digits, fraction) = match data_type {
            DataType::Date => (DATE_DIGITS, 0),
            DataType::DateTime { fraction } | DataType::Timestamp { fraction } => {
                (DATETIME_DIGITS + fraction, *fraction)
            }
            DataType::Time { fraction } => (TIME_DIGITS + fraction, *fraction),
            DataType::Unknown => {
                return Writing::Refused(format!(
                    "`{name}` of an argument of a type unknown to Prisme: a date or a time is read as a number by MySQL, as a temporal value by MariaDB"
                ));
            }
            _ => return Writing::Same,
        };
        match self.output.dialect {
            Dialect::MariaDb => Writing::Double,
            Dialect::MySql if abs => Writing::Decimal { digits, fraction },
            Dialect::MySql if *data_type == DataType::Date => Writing::Integer,
            Dialect::MySql => Writing::Refused(format!(
                "`{name}` of a date and time or a time: MariaDB rounds it as a value of its type (`CEIL(dt)` gives 2024-05-06 07:08:10), {} as a number (20240506070810)",
                self.target()
            )),
        }
    }
}
