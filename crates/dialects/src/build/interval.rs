//! Date and time arithmetic: the type of an interval added to or subtracted
//! from a date or a time, as MySQL and MariaDB give it
//! (`crates/probe/cases/interval*.txt`, measured on MySQL 8.0.11 to 8.4.11,
//! MariaDB 10.6.28 and 11.8.9), always nullable, a date out of range giving
//! NULL:
//!
//! - a `DATE`, by a unit of days or longer: a `DATE`; by a shorter unit, a
//!   `DATETIME`;
//! - a `DATETIME` or a `TIMESTAMP`: a `DATETIME`;
//! - a `TIME`, by a unit shorter than a day: a `TIME`; by a unit of days or
//!   longer, a `DATETIME` in MySQL from 8.0.28, a `TIME` before; a `TIME` in
//!   MariaDB by `DAY` and its compound units, a `DATETIME` by the longer
//!   ones;
//! - another value, read as a date and time: a `VARCHAR`, of 29 characters
//!   in MySQL, of the date, the time and their fraction in MariaDB.
//!
//! The fractional digits are those of the operand, or more: 6 by a unit of
//! microseconds; by `SECOND`, those of a decimal value, 6 for a string or a
//! float.

use prisme_plan::{DateArithmetic, Expr, ExprKind, TimeUnit};
use prisme_types::{DataType, MAX_FRACTION_DIGITS, TextType, Type, Value};

use super::scope::Scope;
use super::{BuildError, Builder, temporal};
use crate::Dialect;
use crate::sql::ast;
use crate::versions::{Construct, registry};

/// Length of the string of a date and time in MySQL.
const MYSQL_LENGTH: u32 = 29;

/// Length of a date and time without fraction (`2024-05-06 07:08:09`).
const DATETIME_LENGTH: u32 = 19;

impl Builder<'_> {
    /// Interval added to or subtracted from a date or a time.
    pub(super) fn date_arithmetic(
        &mut self,
        call: &ast::DateArithmeticCall,
        scope: &Scope,
    ) -> Result<Expr, BuildError> {
        let operand = self.value(&call.operand, scope)?;
        let value = self.value(&call.value, scope)?;
        let time_datetime = registry(self.dialect)
            .check(self.dialect, Construct::TimeIntervalDatetime, self.version)
            .is_ok();
        let data_type = arithmetic_type(&operand, &value, call.unit, self.dialect, time_datetime);
        let arithmetic = DateArithmetic {
            operand,
            value,
            unit: call.unit,
            form: call.form,
        };
        Ok(Expr::date_arithmetic(arithmetic, Type::nullable(data_type)))
    }
}

/// Data type of `operand` moved by `value` of `unit` in `dialect`;
/// `time_datetime` when a `TIME` moved by days gives a `DATETIME`.
fn arithmetic_type(
    operand: &Expr,
    value: &Expr,
    unit: TimeUnit,
    dialect: Dialect,
    time_datetime: bool,
) -> DataType {
    let added = interval_fraction(unit, &value.ty.data_type);
    match &operand.ty.data_type {
        DataType::Date if unit.time() => DataType::DateTime { fraction: added },
        DataType::Date => DataType::Date,
        DataType::DateTime { fraction } | DataType::Timestamp { fraction } => DataType::DateTime {
            fraction: added.max(*fraction),
        },
        DataType::Time { fraction } => {
            let fraction = added.max(*fraction);
            let datetime = unit.days()
                && match dialect {
                    Dialect::MySql => time_datetime,
                    Dialect::MariaDb => !matches!(
                        unit,
                        TimeUnit::Day
                            | TimeUnit::DayMicrosecond
                            | TimeUnit::DaySecond
                            | TimeUnit::DayMinute
                            | TimeUnit::DayHour
                    ),
                };
            if datetime {
                DataType::DateTime { fraction }
            } else {
                DataType::Time { fraction }
            }
        }
        DataType::Unknown => DataType::Unknown,
        _ => {
            let length = match dialect {
                Dialect::MySql => MYSQL_LENGTH,
                Dialect::MariaDb => {
                    let fraction = u32::from(added.max(operand_fraction(operand)));
                    DATETIME_LENGTH + if fraction > 0 { fraction + 1 } else { 0 }
                }
            };
            DataType::Text(TextType {
                length: Some(length),
                fixed: false,
                charset: None,
                collation: None,
            })
        }
    }
}

/// Fractional digits an interval of `unit` and value of type `value` adds.
fn interval_fraction(unit: TimeUnit, value: &DataType) -> u8 {
    if unit.microseconds() {
        return MAX_FRACTION_DIGITS;
    }
    if unit != TimeUnit::Second {
        return 0;
    }
    match value {
        DataType::Null | DataType::Boolean | DataType::Integer(_) => 0,
        DataType::Decimal(decimal) => decimal.scale().min(MAX_FRACTION_DIGITS),
        _ => MAX_FRACTION_DIGITS,
    }
}

/// Fractional digits MariaDB reads in an operand that is not a date nor a
/// time: those written in a string literal, the scale of a decimal, none for
/// an integer, 6 otherwise.
fn operand_fraction(operand: &Expr) -> u8 {
    match (&operand.kind, &operand.ty.data_type) {
        (ExprKind::Literal(Value::Text(text)), _) => temporal::written_digits(text),
        (_, DataType::Null | DataType::Boolean | DataType::Integer(_)) => 0,
        (_, DataType::Decimal(decimal)) => decimal.scale().min(MAX_FRACTION_DIGITS),
        _ => MAX_FRACTION_DIGITS,
    }
}
