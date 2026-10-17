//! Date and time arithmetic towards MySQL and MariaDB, written in its form
//! (`d + INTERVAL 1 DAY`, `DATE_ADD`, `ADDDATE(d, 1)`, `TIMESTAMPADD`), of the
//! same values in both dialects for a `DATE`, a `DATETIME` or a `TIMESTAMP`
//! (`crates/probe/cases/interval*.txt`). The values that differ between the
//! dialects, or between two versions of MySQL (rules):
//!
//! - `interval.time`: a `TIME` moved by days or longer gives a `DATETIME`,
//!   the time read on the current date, in MySQL from 8.0.28, written
//!   `TIMESTAMP(CURRENT_DATE, t)` towards the other versions and MariaDB; a
//!   `TIME` of the hours added by `DAY` and its compound units elsewhere,
//!   and by `WEEK` in MySQL before 8.0.28, NULL by the longer units: an
//!   error where the readings differ otherwise;
//! - `interval.string`: another value, read as a date and time, gives a
//!   string whose fractional seconds MySQL writes with 6 digits when they
//!   are not zero, MariaDB with the digits of its operands: an error, NULL
//!   and a string literal of a date or of a date and time without fraction,
//!   moved by a unit without fraction, aside;
//! - `interval.year`: a number of years beyond 2^31, which MariaDB reduces
//!   to 32 bits (`INTERVAL 4294967297 YEAR` adds a year), MySQL giving NULL:
//!   an error for a value whose type may reach it;
//! - `interval.microseconds`: microseconds of more than 6 digits in the
//!   string of a compound unit (`INTERVAL '1.1234567' SECOND_MICROSECOND`),
//!   a number of microseconds in MySQL, their first 6 digits in MariaDB: an
//!   error for a value whose text may hold them;
//! - `interval.unknown`: an operand of unknown type, a function, which may
//!   be a `TIME` or a string: an error, until the types of the function
//!   catalog.
//!
//! The dates moved into the year 0, out of the range both dialects support
//! (from `1000-01-01`), give a zero date in MySQL, NULL in MariaDB: no rule
//! (`docs/DEV-DIALECTS.md`, section 6.3).

use prisme_plan::{DateArithmetic, DateArithmeticForm, Expr, ExprKind, TimeUnit};
use prisme_types::{DataType, IntegerSize, Value};

use super::Emitter;
use super::cast::plain_temporal;
use super::expr::{ADDITIVE, NOT, OPERAND, PREDICATE};
use crate::Dialect;
use crate::report::StepKind;
use crate::versions::Construct;

/// Number of years from which MariaDB reduces a value to 32 bits.
const YEAR_BITS: i128 = 1 << 31;

/// Largest number of digits of the microseconds of a compound unit read
/// alike.
const MICROSECOND_DIGITS: usize = 6;

/// Value of a `TIME` moved by days or longer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TimeReading {
    /// A `DATETIME`, the time read on the current date (MySQL from 8.0.28).
    Today,
    /// A `TIME`, of the hours added.
    Hours,
    /// NULL, of type `TIME`.
    NullTime,
    /// NULL, of type `DATETIME`.
    NullDatetime,
}

impl TimeReading {
    /// Reading of `dialect` for `unit`; `datetime` when the version reads the
    /// time on the current date.
    fn of(dialect: Dialect, datetime: bool, unit: TimeUnit) -> Self {
        let day = matches!(
            unit,
            TimeUnit::Day
                | TimeUnit::DayMicrosecond
                | TimeUnit::DaySecond
                | TimeUnit::DayMinute
                | TimeUnit::DayHour
        );
        match dialect {
            Dialect::MySql if datetime => Self::Today,
            Dialect::MySql if day || unit == TimeUnit::Week => Self::Hours,
            Dialect::MySql => Self::NullTime,
            Dialect::MariaDb if day => Self::Hours,
            Dialect::MariaDb => Self::NullDatetime,
        }
    }

    /// Description of the value.
    const fn label(self) -> &'static str {
        match self {
            Self::Today => "a `DATETIME`, the time read on the current date",
            Self::Hours => "a `TIME`, of the hours added",
            Self::NullTime => "NULL, of type `TIME`",
            Self::NullDatetime => "NULL, of type `DATETIME`",
        }
    }
}

impl Emitter<'_> {
    /// Writes `expr`, the arithmetic `arithmetic`, between parentheses when
    /// its level is below `min`.
    pub(super) fn date_arithmetic(&mut self, expr: &Expr, arithmetic: &DateArithmetic, min: u8) {
        let today = self.interval_difference(expr, arithmetic);
        let level = match arithmetic.form {
            DateArithmeticForm::Plus | DateArithmeticForm::Minus => ADDITIVE,
            // `x` runs to the next `AND`, `OR` or `XOR`.
            DateArithmeticForm::IntervalFirst => NOT,
            _ => OPERAND,
        };
        let parenthesized = level < min;
        if parenthesized {
            self.write("(");
        }
        let operand = |emitter: &mut Self, min: u8| {
            if today {
                emitter.write("TIMESTAMP(CURRENT_DATE, ");
                emitter.expr(&arithmetic.operand, 0);
                emitter.write(")");
            } else {
                emitter.expr(&arithmetic.operand, min);
            }
        };
        match arithmetic.form {
            DateArithmeticForm::Plus | DateArithmeticForm::Minus => {
                operand(self, ADDITIVE);
                self.write(if arithmetic.form.subtracts() {
                    " - "
                } else {
                    " + "
                });
                self.interval(arithmetic);
            }
            DateArithmeticForm::IntervalFirst => {
                self.interval(arithmetic);
                self.write(" + ");
                operand(self, PREDICATE);
            }
            DateArithmeticForm::TimestampAdd => {
                self.write(&format!("TIMESTAMPADD({}, ", arithmetic.unit.keyword()));
                self.expr(&arithmetic.value, 0);
                self.write(", ");
                operand(self, 0);
                self.write(")");
            }
            form => {
                let name = match form {
                    DateArithmeticForm::DateAdd => "DATE_ADD",
                    DateArithmeticForm::DateSub => "DATE_SUB",
                    DateArithmeticForm::AddDate | DateArithmeticForm::AddDays => "ADDDATE",
                    _ => "SUBDATE",
                };
                self.write(&format!("{name}("));
                operand(self, 0);
                self.write(", ");
                if matches!(
                    form,
                    DateArithmeticForm::AddDays | DateArithmeticForm::SubDays
                ) {
                    self.expr(&arithmetic.value, 0);
                } else {
                    self.interval(arithmetic);
                }
                self.write(")");
            }
        }
        if parenthesized {
            self.write(")");
        }
    }

    /// `INTERVAL n unit`.
    fn interval(&mut self, arithmetic: &DateArithmetic) {
        self.write("INTERVAL ");
        self.expr(&arithmetic.value, 0);
        self.write(&format!(" {}", arithmetic.unit.keyword()));
    }

    /// Reports the values the output gives otherwise; `true` when the
    /// operand, a `TIME`, is written on the current date.
    fn interval_difference(&mut self, expr: &Expr, arithmetic: &DateArithmetic) -> bool {
        let unit = arithmetic.unit;
        if matches!(arithmetic.operand.ty.data_type, DataType::Time { .. }) && unit.days() {
            let input = TimeReading::of(
                self.source.dialect,
                self.input_has(Construct::TimeIntervalDatetime),
                unit,
            );
            let output = TimeReading::of(
                self.output.dialect,
                self.has(Construct::TimeIntervalDatetime),
                unit,
            );
            if input == output {
                return false;
            }
            let source = format!("{} {}", self.source.dialect.label(), self.source.version);
            if input == TimeReading::Today {
                let message = format!(
                    "a `TIME` moved by `{}` is {} in {source}, {} in {}: written `TIMESTAMP(CURRENT_DATE, t)`, of the same value",
                    unit.keyword(),
                    input.label(),
                    output.label(),
                    self.target()
                );
                self.step("interval.time", StepKind::Emulation, message, expr.span);
                return true;
            }
            let message = format!(
                "a `TIME` moved by `{}` is {} in {source}, {} in {}",
                unit.keyword(),
                input.label(),
                output.label(),
                self.target()
            );
            self.error("interval.time", &message, expr.span);
            return false;
        }
        if self.source.dialect == self.output.dialect {
            return false;
        }
        let reason = operand_difference(arithmetic)
            .or_else(|| year_difference(arithmetic))
            .or_else(|| microsecond_difference(arithmetic));
        if let Some((rule, message)) = reason {
            self.error(rule, &message, expr.span);
        }
        false
    }
}

/// Difference of the values of an operand that is not a date nor a
/// time.
fn operand_difference(arithmetic: &DateArithmetic) -> Option<(&'static str, String)> {
    match &arithmetic.operand.ty.data_type {
        DataType::Date
        | DataType::DateTime { .. }
        | DataType::Timestamp { .. }
        | DataType::Time { .. }
        | DataType::Null => None,
        DataType::Unknown => Some((
            "interval.unknown",
            "an interval moves a value whose type is unknown (a function): MySQL and MariaDB give different values for a `TIME` or a string, unknown until the types of the function catalog".to_string(),
        )),
        _ if plain_string(arithmetic) => None,
        _ => Some((
            "interval.string",
            "an interval moves a value that is not a date nor a time, read as a date and time: the string of the result has fractional seconds of 6 digits in MySQL when they are not zero, of the digits of its operands in MariaDB (`'2024-05-06 07:08:09.5' + INTERVAL 1 SECOND` gives `2024-05-06 07:08:10.500000` against `2024-05-06 07:08:10.5`); a typed literal (`TIMESTAMP '...'`) or a `CAST` gives a date and time".to_string(),
        )),
    }
}

/// Tells whether the operand is a string literal both dialects read as
/// the same date or date and time, without fraction, moved by a unit
/// that adds none.
fn plain_string(arithmetic: &DateArithmetic) -> bool {
    let ExprKind::Literal(Value::Text(text)) = &arithmetic.operand.kind else {
        return false;
    };
    let fraction = arithmetic.unit.microseconds()
        || (arithmetic.unit == TimeUnit::Second
            && !matches!(
                arithmetic.value.ty.data_type,
                DataType::Null | DataType::Boolean | DataType::Integer(_)
            ));
    !fraction && plain_temporal(text, Some(0))
}

/// Difference of a number of years MariaDB reduces to 32 bits.
fn year_difference(arithmetic: &DateArithmetic) -> Option<(&'static str, String)> {
    if arithmetic.unit != TimeUnit::Year {
        return None;
    }
    let value = &arithmetic.value;
    let bounded = match literal_number(value) {
        Some(years) => years.is_some_and(|years| years.abs() < YEAR_BITS),
        None => match &value.ty.data_type {
            DataType::Null | DataType::Boolean => true,
            DataType::Integer(integer) if integer.unsigned => matches!(
                integer.size,
                IntegerSize::Tiny | IntegerSize::Small | IntegerSize::Medium
            ),
            DataType::Integer(integer) => integer.size != IntegerSize::Big,
            DataType::Decimal(decimal) => decimal.precision() - decimal.scale() <= 9,
            _ => false,
        },
    };
    (!bounded).then(|| {
        (
            "interval.year",
            "a number of years that may reach 2^31: MariaDB reduces it to 32 bits (`INTERVAL 4294967297 YEAR` adds a year), MySQL gives NULL".to_string(),
        )
    })
}

/// Difference of the microseconds of a compound unit.
fn microsecond_difference(arithmetic: &DateArithmetic) -> Option<(&'static str, String)> {
    if !arithmetic.unit.microseconds() || arithmetic.unit == TimeUnit::Microsecond {
        return None;
    }
    let value = &arithmetic.value;
    let short = match literal_text(value) {
        Some(text) => last_digits(&text) <= MICROSECOND_DIGITS,
        None => match &value.ty.data_type {
            DataType::Null => true,
            DataType::Integer(integer) => {
                matches!(integer.size, IntegerSize::Tiny | IntegerSize::Small)
            }
            DataType::Decimal(decimal) => (1..=6).contains(&decimal.scale()),
            _ => false,
        },
    };
    (!short).then(|| {
        (
            "interval.microseconds",
            format!(
                "`{}` of microseconds that may have more than 6 digits: a number of microseconds in MySQL (`'1.1234567'` adds 2.234567 seconds), their first 6 digits in MariaDB (1.123456 seconds)",
                arithmetic.unit.keyword()
            ),
        )
    })
}

/// Rounded value of a literal number, or of the integer of a string
/// literal; `Some(None)` for a literal of another value; `None` for another
/// expression.
fn literal_number(expr: &Expr) -> Option<Option<i128>> {
    match &expr.kind {
        ExprKind::Negate(inner) => literal_number(inner).map(|value| value.map(|value| -value)),
        ExprKind::Literal(Value::Null) => Some(Some(0)),
        ExprKind::Literal(Value::Integer(value)) => Some(Some(*value)),
        ExprKind::Literal(Value::Decimal(value)) => Some(round(value.as_str())),
        ExprKind::Literal(Value::Text(text)) => Some(text.trim().parse().ok()),
        ExprKind::Literal(_) => Some(None),
        _ => None,
    }
}

/// Integer nearest to the decimal `text`, halves away from zero.
fn round(text: &str) -> Option<i128> {
    let (integer, fraction) = text.split_once('.').unwrap_or((text, ""));
    let magnitude: i128 = integer.trim_start_matches('-').parse().ok()?;
    let up = i128::from(fraction.starts_with(['5', '6', '7', '8', '9']));
    let sign = if integer.starts_with('-') { -1 } else { 1 };
    Some(sign * (magnitude + up))
}

/// Text of a literal string or number, as the server reads it in a
/// compound unit; `None` for another expression.
fn literal_text(expr: &Expr) -> Option<String> {
    match &expr.kind {
        ExprKind::Negate(inner) => literal_text(inner),
        ExprKind::Literal(Value::Null) => Some(String::new()),
        ExprKind::Literal(Value::Integer(value)) => Some(value.to_string()),
        ExprKind::Literal(Value::Decimal(value)) => Some(value.as_str().to_string()),
        ExprKind::Literal(Value::Text(text)) => Some(text.clone()),
        _ => None,
    }
}

/// Number of digits of the last group of digits of `text`: the
/// microseconds of a compound unit.
fn last_digits(text: &str) -> usize {
    text.bytes()
        .rev()
        .skip_while(|byte| !byte.is_ascii_digit())
        .take_while(u8::is_ascii_digit)
        .count()
}
