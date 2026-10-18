//! `GREATEST` and `LEAST` between MySQL and MariaDB. Both compare numbers
//! as numbers, strings as strings, dates and times as such; they differ on
//! a mix of them:
//!
//! - a string and a number: compared as strings in MySQL, as `DOUBLE` in
//!   MariaDB (`LEAST('10', 9)` gives `'10'`, 9 in MariaDB);
//! - a date and a string: compared as dates in both, the result a string
//!   in MySQL, a date in MariaDB; MySQL leaves an invalid date out of the
//!   comparison, MariaDB takes it as a zero date or NULL;
//! - a time and a string or a number: compared as strings in MySQL, as
//!   times in MariaDB;
//! - integers or decimals with a `NULL` literal: their type in MySQL, a
//!   `DOUBLE` in MariaDB.
//!
//! The context matters too: MariaDB compares strings as numbers when the
//! call is read as a number (`LEAST('10', '9') + 0` gives 9, 10 in MySQL),
//! and MySQL compares a time with a string as numbers then. A call read as
//! a value of its type (`Emitter::column`) is translated in more cases.
//!
//! Measured on MySQL 8.0.11, 8.4 and 9.3, MariaDB 10.6 and 11.8.

use prisme_plan::{Expr, ExprKind, FunctionCall};
use prisme_types::{DataType, Value};

use super::Emitter;
use super::expr::ADDITIVE;
use super::rules::{literal_digits, literal_year};
use crate::Dialect;
use crate::builtins::Rule;
use crate::report::StepKind;

/// Largest number of hours of a `TIME`.
const MAX_HOURS: u32 = 838;

/// What an argument is compared as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Family {
    /// The `NULL` literal.
    Null,
    /// Integer, decimal or boolean.
    Exact,
    /// Floating point number.
    Float,
    /// Character string, `ENUM` or `SET`.
    Text,
    /// Byte string.
    Binary,
    /// Date, date and time, timestamp.
    Date,
    /// Time.
    Time,
    /// Another type, whose comparison differs (`YEAR`, `BIT`, `JSON`...),
    /// or a type unknown.
    Other,
}

impl Family {
    /// Family of a value of `data_type`.
    const fn of(data_type: &DataType) -> Self {
        match data_type {
            DataType::Null => Self::Null,
            DataType::Boolean | DataType::Integer(_) | DataType::Decimal(_) => Self::Exact,
            DataType::Float(_) => Self::Float,
            DataType::Text(_) | DataType::Enum | DataType::Set => Self::Text,
            DataType::Binary(_) => Self::Binary,
            DataType::Date | DataType::DateTime { .. } | DataType::Timestamp { .. } => Self::Date,
            DataType::Time { .. } => Self::Time,
            _ => Self::Other,
        }
    }
}

/// How an argument, or the call, is written.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Conversion {
    /// As in the input.
    None,
    /// `CONCAT(x)`: a string, of the collation of the connection, which
    /// gives way to the collation of a column.
    Concat,
    /// `x + 0e0`: a `DOUBLE`.
    Double,
    /// `CAST(x AS type)`.
    Cast(String),
    /// The `NULL` literal of the type of the numbers: `IF(FALSE, x, NULL)`,
    /// `x` the first argument that is not `NULL`, never evaluated.
    TypedNull,
    /// The `NULL` literal as a `DOUBLE`: `NULL + 0e0`.
    DoubleNull,
}

impl Emitter<'_> {
    /// `GREATEST(x, ...)` and `LEAST(x, ...)`, read as a value of its type
    /// when `column`: natively when the arguments are compared alike in
    /// both dialects, otherwise with the arguments or the result converted
    /// so as to compare them as the input does, or an error.
    pub(super) fn greatest(&mut self, rule: Rule, expr: &Expr, call: &FunctionCall, column: bool) {
        // The server reports the number of arguments.
        if call.args.len() < 2 {
            self.input_call(expr, call);
            return;
        }
        let name = call.name.value.to_ascii_uppercase();
        let families: Vec<Family> = call
            .args
            .iter()
            .map(|arg| Family::of(&arg.ty.data_type))
            .collect();
        let any = |family: Family| families.contains(&family);
        if let Some(arg) = call
            .args
            .iter()
            .find(|arg| Family::of(&arg.ty.data_type) == Family::Other)
        {
            let message = match type_label(&arg.ty.data_type) {
                Some(label) => format!(
                    "`{name}` of a value of type {label}: MySQL and MariaDB compare it with the other arguments differently"
                ),
                None => format!(
                    "`{name}` of an argument whose type is unknown: MySQL and MariaDB compare a string with a number, a date or a time differently"
                ),
            };
            self.greatest_error(rule, &message, expr, call);
            return;
        }
        let strings = any(Family::Text) || any(Family::Binary);
        let numbers = any(Family::Exact) || any(Family::Float);
        let (date, time) = (any(Family::Date), any(Family::Time));
        let towards_mariadb = self.output.dialect == Dialect::MariaDb;
        if !strings && !numbers {
            // Dates and times, NULL: compared alike.
            self.input_call(expr, call);
        } else if !strings && !date && !time {
            self.greatest_numbers(rule, expr, call, &families, &name);
        } else if !numbers && !date && !time {
            self.greatest_strings(rule, expr, call, column, &name);
        } else if !date && !time {
            self.greatest_mixed(rule, expr, call, &families, column, &name);
        } else if date && !time && !numbers && !any(Family::Binary) {
            self.greatest_dates(rule, expr, call, &families, column, &name);
        } else if time && !date && (towards_mariadb || (!numbers && !any(Family::Binary))) {
            self.greatest_times(rule, expr, call, &families, column, &name);
        } else {
            let message = format!(
                "`{name}` of a date or a time with a number or a byte string: MySQL and MariaDB compare them differently"
            );
            self.greatest_error(rule, &message, expr, call);
        }
    }

    /// Numbers, and NULL: a `NULL` literal among integers or decimals gives
    /// a `DOUBLE` in MariaDB.
    fn greatest_numbers(
        &mut self,
        rule: Rule,
        expr: &Expr,
        call: &FunctionCall,
        families: &[Family],
        name: &str,
    ) {
        if !families.contains(&Family::Null) || families.contains(&Family::Float) {
            self.input_call(expr, call);
            return;
        }
        let (conversion, how) = if self.output.dialect == Dialect::MariaDb {
            (
                Conversion::TypedNull,
                "`IF(FALSE, x, NULL)` gives the NULL their type",
            )
        } else {
            (
                Conversion::DoubleNull,
                "`NULL + 0e0` gives the NULL the type `DOUBLE`",
            )
        };
        let conversions = families
            .iter()
            .map(|family| match family {
                Family::Null => conversion.clone(),
                _ => Conversion::None,
            })
            .collect::<Vec<_>>();
        let message = format!(
            "`{name}` of integers or decimals and NULL gives their type in MySQL, a `DOUBLE` in MariaDB: {how}"
        );
        self.step(rule.id(), StepKind::Emulation, message, expr.span);
        self.greatest_call(expr, call, &conversions, false);
    }

    /// Strings, and NULL: compared as numbers by MariaDB when the call is
    /// read as a number.
    fn greatest_strings(
        &mut self,
        rule: Rule,
        expr: &Expr,
        call: &FunctionCall,
        column: bool,
        name: &str,
    ) {
        if column {
            self.input_call(expr, call);
            return;
        }
        let context = format!(
            "`{name}` of strings read as a number compares them as strings in MySQL, as numbers in MariaDB (`{name}('10', '9') + 0` gives 10, 9 in MariaDB)"
        );
        if self.output.dialect == Dialect::MySql {
            let message = format!(
                "{context}: translated as a column of the result, a key of `ORDER BY` or `GROUP BY`"
            );
            self.greatest_error(rule, &message, expr, call);
            return;
        }
        let message = format!("{context}: `CONCAT` reads the result as a string");
        self.step(rule.id(), StepKind::Emulation, message, expr.span);
        let conversions = vec![Conversion::None; call.args.len()];
        self.greatest_call(expr, call, &conversions, true);
    }

    /// Strings and numbers: compared as strings in MySQL, as `DOUBLE` in
    /// MariaDB.
    fn greatest_mixed(
        &mut self,
        rule: Rule,
        expr: &Expr,
        call: &FunctionCall,
        families: &[Family],
        column: bool,
        name: &str,
    ) {
        let context = format!(
            "`{name}` compares strings with numbers as strings in MySQL, as numbers in MariaDB (`{name}('10', 9)` gives `'10'`, 9 in MariaDB)"
        );
        let (converted, conversion, how) = if self.output.dialect == Dialect::MariaDb {
            let how = if column {
                "`CONCAT` converts the numbers to strings"
            } else {
                "`CONCAT` converts the numbers to strings, and reads the result as a string"
            };
            ([Family::Exact, Family::Float], Conversion::Concat, how)
        } else {
            (
                [Family::Text, Family::Binary],
                Conversion::Double,
                "`+ 0e0` converts the strings to numbers",
            )
        };
        let conversions = families
            .iter()
            .map(|family| {
                if converted.contains(family) {
                    conversion.clone()
                } else {
                    Conversion::None
                }
            })
            .collect::<Vec<_>>();
        self.step(
            rule.id(),
            StepKind::Emulation,
            format!("{context}: {how}"),
            expr.span,
        );
        let wrap = self.output.dialect == Dialect::MariaDb && !column;
        self.greatest_call(expr, call, &conversions, wrap);
    }

    /// Dates and strings, and NULL: compared as dates, the result a string
    /// in MySQL, a date in MariaDB. Each string must be a literal valid date.
    fn greatest_dates(
        &mut self,
        rule: Rule,
        expr: &Expr,
        call: &FunctionCall,
        families: &[Family],
        column: bool,
        name: &str,
    ) {
        let valid = call.args.iter().zip(families).all(|(arg, family)| {
            *family != Family::Text
                || matches!(&arg.kind, ExprKind::Literal(Value::Text(text))
                    if literal_year(text).is_some_and(|year| year >= 1000))
        });
        if !valid {
            let message = format!(
                "`{name}` of a date and a string that may not be a valid date: MySQL leaves an invalid date out of the comparison, MariaDB takes it as a zero date or NULL; translated for a literal string `YYYY-MM-DD[ hh:mm:ss[.fraction]]`"
            );
            self.greatest_error(rule, &message, expr, call);
            return;
        }
        let context = format!(
            "`{name}` of a date and a string compares them as dates, and gives a string in MySQL, a date in MariaDB"
        );
        if self.output.dialect == Dialect::MariaDb {
            if !column {
                let message = format!(
                    "{context}, which MySQL reads as a number of the date in an expression: translated as a column of the result, a key of `ORDER BY` or `GROUP BY`"
                );
                self.greatest_error(rule, &message, expr, call);
                return;
            }
            let message = format!("{context}: `CONCAT` converts the result to a string");
            self.step(rule.id(), StepKind::Emulation, message, expr.span);
            let conversions = vec![Conversion::None; call.args.len()];
            self.greatest_call(expr, call, &conversions, true);
            return;
        }
        // The type MariaDB gives: a `DATE` from dates only, otherwise a
        // `DATETIME` with the fractional digits of its arguments.
        let dates_only = call.args.iter().all(|arg| {
            !matches!(
                arg.ty.data_type,
                DataType::DateTime { .. } | DataType::Timestamp { .. }
            )
        });
        let conversions = call
            .args
            .iter()
            .zip(families)
            .map(|(arg, family)| match (&arg.kind, family) {
                (ExprKind::Literal(Value::Text(_)), Family::Text) if dates_only => {
                    Conversion::Cast("DATE".to_string())
                }
                (ExprKind::Literal(Value::Text(text)), Family::Text) => {
                    Conversion::Cast(with_digits("DATETIME", literal_digits(text)))
                }
                _ => Conversion::None,
            })
            .collect::<Vec<_>>();
        let message = format!("{context}: the strings are cast to dates");
        self.step(rule.id(), StepKind::Emulation, message, expr.span);
        self.greatest_call(expr, call, &conversions, false);
    }

    /// Times with strings or numbers, and NULL: compared as strings in
    /// MySQL, as times in MariaDB. Towards MySQL, each string must be a
    /// literal valid time.
    fn greatest_times(
        &mut self,
        rule: Rule,
        expr: &Expr,
        call: &FunctionCall,
        families: &[Family],
        column: bool,
        name: &str,
    ) {
        let context = format!(
            "`{name}` compares a time with a string or a number as strings in MySQL, as times in MariaDB"
        );
        if self.output.dialect == Dialect::MariaDb {
            if !column {
                let message = format!(
                    "{context}, as numbers in MySQL in an expression that reads a number: translated as a column of the result, a key of `ORDER BY` or `GROUP BY`"
                );
                self.greatest_error(rule, &message, expr, call);
                return;
            }
            let conversions = families
                .iter()
                .map(|family| match family {
                    Family::Time | Family::Exact | Family::Float => Conversion::Concat,
                    _ => Conversion::None,
                })
                .collect::<Vec<_>>();
            let message =
                format!("{context}: `CONCAT` converts the times and the numbers to strings");
            self.step(rule.id(), StepKind::Emulation, message, expr.span);
            self.greatest_call(expr, call, &conversions, false);
            return;
        }
        let mut conversions = Vec::with_capacity(call.args.len());
        for (arg, family) in call.args.iter().zip(families) {
            conversions.push(match (&arg.kind, family) {
                (ExprKind::Literal(Value::Text(text)), Family::Text) if literal_time(text) => {
                    Conversion::Cast(with_digits("TIME", literal_digits(text)))
                }
                (_, Family::Text) => {
                    let message = format!(
                        "`{name}` of a time and a string that may not be a valid time: MariaDB compares them as times, MySQL as strings, and its cast to `TIME` gives NULL to an invalid time; translated for a literal string `[-]h:mm:ss[.fraction]`"
                    );
                    self.greatest_error(rule, &message, expr, call);
                    return;
                }
                _ => Conversion::None,
            });
        }
        let message = format!("{context}: the strings are cast to times");
        self.step(rule.id(), StepKind::Emulation, message, expr.span);
        self.greatest_call(expr, call, &conversions, false);
    }

    /// Records an error of `rule`, then writes the call as in the input.
    fn greatest_error(&mut self, rule: Rule, message: &str, expr: &Expr, call: &FunctionCall) {
        self.error(rule.id(), message, expr.span);
        self.input_call(expr, call);
    }

    /// Writes the call, each argument with its conversion, within
    /// `CONCAT(...)` when `wrap`.
    fn greatest_call(
        &mut self,
        expr: &Expr,
        call: &FunctionCall,
        conversions: &[Conversion],
        wrap: bool,
    ) {
        if wrap {
            self.write("CONCAT(");
        }
        let form = self.form(&call.name, expr.span);
        self.call_name(&call.name.value, form);
        let first = call
            .args
            .iter()
            .find(|arg| arg.ty.data_type != DataType::Null);
        for (index, (arg, conversion)) in call.args.iter().zip(conversions).enumerate() {
            if index > 0 {
                self.write(", ");
            }
            match conversion {
                Conversion::None => self.expr(arg, 0),
                Conversion::Concat => {
                    self.write("CONCAT(");
                    self.expr(arg, 0);
                    self.write(")");
                }
                Conversion::Double => {
                    self.expr(arg, ADDITIVE);
                    self.write(" + 0e0");
                }
                Conversion::Cast(target) => {
                    self.write("CAST(");
                    self.expr(arg, 0);
                    self.write(&format!(" AS {target})"));
                }
                Conversion::TypedNull => {
                    self.write("IF(FALSE, ");
                    self.expr(first.unwrap_or(arg), 0);
                    self.write(", ");
                    self.expr(arg, 0);
                    self.write(")");
                }
                Conversion::DoubleNull => {
                    self.expr(arg, ADDITIVE);
                    self.write(" + 0e0");
                }
            }
        }
        self.write(")");
        if wrap {
            self.write(")");
        }
    }
}

/// `type` with `digits` fractional digits: `DATETIME`, `DATETIME(3)`.
fn with_digits(data_type: &str, digits: u8) -> String {
    if digits == 0 {
        data_type.to_string()
    } else {
        format!("{data_type}({digits})")
    }
}

/// Name of a type whose comparison differs, for the messages; `None` for
/// a type unknown.
fn type_label(data_type: &DataType) -> Option<String> {
    Some(match data_type {
        DataType::Year => "`YEAR`".to_string(),
        DataType::Bit { .. } => "`BIT`".to_string(),
        DataType::Hybrid { .. } => "a hexadecimal or bit literal".to_string(),
        DataType::Json => "`JSON`".to_string(),
        DataType::Geometry => "`GEOMETRY`".to_string(),
        DataType::Other(name) => format!("`{}`", name.to_ascii_uppercase()),
        _ => return None,
    })
}

/// Tells whether `text` is a time `[-]h:mm:ss[.fraction]`, at most 838
/// hours, 6 fractional digits.
fn literal_time(text: &str) -> bool {
    let text = text.strip_prefix('-').unwrap_or(text);
    let (clock, fraction) = text.split_once('.').unwrap_or((text, "0"));
    let digits = |part: &str, min: usize, max: usize| {
        (min..=max).contains(&part.len()) && part.bytes().all(|byte| byte.is_ascii_digit())
    };
    let mut parts = clock.split(':');
    let (Some(hours), Some(minutes), Some(seconds), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return false;
    };
    digits(hours, 1, 3)
        && hours.parse::<u32>().is_ok_and(|hours| hours <= MAX_HOURS)
        && digits(minutes, 2, 2)
        && minutes < "60"
        && digits(seconds, 2, 2)
        && seconds < "60"
        && digits(fraction, 1, 6)
}
