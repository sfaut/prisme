//! Dates of a level of the Pivoteur: what a level keeps of the dates of its
//! column, its precision (year, quarter, month, week, day, then an instant to
//! the hour, the minute, the second, the millisecond or the microsecond), one
//! group per period, in the order of time.
//!
//! A date is read in the text of its cell, by the civil calendar. A precision of
//! the day or coarser leaves out the time and the time zone that may follow it.
//! A precision of the time reads the time as it is written, `HH:MM:SS` and its
//! fraction, truncated to its unit, as `DATE_TRUNC` does: `10:30:15.9` falls in
//! the second `10:30:15`. It leaves out the time zone. A date written without a
//! time falls at midnight, `2026-01-15` in the hour `2026-01-15 00:00:00`; a time
//! that is not one of the day, such as `25:00:00`, has no period at such a
//! precision.
//!
//! The calendar holds the years 0001 to 9999, as in ANSI SQL. A cell outside of
//! it has no period, and stops the computation when a row taken into account
//! holds it: a date without a day in the calendar, which MySQL and MariaDB can
//! hold (`0000-00-00`, `2026-03-00`, `2026-02-30`) and which the servers read
//! each in their own way (`docs/SQL-COMPARISON.md`, section 3.12), a date before
//! the Christian era (PostgreSQL, DuckDB), `infinity`, any other text.
//!
//! A filter compares dates, instants, hours and periods as their text, once their
//! form is checked (`comparable`, `is_label`): that form makes the order of the
//! text the order of time.

use prisme_store::{Dictionary, FxMap, TextColumn};

use crate::Family;

/// What a level keeps of the dates of its column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Precision {
    /// ISO year, that of the ISO week of the date: `2026` for 2027-01-01.
    Year,
    /// Quarter of the calendar: `2026-T2`.
    Quarter,
    /// Month of the calendar: `2026-01`.
    Month,
    /// ISO week, which starts on Monday: `2026-S53` for 2027-01-01.
    Week,
    /// Day, without the time: `2026-01-15`.
    Date,
    /// Hour of an instant, its minutes and seconds at 0: `2026-01-15 10:00:00`.
    Hour,
    /// Minute of an instant, its seconds at 0: `2026-01-15 10:30:00`.
    Minute,
    /// Second of an instant, without its fraction: `2026-01-15 10:30:15`.
    Second,
    /// Millisecond of an instant, as a `TIMESTAMP(3)`: `2026-01-15 10:30:15.123`.
    Millisecond,
    /// Microsecond of an instant, as a `TIMESTAMP(6)`:
    /// `2026-01-15 10:30:15.123456`.
    Microsecond,
}

impl Precision {
    /// Number of precisions.
    pub const COUNT: usize = 10;

    /// Microseconds of the unit of a precision of the time, `None` for a
    /// precision of the day or coarser.
    fn unit(self) -> Option<i64> {
        match self {
            Precision::Hour => Some(3_600_000_000),
            Precision::Minute => Some(60_000_000),
            Precision::Second => Some(1_000_000),
            Precision::Millisecond => Some(1_000),
            Precision::Microsecond => Some(1),
            _ => None,
        }
    }

    /// Digits of the fraction of the seconds a label of this precision writes.
    fn fraction_digits(self) -> usize {
        match self {
            Precision::Millisecond => 3,
            Precision::Microsecond => 6,
            _ => 0,
        }
    }
}

/// Period of a date: its year, then its rank in the year; for a precision of the
/// time, its day (`YYYYMMDD`), then its microseconds in the day, truncated to
/// the unit. Periods sort in the order of time.
type Period = (i64, i64);

/// Number written by a field of digits.
fn digits(field: &str) -> Option<i64> {
    if field.bytes().all(|byte| byte.is_ascii_digit()) {
        field.parse::<i32>().ok().map(i64::from)
    } else {
        None
    }
}

/// Days from 1970-01-01 to a day of the civil calendar, proleptic Gregorian
/// (`days_from_civil`, by Howard Hinnant).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let year_of_era = year.rem_euclid(400);
    let shifted_month = if month > 2 { month - 3 } else { month + 9 };
    let day_of_year = (153 * shifted_month + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    year.div_euclid(400) * 146_097 + day_of_era - 719_468
}

/// Number of days of a month.
fn days_in_month(year: i64, month: i64) -> i64 {
    let (next_year, next_month) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    days_from_civil(next_year, next_month, 1) - days_from_civil(year, month, 1)
}

/// Microseconds in the day of a time written `HH:MM:SS`, then an optional
/// fraction, then anything, such as a time zone: the fraction is truncated to
/// the microsecond. `None` without such a time, or for an hour out of the day.
fn time_of_day(text: &str) -> Option<i64> {
    let field = |range: std::ops::Range<usize>, largest: i64| {
        text.get(range)
            .and_then(digits)
            .filter(|&value| value <= largest)
    };
    let separated = text.get(2..3) == Some(":") && text.get(5..6) == Some(":");
    let (hour, minute, second) = (field(0..2, 23)?, field(3..5, 59)?, field(6..8, 59)?);
    if !separated {
        return None;
    }
    let fraction = match text[8..].strip_prefix('.') {
        Some(rest) => {
            let digits_end = rest
                .find(|character: char| !character.is_ascii_digit())
                .unwrap_or(rest.len());
            if digits_end == 0 {
                return None;
            }
            let kept = &rest[..digits_end.min(6)];
            digits(kept)? * 10_i64.pow(6 - kept.len() as u32)
        }
        None => 0,
    };
    Some(((hour * 60 + minute) * 60 + second) * 1_000_000 + fraction)
}

/// Date of the calendar written in a cell, with its time when one follows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CellDate {
    /// Year.
    year: i64,
    /// Month.
    month: i64,
    /// Day.
    day: i64,
    /// Microseconds in the day: 0, midnight, for a date written without a time;
    /// `None` for a time that is not one of the day.
    time: Option<i64>,
}

impl CellDate {
    /// Reads the date at the start of a cell: `YYYY-MM-DD`, then nothing or a
    /// time. `None` for any other text, and for a date outside of the calendar.
    fn parse(cell: &str) -> Option<Self> {
        let (year, rest) = cell.split_once('-')?;
        let (month, rest) = rest.split_once('-')?;
        let day = rest.get(..2)?;
        let time = &rest[2..];
        let time_follows =
            time.is_empty() || matches!(time.as_bytes(), [b' ' | b'T', b'0'..=b'9', ..]);
        if year.len() < 4 || month.len() != 2 || !time_follows || time.ends_with("BC") {
            return None;
        }
        let (year, month, day) = (digits(year)?, digits(month)?, digits(day)?);
        let in_calendar = (1..=9999).contains(&year)
            && (1..=12).contains(&month)
            && (1..=days_in_month(year, month)).contains(&day);
        let time = match time.get(1..) {
            Some(written) => time_of_day(written),
            None => Some(0),
        };
        in_calendar.then_some(CellDate {
            year,
            month,
            day,
            time,
        })
    }

    /// ISO year and ISO week: a week starts on Monday, and belongs to the year
    /// of its Thursday.
    fn iso_week(self) -> Period {
        let first_day = |year| days_from_civil(year, 1, 1);
        let days = days_from_civil(self.year, self.month, self.day);
        // 1970-01-01 was a Thursday.
        let thursday = days - (days + 3).rem_euclid(7) + 3;
        let year = if thursday < first_day(self.year) {
            self.year - 1
        } else if thursday >= first_day(self.year + 1) {
            self.year + 1
        } else {
            self.year
        };
        (year, (thursday - first_day(year)) / 7 + 1)
    }

    /// Period of the date at a precision; `None` for a precision of the time
    /// when the time written is not one of the day.
    fn period(self, precision: Precision) -> Option<Period> {
        if let Some(unit) = precision.unit() {
            let day = (self.year * 100 + self.month) * 100 + self.day;
            return self.time.map(|time| (day, time - time % unit));
        }
        Some(match precision {
            Precision::Year => (self.iso_week().0, 0),
            Precision::Quarter => (self.year, (self.month + 2) / 3),
            Precision::Month => (self.year, self.month),
            Precision::Week => self.iso_week(),
            _ => (self.year, self.month * 100 + self.day),
        })
    }
}

/// Is a text an hour of the day, `HH:MM:SS` with an optional fraction? MySQL and
/// MariaDB can hold others, such as `25:00:00` or `-120:00:00`.
fn is_time(text: &str) -> bool {
    let (clock, fraction) = match text.split_once('.') {
        Some((clock, fraction)) => (clock, Some(fraction)),
        None => (text, None),
    };
    let mut fields = clock.split(':');
    let mut field = |largest: i64| {
        fields
            .next()
            .filter(|field| field.len() == 2)
            .and_then(digits)
            .is_some_and(|value| value <= largest)
    };
    field(23)
        && field(59)
        && field(59)
        && clock.len() == 8
        && fraction.is_none_or(|digits| {
            !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())
        })
}

/// Text of a date, an instant or an hour that compares in the order of time: the
/// cell without the zeros that end its fraction, so that `10:30:00.50` equals
/// `10:30:00.5`. `None` for a text outside of the calendar or of the day.
pub fn comparable(family: Family, cell: &str) -> Option<&str> {
    let date = |text: &str| text.len() == 10 && CellDate::parse(text).is_some();
    let valid = match family {
        Family::Date => date(cell),
        Family::Datetime => {
            date(cell.get(..10)?) && cell.get(10..11)? == " " && cell.get(11..).is_some_and(is_time)
        }
        Family::Time => is_time(cell),
        _ => true,
    };
    valid.then(|| {
        if family != Family::Date && cell.contains('.') {
            cell.trim_end_matches('0').trim_end_matches('.')
        } else {
            cell
        }
    })
}

/// Is a text a label a precision can give? A period that does not hold a date of
/// a column, such as a week 53 of a year of 52 weeks, is one: it keeps nothing.
pub fn is_label(precision: Precision, text: &str) -> bool {
    let (Some(year), Some(rest)) = (text.get(..4), text.get(4..)) else {
        return false;
    };
    let in_range = |field: Option<&str>, length: usize, largest: i64| {
        field
            .filter(|field| field.len() == length)
            .and_then(digits)
            .is_some_and(|value| (1..=largest).contains(&value))
    };
    in_range(Some(year), 4, 9999)
        && match precision {
            Precision::Year => rest.is_empty(),
            Precision::Quarter => in_range(rest.strip_prefix("-T"), 1, 4),
            Precision::Month => in_range(rest.strip_prefix('-'), 2, 12),
            Precision::Week => in_range(rest.strip_prefix("-S"), 2, 53),
            Precision::Date => text.len() == 10 && CellDate::parse(text).is_some(),
            _ => is_instant_label(precision, text),
        }
}

/// Is a text the label of an instant at a precision of the time: a day of the
/// calendar, a space, `HH:MM:SS` with the fields below the unit at 0, then the
/// digits of the fraction the precision writes?
fn is_instant_label(precision: Precision, text: &str) -> bool {
    let (Some(unit), Some(time)) = (precision.unit(), text.get(11..)) else {
        return false;
    };
    let written = match precision.fraction_digits() {
        0 => time.len() == 8,
        digits => {
            time.len() == 9 + digits
                && time.as_bytes()[8] == b'.'
                && time.bytes().skip(9).all(|byte| byte.is_ascii_digit())
        }
    };
    text.get(..10)
        .is_some_and(|date| CellDate::parse(date).is_some())
        && text.as_bytes()[10] == b' '
        && written
        && time_of_day(time).is_some_and(|micros| micros % unit == 0)
}

/// Label of a period.
fn label(precision: Precision, (year, rank): Period) -> String {
    let day = |date: i64| {
        format!(
            "{:04}-{:02}-{:02}",
            date / 10_000,
            date / 100 % 100,
            date % 100
        )
    };
    match precision {
        Precision::Year => format!("{year:04}"),
        Precision::Quarter => format!("{year:04}-T{rank}"),
        Precision::Month => format!("{year:04}-{rank:02}"),
        Precision::Week => format!("{year:04}-S{rank:02}"),
        Precision::Date => format!("{year:04}-{:02}-{:02}", rank / 100, rank % 100),
        _ => {
            let seconds = rank / 1_000_000;
            let clock = format!(
                "{} {:02}:{:02}:{:02}",
                day(year),
                seconds / 3600,
                seconds / 60 % 60,
                seconds % 60
            );
            match precision.fraction_digits() {
                0 => clock,
                digits => {
                    let fraction = rank % 1_000_000 / 10_i64.pow(6 - digits as u32);
                    format!("{clock}.{fraction:0digits$}")
                }
            }
        }
    }
}

/// What a cell gives at a precision, in the order of the groups: NULL first, as
/// in the grid, the cells outside of the calendar, then the periods.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum Slot {
    /// NULL.
    Null,
    /// A cell outside of the calendar.
    Outside,
    /// A period.
    Period(Period),
}

/// Level of a column at a precision, built from the dictionary of the column:
/// one read per distinct value, then one code per row.
#[derive(Debug)]
pub struct DerivedLevel {
    /// Code of each row, one code per period, NULL included.
    pub dictionary: Dictionary,
    /// Rank of each code in the order of time, NULL first as in the grid.
    pub ranks: Vec<u32>,
    /// Label of each code, `None` for NULL.
    labels: Vec<Option<String>>,
    /// Code of the cells outside of the calendar, `None` without any.
    outside: Option<u32>,
}

impl DerivedLevel {
    /// Reads the dates of a column.
    pub fn of(cells: &TextColumn, values: &Dictionary, precision: Precision) -> Self {
        let mut index: FxMap<Slot, u32> = FxMap::default();
        let mut slots = Vec::new();
        let mut first_rows = Vec::new();
        let mut code_of_value = Vec::with_capacity(values.len());
        for &row in &values.first_rows {
            let slot = match cells.get(row as usize) {
                Some(cell) => CellDate::parse(cell)
                    .and_then(|date| date.period(precision))
                    .map_or(Slot::Outside, Slot::Period),
                None => Slot::Null,
            };
            code_of_value.push(*index.entry(slot).or_insert_with(|| {
                slots.push(slot);
                first_rows.push(row);
                (slots.len() - 1) as u32
            }));
        }
        let mut order: Vec<u32> = (0..slots.len() as u32).collect();
        order.sort_unstable_by_key(|&code| slots[code as usize]);
        DerivedLevel {
            dictionary: Dictionary {
                codes: values
                    .codes
                    .iter()
                    .map(|&value| code_of_value[value as usize])
                    .collect(),
                first_rows,
            },
            ranks: crate::ranks_of(&order),
            labels: slots
                .iter()
                .map(|slot| match slot {
                    Slot::Period(period) => Some(label(precision, *period)),
                    _ => None,
                })
                .collect(),
            outside: index.get(&Slot::Outside).copied(),
        }
    }

    /// First of some rows whose cell is outside of the calendar.
    pub fn first_outside(&self, rows: &[u32]) -> Option<u32> {
        let code = self.outside?;
        rows.iter()
            .copied()
            .find(|&row| self.dictionary.codes[row as usize] == code)
    }

    /// Label of the period of a row, `None` for NULL.
    pub fn label(&self, row: u32) -> Option<&str> {
        self.code_label(self.dictionary.codes[row as usize])
    }

    /// Label of a code, `None` for NULL or for the cells outside of the calendar.
    pub fn code_label(&self, code: u32) -> Option<&str> {
        self.labels[code as usize].as_deref()
    }

    /// Is this the code of the cells outside of the calendar?
    pub fn is_outside(&self, code: u32) -> bool {
        self.outside == Some(code)
    }

    /// Labels of the periods in the order of time, NULL first, without the cells
    /// outside of the calendar, which have no period to offer.
    pub fn listed(&self) -> Vec<Option<&str>> {
        let mut codes: Vec<u32> = (0..self.labels.len() as u32)
            .filter(|&code| !self.is_outside(code))
            .collect();
        codes.sort_unstable_by_key(|&code| self.ranks[code as usize]);
        codes.iter().map(|&code| self.code_label(code)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Level of cells at a precision.
    fn level(cells: &[Option<&str>], precision: Precision) -> DerivedLevel {
        let mut column = TextColumn::default();
        for &cell in cells {
            column.push(cell);
        }
        DerivedLevel::of(&column, &Dictionary::of(&column), precision)
    }

    /// Label of each cell at a precision.
    fn labels(cells: &[&str], precision: Precision) -> Vec<Option<String>> {
        let cells: Vec<_> = cells.iter().map(|&cell| Some(cell)).collect();
        let level = level(&cells, precision);
        (0..cells.len() as u32)
            .map(|row| level.label(row).map(str::to_owned))
            .collect()
    }

    /// The label of a single date.
    fn single(cell: &str, precision: Precision) -> Option<String> {
        labels(&[cell], precision).remove(0)
    }

    #[test]
    fn a_date_gives_the_label_of_each_precision() {
        let cell = "2026-05-15 10:30:00";
        assert_eq!(single(cell, Precision::Year).unwrap(), "2026");
        assert_eq!(single(cell, Precision::Quarter).unwrap(), "2026-T2");
        assert_eq!(single(cell, Precision::Month).unwrap(), "2026-05");
        assert_eq!(single(cell, Precision::Week).unwrap(), "2026-S20");
        assert_eq!(single(cell, Precision::Date).unwrap(), "2026-05-15");
    }

    #[test]
    fn an_instant_gives_the_label_of_each_precision_of_the_time() {
        let cell = "2026-05-15 10:30:15.123456";
        assert_eq!(
            single(cell, Precision::Hour).unwrap(),
            "2026-05-15 10:00:00"
        );
        assert_eq!(
            single(cell, Precision::Minute).unwrap(),
            "2026-05-15 10:30:00"
        );
        assert_eq!(
            single(cell, Precision::Second).unwrap(),
            "2026-05-15 10:30:15"
        );
        assert_eq!(
            single(cell, Precision::Millisecond).unwrap(),
            "2026-05-15 10:30:15.123"
        );
        assert_eq!(
            single(cell, Precision::Microsecond).unwrap(),
            "2026-05-15 10:30:15.123456"
        );
        assert_eq!(single(cell, Precision::Date).unwrap(), "2026-05-15");
    }

    #[test]
    fn a_precision_of_the_time_truncates_and_writes_its_digits() {
        for (cell, precision, expected) in [
            // Truncated, never rounded: the last microsecond of the year stays in it.
            (
                "2026-12-31 23:59:59.999999",
                Precision::Millisecond,
                "2026-12-31 23:59:59.999",
            ),
            (
                "2026-12-31 23:59:59.9",
                Precision::Second,
                "2026-12-31 23:59:59",
            ),
            (
                "2026-12-31 23:59:59",
                Precision::Hour,
                "2026-12-31 23:00:00",
            ),
            // The digits a precision writes, whatever the cell holds.
            (
                "2026-01-15 10:30:15",
                Precision::Millisecond,
                "2026-01-15 10:30:15.000",
            ),
            (
                "2026-01-15 10:30:15.5",
                Precision::Microsecond,
                "2026-01-15 10:30:15.500000",
            ),
            (
                "2026-01-15 10:30:15.000000",
                Precision::Second,
                "2026-01-15 10:30:15",
            ),
            // Beyond the microsecond, as DuckDB's TIMESTAMP_NS.
            (
                "2026-01-15 10:30:15.123456789",
                Precision::Microsecond,
                "2026-01-15 10:30:15.123456",
            ),
            // The time as written, its zone left out.
            (
                "2026-01-15T10:30:15+02:00",
                Precision::Minute,
                "2026-01-15 10:30:00",
            ),
            (
                "2026-01-15 00:00:00",
                Precision::Hour,
                "2026-01-15 00:00:00",
            ),
        ] {
            assert_eq!(
                single(cell, precision).unwrap(),
                expected,
                "{cell} {precision:?}"
            );
        }
    }

    #[test]
    fn instants_group_by_their_unit_in_the_order_of_time() {
        let cells = [
            Some("2026-01-15 10:59:59.999999"),
            Some("2026-01-15 09:00:00"),
            Some("2026-01-15 10:00:00"),
            Some("2025-12-31 23:30:00"),
        ];
        let level = level(&cells, Precision::Hour);
        assert_eq!(level.dictionary.codes, [0, 1, 0, 2]);
        assert_eq!(
            level.listed(),
            [
                Some("2025-12-31 23:00:00"),
                Some("2026-01-15 09:00:00"),
                Some("2026-01-15 10:00:00")
            ]
        );
    }

    #[test]
    fn a_date_without_a_time_falls_at_midnight() {
        assert_eq!(
            single("2026-01-15", Precision::Hour).unwrap(),
            "2026-01-15 00:00:00"
        );
        assert_eq!(
            single("2026-01-15", Precision::Microsecond).unwrap(),
            "2026-01-15 00:00:00.000000"
        );
        let cells = [Some("2026-01-15 00:59:59"), Some("2026-01-15")];
        let level = level(&cells, Precision::Hour);
        assert_eq!(level.dictionary.codes, [0, 0]);
        assert_eq!(level.first_outside(&[0, 1]), None);
    }

    #[test]
    fn a_time_out_of_the_day_has_no_period_of_the_time() {
        for cell in [
            "2026-01-15 25:00:00",
            "2026-01-15 10:60:00",
            "2026-01-15 10:30",
            "2026-01-15 10:30:15.",
            "2026-01-15 10-30-15",
        ] {
            let cells = [Some("2026-01-15 10:30:00"), Some(cell)];
            let seconds = level(&cells, Precision::Second);
            assert_eq!(seconds.first_outside(&[0, 1]), Some(1), "«{cell}»");
            // A precision of the day reads its date.
            assert_eq!(
                level(&cells, Precision::Date).first_outside(&[0, 1]),
                None,
                "«{cell}»"
            );
        }
    }

    #[test]
    fn each_month_falls_in_its_quarter() {
        let quarters: Vec<_> = (1..=12)
            .map(|month| single(&format!("2026-{month:02}-01"), Precision::Quarter).unwrap())
            .collect();
        assert_eq!(
            quarters,
            [
                "2026-T1", "2026-T1", "2026-T1", "2026-T2", "2026-T2", "2026-T2", "2026-T3",
                "2026-T3", "2026-T3", "2026-T4", "2026-T4", "2026-T4",
            ]
        );
    }

    #[test]
    fn iso_weeks_match_the_iso_calendar() {
        // Values of `date.isocalendar()` in Python.
        for (date, week) in [
            ("2015-12-31", "2015-S53"),
            ("2016-01-03", "2015-S53"),
            ("2018-12-31", "2019-S01"),
            ("2019-12-29", "2019-S52"),
            ("2019-12-30", "2020-S01"),
            ("2020-12-31", "2020-S53"),
            ("2021-01-03", "2020-S53"),
            ("2021-01-04", "2021-S01"),
            ("2024-12-29", "2024-S52"),
            ("2024-12-30", "2025-S01"),
            ("2025-12-28", "2025-S52"),
            ("2025-12-29", "2026-S01"),
            ("2026-01-15", "2026-S03"),
            ("2026-12-31", "2026-S53"),
            ("2027-01-01", "2026-S53"),
            ("2027-01-03", "2026-S53"),
            ("2027-01-04", "2027-S01"),
            ("2000-02-29", "2000-S09"),
            ("1999-01-01", "1998-S53"),
            ("1600-01-01", "1599-S52"),
            ("9999-12-31", "9999-S52"),
        ] {
            assert_eq!(single(date, Precision::Week).unwrap(), week, "{date}");
        }
    }

    #[test]
    fn the_iso_year_is_the_year_of_the_iso_week() {
        assert_eq!(single("2027-01-01", Precision::Year).unwrap(), "2026");
        assert_eq!(single("2027-01-04", Precision::Year).unwrap(), "2027");
        assert_eq!(single("2025-12-29", Precision::Year).unwrap(), "2026");
    }

    #[test]
    fn the_week_changes_on_mondays_only() {
        let mut previous = None;
        for year in 1999..=2031 {
            for month in 1..=12 {
                for day in 1..=days_in_month(year, month) {
                    let date = CellDate {
                        year,
                        month,
                        day,
                        time: None,
                    };
                    let week = date.iso_week();
                    let monday = (days_from_civil(year, month, day) + 3).rem_euclid(7) == 0;
                    if let Some(previous) = previous {
                        assert_eq!(week != previous, monday, "{date:?}");
                    }
                    previous = Some(week);
                }
            }
        }
    }

    #[test]
    fn periods_group_their_rows_and_follow_the_order_of_time() {
        let cells = [
            Some("2026-03-15 08:00:00"),
            Some("2025-12-31"),
            None,
            Some("2026-03-01T23:59:59+02:00"),
            Some("2026-01-15"),
        ];
        let level = level(&cells, Precision::Month);
        assert_eq!(level.dictionary.codes, [0, 1, 2, 0, 3]);
        assert_eq!(level.dictionary.first_rows, [0, 1, 2, 4]);
        // NULL, 2025-12, 2026-01, 2026-03.
        assert_eq!(level.ranks, [3, 1, 0, 2]);
        assert_eq!(level.label(3), Some("2026-03"));
        assert_eq!(level.label(2), None);
    }

    #[test]
    fn the_first_and_the_last_year_of_the_calendar_are_read() {
        assert_eq!(single("0001-01-01", Precision::Week).unwrap(), "0001-S01");
        assert_eq!(single("9999-12-31", Precision::Month).unwrap(), "9999-12");
    }

    #[test]
    fn a_cell_outside_of_the_calendar_gives_its_row() {
        for cell in [
            "0000-00-00",
            "2026-03-00",
            "2026-00-15",
            "2026-02-30",
            "2025-02-29",
            "0000-01-01",
            "12345-06-07",
            "infinity",
            "Nord",
            "",
            "2026",
            "2026-01",
            "26-01-15",
            "2026-1-15",
            "2026-13-01",
            "2026-01-32",
            "2026-01-15x",
            "2026-01-15 BC",
            "0044-03-15 12:00:00 BC",
            "0001-01-01 (BC)",
            "-2026-01-15",
            "+026-01-15",
            "99999999999-01-15",
        ] {
            for precision in [
                Precision::Year,
                Precision::Quarter,
                Precision::Month,
                Precision::Week,
                Precision::Date,
                Precision::Hour,
                Precision::Microsecond,
            ] {
                let cells = [Some("2026-01-15 10:30:00"), None, Some(cell)];
                let level = level(&cells, precision);
                assert_eq!(level.first_outside(&[0, 1, 2]), Some(2), "«{cell}»");
            }
        }
    }

    #[test]
    fn only_the_rows_given_can_be_outside_of_the_calendar() {
        let cells = [Some("0000-00-00"), Some("2026-01-15"), Some("2026-03-00")];
        let level = level(&cells, Precision::Month);
        assert_eq!(level.first_outside(&[1]), None);
        assert_eq!(level.first_outside(&[1, 2, 0]), Some(2));
    }

    #[test]
    fn a_cell_outside_of_the_calendar_groups_apart_from_null() {
        let cells = [Some("2026-01-15"), Some("0000-00-00"), None, Some("x")];
        let level = level(&cells, Precision::Month);
        assert_eq!(level.dictionary.codes, [0, 1, 2, 1]);
        // NULL, outside of the calendar, 2026-01.
        assert_eq!(level.ranks, [2, 1, 0]);
    }

    #[test]
    fn the_periods_are_listed_in_the_order_of_time_without_the_cells_outside() {
        let cells = [
            Some("2026-03-15"),
            Some("0000-00-00"),
            None,
            Some("2026-01-15"),
        ];
        assert_eq!(
            level(&cells, Precision::Month).listed(),
            [None, Some("2026-01"), Some("2026-03")]
        );
    }

    #[test]
    fn a_date_an_instant_or_an_hour_compares_in_its_form() {
        for (family, cell, expected) in [
            (Family::Date, "2026-01-15", Some("2026-01-15")),
            (Family::Date, "0000-00-00", None),
            (Family::Date, "2026-02-30", None),
            (Family::Date, "2026-01-15 10:00:00", None),
            (Family::Date, "infinity", None),
            (
                Family::Datetime,
                "2026-01-15 10:30:00",
                Some("2026-01-15 10:30:00"),
            ),
            (
                Family::Datetime,
                "2026-01-15 10:30:00.500",
                Some("2026-01-15 10:30:00.5"),
            ),
            (
                Family::Datetime,
                "2026-01-15 10:30:00.000",
                Some("2026-01-15 10:30:00"),
            ),
            (Family::Datetime, "2026-01-15", None),
            (Family::Datetime, "0000-00-00 00:00:00", None),
            (Family::Datetime, "2026-01-15T10:30:00", None),
            (Family::Datetime, "2026-01-15 10:30:00+01", None),
            (Family::Time, "10:30:00", Some("10:30:00")),
            (Family::Time, "10:30:00.10", Some("10:30:00.1")),
            (Family::Time, "00:00:00", Some("00:00:00")),
            (Family::Time, "23:59:59", Some("23:59:59")),
            (Family::Time, "25:00:00", None),
            (Family::Time, "-12:00:00", None),
            (Family::Time, "100:00:00", None),
            (Family::Time, "10:60:00", None),
            (Family::Time, "10:30", None),
            (Family::Time, "10:30:00.", None),
            (Family::Time, "10:30:00.x", None),
        ] {
            assert_eq!(comparable(family, cell), expected, "{family:?} «{cell}»");
        }
    }

    #[test]
    fn a_label_has_the_form_of_its_precision() {
        for (precision, text, expected) in [
            (Precision::Year, "2026", true),
            (Precision::Year, "0000", false),
            (Precision::Year, "2026-01", false),
            (Precision::Year, "26", false),
            (Precision::Quarter, "2026-T1", true),
            (Precision::Quarter, "2026-T5", false),
            (Precision::Quarter, "2026-1", false),
            (Precision::Month, "2026-12", true),
            (Precision::Month, "2026-13", false),
            (Precision::Month, "2026-1", false),
            (Precision::Week, "2026-S53", true),
            (Precision::Week, "2026-S54", false),
            (Precision::Week, "2026-S00", false),
            (Precision::Week, "2026-03", false),
            (Precision::Date, "2026-01-15", true),
            (Precision::Date, "2026-02-30", false),
            (Precision::Date, "2026-01-15 10:00:00", false),
            (Precision::Hour, "2026-01-15 10:00:00", true),
            (Precision::Hour, "2026-01-15 10:30:00", false),
            (Precision::Hour, "2026-01-15 24:00:00", false),
            (Precision::Hour, "2026-01-15 10", false),
            (Precision::Hour, "2026-02-30 10:00:00", false),
            (Precision::Minute, "2026-01-15 10:30:00", true),
            (Precision::Minute, "2026-01-15 10:30:15", false),
            (Precision::Second, "2026-01-15 10:30:15", true),
            (Precision::Second, "2026-01-15 10:30:15.000", false),
            (Precision::Second, "2026-01-15T10:30:15", false),
            (Precision::Millisecond, "2026-01-15 10:30:15.123", true),
            (Precision::Millisecond, "2026-01-15 10:30:15.12", false),
            (Precision::Millisecond, "2026-01-15 10:30:15.1234", false),
            (Precision::Millisecond, "2026-01-15 10:30:15", false),
            (Precision::Millisecond, "2026-01-15 10:30:15.1.3", false),
            (Precision::Microsecond, "2026-01-15 10:30:15.123456", true),
            (Precision::Microsecond, "2026-01-15 10:30:15.123", false),
            (Precision::Microsecond, "2026-01-15 10:30:15.12345x", false),
        ] {
            assert_eq!(
                is_label(precision, text),
                expected,
                "{precision:?} «{text}»"
            );
        }
    }

    #[test]
    fn a_column_of_dates_of_the_calendar_has_no_cell_outside() {
        let cells = [Some("2026-01-15"), None];
        assert_eq!(level(&cells, Precision::Month).first_outside(&[0, 1]), None);
    }
}
