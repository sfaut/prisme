//! `EXTRACT(unit FROM x)` between MySQL and MariaDB. Both take the same part
//! of a date and time; they read a `TIME`, the day of a date by the units of
//! a day, and the strings and numbers differently
//! (`crates/probe/cases/extract*.txt`, measured on MySQL 8.0.11 and 8.4.11,
//! MariaDB 10.6.28 and 11.8.9):
//!
//! - a `TIME` by `HOUR` and its compound units: MySQL counts all its hours
//!   (`EXTRACT(HOUR FROM TIME '100:20:30')` gives 100), MariaDB the hours
//!   within a day, the days apart (4, and 4 by `DAY`), both with the sign
//!   of the time;
//! - a `TIME` by `DAY_HOUR` and the other compound units of `DAY`: MySQL
//!   gives the part of the hours (100), MariaDB the days and the hours
//!   (404);
//! - a `TIME` by the units of a date: MySQL reads the time on the current
//!   date, MariaDB gives the days by `DAY`, 0 by `MONTH`, `QUARTER`, `YEAR`
//!   and `YEAR_MONTH`, the week of the zero date by `WEEK`;
//! - a date by the compound units of `DAY`: MariaDB gives the day of the
//!   month and the part of the hours (`EXTRACT(DAY_HOUR FROM TIMESTAMP
//!   '2024-05-06 07:08:09')` gives 607), MySQL keeps the day or drops it
//!   after the expression: it keeps it for a column of a table and the date
//!   and time arithmetic, drops it for a literal, a `CAST` and the functions
//!   of the current date and time (7), and reads a column of a view, of a
//!   derived table or of a common table expression as the expression it
//!   stands for; MySQL 8.0.11 keeps it for a subquery that selects a
//!   literal, 8.4.11 drops it;
//! - a string or a number: MySQL reads a date as a time by the units of a
//!   time, of an hour or of a day (`EXTRACT(MINUTE FROM '2024-05-06')` gives
//!   20, 0 in MariaDB), MariaDB a number as hours by `DAY`; both read a date
//!   and time of 6 fractional digits at most alike, a date alike by the
//!   units of a date, any value alike by `WEEK`, `MONTH`, `QUARTER`, `YEAR`
//!   and `YEAR_MONTH`.
//!
//! The parts read otherwise are written as the input reads them, from the
//! parts both read alike (rule `function.extract`):
//!
//! - towards MariaDB, a `TIME` by `HOUR` or `DAY_HOUR`: `TIME_TO_SEC(t) DIV
//!   3600`; by the other compound units of `HOUR` and `DAY`: the part of the
//!   hours with the days added back (`EXTRACT(HOUR_MINUTE FROM t) +
//!   EXTRACT(DAY FROM t) * 2400`); by the units of a date: on the current
//!   date (`EXTRACT(MONTH FROM TIMESTAMP(CURRENT_DATE, t))`). A date by the
//!   compound units of `DAY`, whose day MySQL drops: the part of the hours
//!   (`EXTRACT(HOUR FROM e)` for `DAY_HOUR`);
//! - towards MySQL, a `TIME` by `HOUR`: `MOD(EXTRACT(HOUR FROM t), 24)`; by
//!   `DAY`: `EXTRACT(HOUR FROM t) DIV 24`; by the other units of an hour or
//!   of a day: from the hours of MySQL (`EXTRACT(HOUR_MINUTE FROM t) -
//!   EXTRACT(HOUR FROM t) DIV 24 * 2400`); by `MONTH`, `QUARTER`, `YEAR` and
//!   `YEAR_MONTH`: `EXTRACT(MINUTE FROM t) * 0`, NULL for NULL. A date by
//!   the compound units of `DAY`, but for the arithmetic whose day MySQL
//!   keeps: the day of the month and the part of the hours
//!   (`EXTRACT(DAY FROM e) * 100 + EXTRACT(HOUR FROM e)`).
//!
//! The others are errors: a `TIME` by `WEEK` towards MySQL (the week of
//! the zero date depends on the week mode), a date by the compound units
//! of `DAY` towards MariaDB whose day MySQL may keep or drop, a string or a
//! number read otherwise, a value of another type or of a type unknown.

use prisme_plan::{Expr, ExprKind, Extract, TimeUnit};
use prisme_types::{DataType, Value};

use super::Emitter;
use super::cast::plain_temporal;
use super::expr::{ADDITIVE, MULTIPLY};
use super::rules::repeatable;
use crate::Dialect;
use crate::builtins::Rule;
use crate::report::StepKind;

/// Functions of the current date and time, whose day MySQL drops by the
/// compound units of `DAY` (`LOCALTIME` and `LOCALTIMESTAMP` are `NOW`
/// there).
const CURRENT: [&str; 9] = [
    "CURDATE",
    "CURRENT_DATE",
    "CURRENT_TIMESTAMP",
    "LOCALTIME",
    "LOCALTIMESTAMP",
    "NOW",
    "SYSDATE",
    "UTC_DATE",
    "UTC_TIMESTAMP",
];

/// Units of `EXTRACT`, by how the dialects read them.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Group {
    /// Parts of an hour: `MICROSECOND`, `SECOND`, `MINUTE` and their
    /// compound units.
    Time,
    /// `HOUR` and its compound units.
    Hour,
    /// The compound units of `DAY`.
    Day,
    /// `DAY`, `WEEK`, `MONTH`, `QUARTER`, `YEAR` and `YEAR_MONTH`.
    Date,
}

impl Group {
    /// Group of `unit`.
    fn of(unit: TimeUnit) -> Self {
        match unit {
            TimeUnit::Hour
            | TimeUnit::HourMicrosecond
            | TimeUnit::HourSecond
            | TimeUnit::HourMinute => Self::Hour,
            TimeUnit::DayMicrosecond
            | TimeUnit::DaySecond
            | TimeUnit::DayMinute
            | TimeUnit::DayHour => Self::Day,
            unit if unit.time() => Self::Time,
            _ => Self::Date,
        }
    }
}

/// How the dialects read the operand of `EXTRACT`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Operand {
    /// A `DATE`, a `DATETIME` or a `TIMESTAMP`.
    Date,
    /// A `TIME`.
    Time,
    /// NULL, or a date and time literal both read alike
    /// (`'2024-05-06 07:08:09'`): the same part by every unit.
    Same,
    /// A date literal both read alike (`'2024-05-06'`), by the units of a
    /// date.
    PlainDate,
    /// Another string, or a number.
    Converted,
    /// A value of another type, whose reading was not measured.
    Other,
    /// A value of a type unknown.
    Unknown,
}

impl Operand {
    /// Reading of `operand`.
    fn of(operand: &Expr) -> Self {
        match &operand.ty.data_type {
            DataType::Date | DataType::DateTime { .. } | DataType::Timestamp { .. } => Self::Date,
            DataType::Time { .. } => Self::Time,
            DataType::Null => Self::Same,
            DataType::Unknown => Self::Unknown,
            DataType::Text(_) => match &operand.kind {
                ExprKind::Literal(Value::Text(text)) if plain_temporal(text, Some(6)) => {
                    if text.contains(' ') {
                        Self::Same
                    } else {
                        Self::PlainDate
                    }
                }
                _ => Self::Converted,
            },
            DataType::Boolean
            | DataType::Integer(_)
            | DataType::Decimal(_)
            | DataType::Float(_) => Self::Converted,
            _ => Self::Other,
        }
    }
}

/// How MySQL reads the day of a date by the compound units of `DAY`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum DayReading {
    /// It keeps the day, as MariaDB.
    Kept,
    /// It drops the day.
    Dropped,
    /// It keeps or drops it after the expression a column stands for, or
    /// for an expression not measured.
    Unsure,
}

impl DayReading {
    /// Reading of the day of `operand`, a date, by MySQL.
    fn of(operand: &Expr) -> Self {
        match &operand.kind {
            ExprKind::DateArithmetic(arithmetic)
                if Operand::of(&arithmetic.operand) == Operand::Date =>
            {
                Self::Kept
            }
            ExprKind::Literal(_) | ExprKind::Cast(_) => Self::Dropped,
            ExprKind::Function(call)
                if !call.stored
                    && CURRENT.contains(&call.name.value.to_ascii_uppercase().as_str()) =>
            {
                Self::Dropped
            }
            _ => Self::Unsure,
        }
    }
}

/// Unit of the part of the hours of `unit`, a compound unit of `DAY`:
/// `HOUR` for `DAY_HOUR`, `HOUR_MINUTE` for `DAY_MINUTE`...; `unit` itself
/// otherwise.
fn hour_unit(unit: TimeUnit) -> TimeUnit {
    match unit {
        TimeUnit::DayHour => TimeUnit::Hour,
        TimeUnit::DayMinute => TimeUnit::HourMinute,
        TimeUnit::DaySecond => TimeUnit::HourSecond,
        TimeUnit::DayMicrosecond => TimeUnit::HourMicrosecond,
        unit => unit,
    }
}

/// Value of an hour in the part of the hours of `unit`, `HOUR` or one of
/// its compound units: 1 for `HOUR`, 100 for `HOUR_MINUTE`...
fn hour_value(unit: TimeUnit) -> u64 {
    match unit {
        TimeUnit::HourMinute => 100,
        TimeUnit::HourSecond => 10_000,
        TimeUnit::HourMicrosecond => 10_000_000_000,
        _ => 1,
    }
}

impl Emitter<'_> {
    /// `EXTRACT(unit FROM x)`, at level `min` at least: natively when both
    /// dialects read the part alike, otherwise as the input reads it, or an
    /// error.
    pub(super) fn extract(&mut self, expr: &Expr, extract: &Extract, min: u8) {
        let unit = extract.unit;
        let operand = &extract.operand;
        let group = Group::of(unit);
        let reading = Operand::of(operand);
        if self.source.dialect == self.output.dialect {
            let subquery = matches!(operand.kind, ExprKind::ScalarSubquery(_));
            if self.source.dialect == Dialect::MySql
                && self.source.version != self.output.version
                && group == Group::Day
                && reading == Operand::Date
                && subquery
            {
                let message = format!(
                    "`EXTRACT({})` of a subquery: MySQL 8.0.11 keeps the day of the month of a literal it selects, 8.4.11 drops it",
                    unit.keyword()
                );
                self.error(Rule::Extract.id(), &message, expr.span);
            }
            self.extract_part(unit, operand);
            return;
        }
        let refusal = match (reading, group) {
            (Operand::Same, _)
            | (Operand::Date | Operand::Time, Group::Time)
            | (Operand::Date, Group::Hour | Group::Date)
            | (Operand::PlainDate, Group::Date) => None,
            (Operand::Converted, Group::Date) if unit != TimeUnit::Day => None,
            (Operand::Date, Group::Day) => {
                self.extract_day_of_date(expr, extract, min);
                return;
            }
            (Operand::Time, _) => {
                self.extract_of_time(expr, extract, min);
                return;
            }
            (Operand::PlainDate | Operand::Converted, _) => Some(format!(
                "`EXTRACT({})` of a string or a number: MySQL and MariaDB read it otherwise (`EXTRACT(MINUTE FROM '2024-05-06')` gives 20, 0 in MariaDB); a literal `DATE '...'`, `TIME '...'` or `TIMESTAMP '...'` is read alike",
                unit.keyword()
            )),
            (Operand::Other, _) => Some(
                "`EXTRACT` of a value of this type: its reading in MySQL and MariaDB is not established"
                    .to_owned(),
            ),
            (Operand::Unknown, _) => Some(
                "`EXTRACT` of a value whose type is unknown: MySQL and MariaDB read a `TIME`, the day of a date and a string otherwise"
                    .to_owned(),
            ),
        };
        if let Some(message) = refusal {
            self.error(Rule::Extract.id(), &message, expr.span);
        }
        self.extract_part(unit, operand);
    }

    /// `EXTRACT` of a date by a compound unit of `DAY`, towards the other
    /// dialect.
    fn extract_day_of_date(&mut self, expr: &Expr, extract: &Extract, min: u8) {
        let (unit, operand) = (extract.unit, &extract.operand);
        let keyword = unit.keyword();
        let hours = hour_unit(unit);
        let day = DayReading::of(operand);
        if day == DayReading::Kept {
            self.extract_part(unit, operand);
            return;
        }
        if self.output.dialect == Dialect::MariaDb {
            if day == DayReading::Unsure {
                let message = format!(
                    "`EXTRACT({keyword})` of a date: MySQL keeps the day of the month or drops it after the expression (a column of a table keeps it, of a view or of a derived table after the expression it stands for), MariaDB keeps it"
                );
                self.error(Rule::Extract.id(), &message, expr.span);
                self.extract_part(unit, operand);
                return;
            }
            let message = format!(
                "MySQL drops the day of the month of this expression by `{keyword}`, MariaDB keeps it: `{}` gives the part of the hours",
                hours.keyword()
            );
            self.step(Rule::Extract.id(), StepKind::Emulation, message, expr.span);
            self.extract_part(hours, operand);
            return;
        }
        if !repeatable(operand) {
            let message = format!(
                "`EXTRACT({keyword})` of a date: MySQL keeps the day of the month or drops it after the expression, and the emulation writes the expression twice, which may give two values"
            );
            self.error(Rule::Extract.id(), &message, expr.span);
            self.extract_part(unit, operand);
            return;
        }
        let message = format!(
            "MySQL drops the day of the month by `{keyword}` for some expressions, MariaDB keeps it: the day and the part of the hours are taken apart"
        );
        self.step(Rule::Extract.id(), StepKind::Emulation, message, expr.span);
        self.open(ADDITIVE, min);
        self.extract_part(TimeUnit::Day, operand);
        self.write(&format!(" * {} + ", hour_value(hours) * 100));
        self.extract_part(hours, operand);
        self.close(ADDITIVE, min);
    }

    /// `EXTRACT` of a `TIME` by a unit of an hour, of a day or of a date,
    /// towards the other dialect.
    fn extract_of_time(&mut self, expr: &Expr, extract: &Extract, min: u8) {
        let (unit, operand) = (extract.unit, &extract.operand);
        let keyword = unit.keyword();
        let group = Group::of(unit);
        let towards_mariadb = self.output.dialect == Dialect::MariaDb;
        let single = match (towards_mariadb, unit) {
            (true, TimeUnit::Hour | TimeUnit::DayHour) => true,
            (true, _) => group == Group::Date,
            (false, TimeUnit::Week) => {
                let message = "`EXTRACT(WEEK)` of a `TIME`: MariaDB gives the week of the zero date, which depends on the week mode, MySQL the week of the current date";
                self.error(Rule::Extract.id(), message, expr.span);
                self.extract_part(unit, operand);
                return;
            }
            (false, unit) => group == Group::Date || unit == TimeUnit::Hour,
        };
        if !single && !repeatable(operand) {
            let message = format!(
                "`EXTRACT({keyword})` of a `TIME`: MySQL and MariaDB count its hours otherwise, and the emulation writes the time twice, which may give two values"
            );
            self.error(Rule::Extract.id(), &message, expr.span);
            self.extract_part(unit, operand);
            return;
        }
        let message = match (towards_mariadb, group) {
            (true, Group::Date) => format!(
                "MySQL reads a `TIME` on the current date by `{keyword}`, MariaDB alone (`EXTRACT(MONTH FROM TIME '10:20:30')` gives the current month, 0 in MariaDB): `TIMESTAMP(CURRENT_DATE, t)` sets it on the current date"
            ),
            (true, _) => format!(
                "MySQL counts all the hours of a `TIME` by `{keyword}`, MariaDB those within a day, the days apart (`EXTRACT(HOUR FROM TIME '100:20:30')` gives 100, 4 in MariaDB): the days are counted as hours"
            ),
            (false, Group::Date) if unit == TimeUnit::Day => format!(
                "MariaDB gives the days of a `TIME` by `{keyword}`, MySQL reads it on the current date: the days are counted from its hours"
            ),
            (false, Group::Date) => format!(
                "MariaDB gives 0 for a `TIME` by `{keyword}`, MySQL reads it on the current date: 0, NULL for NULL"
            ),
            (false, _) => format!(
                "MariaDB counts the hours of a `TIME` within a day by `{keyword}`, the days apart, MySQL all its hours (`EXTRACT(HOUR FROM TIME '100:20:30')` gives 4, 100 in MySQL): the days are taken from its hours"
            ),
        };
        self.step(Rule::Extract.id(), StepKind::Emulation, message, expr.span);
        if towards_mariadb {
            self.extract_time_for_mariadb(unit, operand, min);
        } else {
            self.extract_time_for_mysql(unit, operand, min);
        }
    }

    /// `EXTRACT` of the `TIME` `operand` by `unit` as MySQL reads it,
    /// written for MariaDB.
    fn extract_time_for_mariadb(&mut self, unit: TimeUnit, operand: &Expr, min: u8) {
        let hours = hour_unit(unit);
        if Group::of(unit) == Group::Date {
            self.write(&format!(
                "EXTRACT({} FROM TIMESTAMP(CURRENT_DATE, ",
                unit.keyword()
            ));
            self.expr(operand, 0);
            self.write("))");
        } else if hours == TimeUnit::Hour {
            self.open(MULTIPLY, min);
            self.write("TIME_TO_SEC(");
            self.expr(operand, 0);
            self.write(") DIV 3600");
            self.close(MULTIPLY, min);
        } else {
            self.open(ADDITIVE, min);
            self.extract_part(hours, operand);
            self.write(" + ");
            self.extract_part(TimeUnit::Day, operand);
            self.write(&format!(" * {}", hour_value(hours) * 24));
            self.close(ADDITIVE, min);
        }
    }

    /// `EXTRACT` of the `TIME` `operand` by `unit` as MariaDB reads it,
    /// written for MySQL.
    fn extract_time_for_mysql(&mut self, unit: TimeUnit, operand: &Expr, min: u8) {
        let hours = hour_unit(unit);
        match unit {
            TimeUnit::Hour => {
                self.write("MOD(");
                self.extract_part(TimeUnit::Hour, operand);
                self.write(", 24)");
            }
            TimeUnit::Day => {
                self.open(MULTIPLY, min);
                self.extract_part(TimeUnit::Hour, operand);
                self.write(" DIV 24");
                self.close(MULTIPLY, min);
            }
            TimeUnit::Month | TimeUnit::Quarter | TimeUnit::Year | TimeUnit::YearMonth => {
                self.open(MULTIPLY, min);
                self.extract_part(TimeUnit::Minute, operand);
                self.write(" * 0");
                self.close(MULTIPLY, min);
            }
            _ => {
                // The days of MariaDB are the hours of MySQL divided by 24:
                // removed from the part of the hours, or written before it
                // as days, 100 parts of an hour each.
                let (sign, factor) = if Group::of(unit) == Group::Day {
                    ("+", 76)
                } else {
                    ("-", 24)
                };
                self.open(ADDITIVE, min);
                self.extract_part(hours, operand);
                self.write(&format!(" {sign} "));
                self.extract_part(TimeUnit::Hour, operand);
                self.write(&format!(" DIV 24 * {}", hour_value(hours) * factor));
                self.close(ADDITIVE, min);
            }
        }
    }

    /// `EXTRACT(unit FROM operand)`.
    fn extract_part(&mut self, unit: TimeUnit, operand: &Expr) {
        self.write(&format!("EXTRACT({} FROM ", unit.keyword()));
        self.expr(operand, 0);
        self.write(")");
    }

    /// `(` before an expression of `level` written at level `min` at least.
    fn open(&mut self, level: u8, min: u8) {
        if level < min {
            self.write("(");
        }
    }

    /// `)` after an expression of `level` written at level `min` at least.
    fn close(&mut self, level: u8, min: u8) {
        if level < min {
            self.write(")");
        }
    }
}
