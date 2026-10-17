//! Date and time arithmetic of the MySQL family: an interval added to or
//! subtracted from a date or a time (`d + INTERVAL 1 DAY`, `DATE_ADD`,
//! `ADDDATE`, `TIMESTAMPADD`), with the form written, since each form has a
//! syntax of its own and the precedence of its operands.

use crate::Expr;

/// Interval added to or subtracted from a date or a time.
#[derive(Debug, Clone, PartialEq)]
pub struct DateArithmetic {
    /// Date or time the interval moves.
    pub operand: Expr,
    /// Number of units, or the string of a compound unit (`'1:30'` for
    /// `HOUR_MINUTE`).
    pub value: Expr,
    /// Unit of the interval.
    pub unit: TimeUnit,
    /// Form written.
    pub form: DateArithmeticForm,
}

/// Form of a date and time arithmetic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DateArithmeticForm {
    /// `x + INTERVAL n unit`.
    Plus,
    /// `x - INTERVAL n unit`.
    Minus,
    /// `INTERVAL n unit + x`.
    IntervalFirst,
    /// `DATE_ADD(x, INTERVAL n unit)`.
    DateAdd,
    /// `DATE_SUB(x, INTERVAL n unit)`.
    DateSub,
    /// `ADDDATE(x, INTERVAL n unit)`.
    AddDate,
    /// `SUBDATE(x, INTERVAL n unit)`.
    SubDate,
    /// `ADDDATE(x, n)`: `n` days.
    AddDays,
    /// `SUBDATE(x, n)`: `n` days.
    SubDays,
    /// `TIMESTAMPADD(unit, n, x)`.
    TimestampAdd,
}

impl DateArithmeticForm {
    /// Tells whether the form subtracts the interval.
    pub const fn subtracts(self) -> bool {
        matches!(
            self,
            Self::Minus | Self::DateSub | Self::SubDate | Self::SubDays
        )
    }
}

/// Unit of an interval, simple or compound (`HOUR_MINUTE`: hours and
/// minutes, written `'1:30'`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TimeUnit {
    /// `MICROSECOND`.
    Microsecond,
    /// `SECOND`.
    Second,
    /// `MINUTE`.
    Minute,
    /// `HOUR`.
    Hour,
    /// `DAY`.
    Day,
    /// `WEEK`: 7 days.
    Week,
    /// `MONTH`.
    Month,
    /// `QUARTER`: 3 months.
    Quarter,
    /// `YEAR`.
    Year,
    /// `SECOND_MICROSECOND`.
    SecondMicrosecond,
    /// `MINUTE_MICROSECOND`.
    MinuteMicrosecond,
    /// `MINUTE_SECOND`.
    MinuteSecond,
    /// `HOUR_MICROSECOND`.
    HourMicrosecond,
    /// `HOUR_SECOND`.
    HourSecond,
    /// `HOUR_MINUTE`.
    HourMinute,
    /// `DAY_MICROSECOND`.
    DayMicrosecond,
    /// `DAY_SECOND`.
    DaySecond,
    /// `DAY_MINUTE`.
    DayMinute,
    /// `DAY_HOUR`.
    DayHour,
    /// `YEAR_MONTH`.
    YearMonth,
}

impl TimeUnit {
    /// Every unit, the simple ones first.
    pub const ALL: [Self; 20] = [
        Self::Microsecond,
        Self::Second,
        Self::Minute,
        Self::Hour,
        Self::Day,
        Self::Week,
        Self::Month,
        Self::Quarter,
        Self::Year,
        Self::SecondMicrosecond,
        Self::MinuteMicrosecond,
        Self::MinuteSecond,
        Self::HourMicrosecond,
        Self::HourSecond,
        Self::HourMinute,
        Self::DayMicrosecond,
        Self::DaySecond,
        Self::DayMinute,
        Self::DayHour,
        Self::YearMonth,
    ];

    /// Keyword of the unit.
    pub const fn keyword(self) -> &'static str {
        match self {
            Self::Microsecond => "MICROSECOND",
            Self::Second => "SECOND",
            Self::Minute => "MINUTE",
            Self::Hour => "HOUR",
            Self::Day => "DAY",
            Self::Week => "WEEK",
            Self::Month => "MONTH",
            Self::Quarter => "QUARTER",
            Self::Year => "YEAR",
            Self::SecondMicrosecond => "SECOND_MICROSECOND",
            Self::MinuteMicrosecond => "MINUTE_MICROSECOND",
            Self::MinuteSecond => "MINUTE_SECOND",
            Self::HourMicrosecond => "HOUR_MICROSECOND",
            Self::HourSecond => "HOUR_SECOND",
            Self::HourMinute => "HOUR_MINUTE",
            Self::DayMicrosecond => "DAY_MICROSECOND",
            Self::DaySecond => "DAY_SECOND",
            Self::DayMinute => "DAY_MINUTE",
            Self::DayHour => "DAY_HOUR",
            Self::YearMonth => "YEAR_MONTH",
        }
    }

    /// Tells whether the unit is simple: a single part, the only units of
    /// `TIMESTAMPADD` and `TIMESTAMPDIFF`.
    pub const fn simple(self) -> bool {
        matches!(
            self,
            Self::Microsecond
                | Self::Second
                | Self::Minute
                | Self::Hour
                | Self::Day
                | Self::Week
                | Self::Month
                | Self::Quarter
                | Self::Year
        )
    }

    /// Tells whether the unit counts microseconds: `MICROSECOND` and the
    /// compound units that end with it.
    pub const fn microseconds(self) -> bool {
        matches!(
            self,
            Self::Microsecond
                | Self::SecondMicrosecond
                | Self::MinuteMicrosecond
                | Self::HourMicrosecond
                | Self::DayMicrosecond
        )
    }

    /// Tells whether the unit counts days or longer periods: `DAY`, `WEEK`,
    /// `MONTH`, `QUARTER`, `YEAR` and the compound units that start with
    /// `DAY` or `YEAR`.
    pub const fn days(self) -> bool {
        matches!(
            self,
            Self::Day
                | Self::Week
                | Self::Month
                | Self::Quarter
                | Self::Year
                | Self::DayMicrosecond
                | Self::DaySecond
                | Self::DayMinute
                | Self::DayHour
                | Self::YearMonth
        )
    }

    /// Tells whether the unit counts parts of a day: hours, minutes,
    /// seconds or microseconds.
    pub const fn time(self) -> bool {
        !matches!(
            self,
            Self::Day | Self::Week | Self::Month | Self::Quarter | Self::Year | Self::YearMonth
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn units_split_into_days_and_parts_of_a_day() {
        // A compound unit of `DAY` counts both.
        let both: Vec<TimeUnit> = TimeUnit::ALL
            .into_iter()
            .filter(|unit| unit.days() && unit.time())
            .collect();
        assert_eq!(
            both,
            [
                TimeUnit::DayMicrosecond,
                TimeUnit::DaySecond,
                TimeUnit::DayMinute,
                TimeUnit::DayHour
            ]
        );
        assert!(
            TimeUnit::ALL
                .into_iter()
                .all(|unit| unit.days() || unit.time())
        );
        assert_eq!(
            TimeUnit::ALL
                .into_iter()
                .filter(|unit| unit.simple())
                .count(),
            9
        );
        assert!(DateArithmeticForm::SubDays.subtracts());
        assert!(!DateArithmeticForm::IntervalFirst.subtracts());
    }
}
