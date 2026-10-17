//! Strings of dates and times as each dialect reads them in a typed literal
//! (`DATE '2024-05-06'`, `TIME '07:08:09.5'`, `TIMESTAMP '...'`, `{d '...'}`),
//! measured on MySQL 8.0.11 and 8.4.11, MariaDB 10.6.28 and 11.8.9
//! (`crates/probe/cases/typed-literals*.txt`).
//!
//! Both dialects read the same forms: the parts of a date separated by one
//! punctuation character (`2024-05-06`, `2024/5/6`, `24.05.06`), a year of
//! two digits in 1970 to 2069, the compact forms (`20240506`, `240506`,
//! `20240506070809`), a time of one to three parts (`7:8`, `1 07:08:09` for
//! a day and 7 hours, `070809`), a fraction of any number of digits. They
//! differ by a fraction beyond 6 digits, rounded by MySQL with its carry,
//! truncated by MariaDB; by the range of a `TIME`, up to 838:59:59 in MySQL,
//! 838:59:59.999999 in MariaDB; and by a time zone offset (`+02:00`), read
//! by MySQL from 8.0.19 only. A form not recognised here, or read
//! differently by the dialects (`2024--05-06`, `TIME ':08:09'`), is left to
//! the server.

use std::fmt::Write as _;

use crate::Dialect;
use crate::sql::ast::TemporalKind;

/// Largest number of fractional digits kept by the servers.
const MAX_FRACTION: usize = 6;

/// Largest number of fractional digits of a `TIME` MySQL rounds: beyond, it
/// truncates some values and rounds others (`07:08:09.1234567890` gives
/// .123456, `07:08:09.9999999999` gives 07:08:10).
const MYSQL_TIME_FRACTION: usize = 9;

/// Largest number of hours of a `TIME`.
const MAX_HOURS: u32 = 838;

/// Largest year of a date.
const MAX_YEAR: u32 = 9999;

/// Largest time zone offset, in minutes, either way.
const MAX_OFFSET: u32 = 14 * 60;

/// What a dialect reads in a temporal string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Reading {
    /// A date or a time.
    Valid(Temporal),
    /// A string the dialect refuses (`DATE '2024-02-30'`, `TIME '10:60:00'`).
    Invalid,
    /// A form Prisme does not read, or that the dialects read differently.
    Unknown,
}

/// Date, time, or date and time read in a string, its fraction brought to 6
/// digits at most by the dialect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Temporal {
    /// Date; absent for a `TIME`.
    pub(crate) date: Option<Date>,
    /// Time; absent for a `DATE`.
    pub(crate) clock: Option<Clock>,
    /// Fractional digits of the seconds, 6 at most.
    pub(crate) fraction: String,
    /// Time zone offset after the time, as written (`+02:00`).
    pub(crate) offset: Option<String>,
}

/// Date of the calendar, a zero month or day included.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Date {
    /// Year, 0 to 9999.
    pub(crate) year: u32,
    /// Month, 0 to 12.
    pub(crate) month: u32,
    /// Day, 0 to 31.
    pub(crate) day: u32,
}

/// Time of a day, or duration of a `TIME`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Clock {
    /// Duration before the zero (`TIME '-01:00:00'`).
    pub(crate) negative: bool,
    /// Hours, up to 838 for a `TIME`, 23 in a date and time.
    pub(crate) hours: u32,
    /// Minutes.
    pub(crate) minutes: u32,
    /// Seconds.
    pub(crate) seconds: u32,
}

/// Reads `text`, the string of a literal of `kind`, as `dialect` does.
pub(crate) fn read(kind: TemporalKind, text: &str, dialect: Dialect) -> Reading {
    let text = text.trim_matches(|character: char| character.is_ascii_whitespace());
    let foreign = |character: char| {
        !(character.is_ascii_digit()
            || character.is_ascii_punctuation()
            || character.is_ascii_whitespace()
            || character == 'T')
    };
    if text.is_empty() || text.contains(foreign) {
        return Reading::Invalid;
    }
    let parsed = match kind {
        TemporalKind::Date => read_date(text),
        TemporalKind::Time => read_time(text),
        TemporalKind::Timestamp => read_date_time(text),
    };
    match parsed {
        Parsed::Value(raw) => raw.settle(kind, dialect),
        Parsed::Invalid => Reading::Invalid,
        Parsed::Unknown => Reading::Unknown,
    }
}

impl Temporal {
    /// Tells whether the date has a zero month or day (`0000-00-00`,
    /// `2024-05-00`), which MariaDB types nullable.
    pub(crate) fn zero(&self) -> bool {
        self.date
            .is_some_and(|date| date.month == 0 || date.day == 0)
    }

    /// Tells whether the date is `0000-00-00`, refused under the SQL mode
    /// `NO_ZERO_DATE`.
    pub(crate) fn zero_date(&self) -> bool {
        self.date
            .is_some_and(|date| date.year == 0 && date.month == 0 && date.day == 0)
    }

    /// Tells whether the date has a zero month or day without being
    /// `0000-00-00`, refused under the SQL mode `NO_ZERO_IN_DATE`.
    pub(crate) fn zero_in_date(&self) -> bool {
        self.zero() && !self.zero_date()
    }

    /// Fractional digits of the type of the literal, 6 at most.
    pub(crate) fn digits(&self) -> u8 {
        u8::try_from(self.fraction.len()).expect("6 digits at most")
    }

    /// Canonical text of the value, which both dialects read as this value
    /// when it is in their range: `2024-05-06`, `07:08:09.5`, `-26:03:04`,
    /// `2024-05-06 07:08:09.123457+02:00`.
    pub(crate) fn text(&self) -> String {
        let mut text = String::new();
        if let Some(date) = self.date {
            let _ = write!(text, "{:04}-{:02}-{:02}", date.year, date.month, date.day);
        }
        if let Some(clock) = self.clock {
            if !text.is_empty() {
                text.push(' ');
            }
            let sign = if clock.negative { "-" } else { "" };
            let _ = write!(
                text,
                "{sign}{:02}:{:02}:{:02}",
                clock.hours, clock.minutes, clock.seconds
            );
            if !self.fraction.is_empty() {
                text.push('.');
                text.push_str(&self.fraction);
            }
        }
        if let Some(offset) = &self.offset {
            text.push_str(offset);
        }
        text
    }
}

/// Outcome of the reading of a form, before the fraction and the range of
/// the dialect.
enum Parsed {
    /// A value of a form both dialects read.
    Value(Raw),
    /// A string both dialects refuse.
    Invalid,
    /// A form not recognised.
    Unknown,
}

/// Value read in a string, its fraction as written.
struct Raw {
    /// Date; absent for a `TIME`.
    date: Option<Date>,
    /// Time; absent for a `DATE`.
    clock: Option<Clock>,
    /// Fractional digits, as written.
    fraction: String,
    /// Time zone offset, as written.
    offset: Option<String>,
}

impl Raw {
    /// Value of a date alone.
    const fn date(date: Date) -> Self {
        Self {
            date: Some(date),
            clock: None,
            fraction: String::new(),
            offset: None,
        }
    }

    /// The value as `dialect` reads it: the fraction rounded by MySQL,
    /// truncated by MariaDB, the range and the offset of the dialect.
    fn settle(self, kind: TemporalKind, dialect: Dialect) -> Reading {
        let zero = self
            .date
            .is_some_and(|date| date.month == 0 || date.day == 0);
        if self.offset.is_some() && (dialect == Dialect::MariaDb) {
            return Reading::Invalid;
        }
        if self.offset.is_some() && zero {
            return Reading::Unknown;
        }
        let mut value = Temporal {
            date: self.date,
            clock: self.clock,
            fraction: self.fraction.chars().take(MAX_FRACTION).collect(),
            offset: self.offset,
        };
        if dialect == Dialect::MySql {
            if kind == TemporalKind::Time && self.fraction.len() > MYSQL_TIME_FRACTION {
                return Reading::Unknown;
            }
            let carry = self
                .fraction
                .as_bytes()
                .get(MAX_FRACTION)
                .is_some_and(|digit| *digit >= b'5');
            if carry {
                if zero {
                    return Reading::Unknown;
                }
                if !round_up(&mut value) {
                    return Reading::Invalid;
                }
            }
            let fraction = value.fraction.bytes().any(|digit| digit != b'0');
            if let (None, Some(clock)) = (value.date, value.clock) {
                let beyond = (clock.hours, clock.minutes, clock.seconds) > (MAX_HOURS, 59, 59)
                    || ((clock.hours, clock.minutes, clock.seconds) == (MAX_HOURS, 59, 59)
                        && fraction);
                if beyond {
                    return Reading::Invalid;
                }
            }
        }
        Reading::Valid(value)
    }
}

/// Adds a unit of the last fractional digit, its carry included, away from
/// the zero for a negative duration; `false` out of the range of the type.
fn round_up(value: &mut Temporal) -> bool {
    let width = value.fraction.len();
    let next = value.fraction.parse::<u32>().unwrap_or(0) + 1;
    if width > 0 && next < 10_u32.pow(u32::try_from(width).expect("6 digits at most")) {
        value.fraction = format!("{next:0width$}");
        return true;
    }
    value.fraction = "0".repeat(width);
    let clock = value.clock.as_mut().expect("a time before a fraction");
    clock.seconds += 1;
    if clock.seconds == 60 {
        clock.seconds = 0;
        clock.minutes += 1;
    }
    if clock.minutes == 60 {
        clock.minutes = 0;
        clock.hours += 1;
    }
    match &mut value.date {
        None => clock.hours <= MAX_HOURS,
        Some(_) if clock.hours < 24 => true,
        Some(date) => {
            clock.hours = 0;
            next_day(date)
        }
    }
}

/// Moves `date` to the next day; `false` beyond 9999-12-31.
fn next_day(date: &mut Date) -> bool {
    date.day += 1;
    if date.day > days_in_month(date.year, date.month) {
        date.day = 1;
        date.month += 1;
    }
    if date.month > 12 {
        date.month = 1;
        date.year += 1;
    }
    date.year <= MAX_YEAR
}

/// Parts of a string of digits and their separators.
struct Parts<'a> {
    /// Runs of digits.
    numbers: Vec<&'a str>,
    /// What follows each run of digits, up to the next one; empty after the
    /// last when the text ends on digits.
    separators: Vec<&'a str>,
}

/// Splits `text`, which starts with a digit, into its runs of digits and
/// what separates them; `None` when it starts otherwise.
fn parts(text: &str) -> Option<Parts<'_>> {
    if !text.starts_with(|character: char| character.is_ascii_digit()) {
        return None;
    }
    let mut parts = Parts {
        numbers: Vec::new(),
        separators: Vec::new(),
    };
    let mut rest = text;
    while !rest.is_empty() {
        let digits = rest
            .find(|character: char| !character.is_ascii_digit())
            .unwrap_or(rest.len());
        parts.numbers.push(&rest[..digits]);
        rest = &rest[digits..];
        let separator = rest
            .find(|character: char| character.is_ascii_digit())
            .unwrap_or(rest.len());
        parts.separators.push(&rest[..separator]);
        rest = &rest[separator..];
    }
    Some(parts)
}

/// Tells whether `separator` is one punctuation character.
fn punctuation(separator: &str) -> bool {
    separator.len() == 1 && separator.bytes().all(|byte| byte.is_ascii_punctuation())
}

/// Value of a run of digits.
fn number(digits: &str) -> u32 {
    digits.bytes().fold(0_u32, |value, digit| {
        value
            .saturating_mul(10)
            .saturating_add(u32::from(digit - b'0'))
    })
}

/// `DATE`: three parts separated by one punctuation character each, one
/// more allowed at the end (`2024-05-06.`), or the compact `YYYYMMDD` and
/// `YYMMDD`.
fn read_date(text: &str) -> Parsed {
    if text.bytes().all(|byte| byte.is_ascii_digit()) {
        return match text.len() {
            8 => date(&text[..4], &text[4..6], &text[6..]),
            6 => date(&text[..2], &text[2..4], &text[4..]),
            4 | 7 | 9 | 14 => Parsed::Invalid,
            _ => Parsed::Unknown,
        };
    }
    let Some(parts) = parts(text) else {
        return Parsed::Unknown;
    };
    let count = parts.numbers.len();
    let inner = &parts.separators[..count - 1];
    let last = parts.separators[count - 1];
    if count == 3 && inner.iter().all(|separator| punctuation(separator)) {
        if last.is_empty() || punctuation(last) {
            return date(parts.numbers[0], parts.numbers[1], parts.numbers[2]);
        }
        return Parsed::Unknown;
    }
    let plain = |separator: &&str| {
        punctuation(separator) || separator.bytes().all(|byte| byte.is_ascii_whitespace())
    };
    if last.is_empty() && count != 3 && inner.iter().all(plain) {
        return Parsed::Invalid;
    }
    Parsed::Unknown
}

/// Date of its parts: a year of two digits in 1970 to 2069, the year 0 kept
/// for the zero date (`00-00-00`).
fn date(year: &str, month: &str, day: &str) -> Parsed {
    let (month, day) = (number(month), number(day));
    let mut year_value = number(year);
    if year.len() > 4 {
        return Parsed::Invalid;
    }
    if year.len() == 2 && (year_value, month, day) != (0, 0, 0) {
        year_value += if year_value < 70 { 2000 } else { 1900 };
    }
    if year_value == 0 && month == 2 && day == 29 {
        return Parsed::Unknown;
    }
    let valid = month <= 12
        && day <= 31
        && (month == 0 || day == 0 || day <= days_in_month(year_value, month));
    if !valid {
        return Parsed::Invalid;
    }
    Parsed::Value(Raw::date(Date {
        year: year_value,
        month,
        day,
    }))
}

/// `TIME`: `[-]D hh[:mm[:ss]][.f]` with a number of days, `[-]h:m[:s][.f]`,
/// or the compact `[-][[h]hmm]ss[.f]`.
fn read_time(text: &str) -> Parsed {
    let (negative, text) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text),
    };
    let time = |hours: u32, minutes: u32, seconds: u32, fraction: &str| {
        if minutes > 59 || seconds > 59 || hours > MAX_HOURS {
            return Parsed::Invalid;
        }
        Parsed::Value(Raw {
            date: None,
            clock: Some(Clock {
                negative,
                hours,
                minutes,
                seconds,
            }),
            fraction: fraction.to_string(),
            offset: None,
        })
    };
    let (body, fraction) = text.split_once('.').unwrap_or((text, ""));
    if !fraction.bytes().all(|byte| byte.is_ascii_digit()) {
        // `07:08:09.5.5` and `07:08:09.-5` are refused by both.
        return if fraction.contains('.') || fraction.starts_with('-') {
            Parsed::Invalid
        } else {
            Parsed::Unknown
        };
    }
    if body.bytes().all(|byte| byte.is_ascii_digit()) {
        if body.is_empty() && fraction.is_empty() {
            return Parsed::Unknown;
        }
        let (rest, seconds) = body.split_at(body.len().saturating_sub(2));
        let (hours, minutes) = rest.split_at(rest.len().saturating_sub(2));
        return time(number(hours), number(minutes), number(seconds), fraction);
    }
    if let Some((days, clock)) = body.split_once(|character: char| character.is_ascii_whitespace())
    {
        let clock = clock.trim_start_matches(|character: char| character.is_ascii_whitespace());
        if days.is_empty() || !days.bytes().all(|byte| byte.is_ascii_digit()) {
            return Parsed::Unknown;
        }
        let days = number(days);
        let fields: Vec<&str> = clock.split(':').collect();
        if fields
            .iter()
            .any(|field| field.is_empty() || !field.bytes().all(|byte| byte.is_ascii_digit()))
        {
            return Parsed::Unknown;
        }
        let hours = |field: &str| days.saturating_mul(24).saturating_add(number(field));
        return match fields.as_slice() {
            [field] if field.len() >= 2 => time(hours(field), 0, 0, fraction),
            [_] => Parsed::Invalid,
            [h, m] => time(hours(h), number(m), 0, fraction),
            [h, m, s] => time(hours(h), number(m), number(s), fraction),
            _ => Parsed::Unknown,
        };
    }
    let fields: Vec<&str> = body.split(':').collect();
    if fields
        .iter()
        .any(|field| field.is_empty() || !field.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return Parsed::Unknown;
    }
    match fields.as_slice() {
        [h, m] => time(number(h), number(m), 0, fraction),
        [h, m, s] => time(number(h), number(m), number(s), fraction),
        [_, _, _, _] => Parsed::Invalid,
        _ => Parsed::Unknown,
    }
}

/// `TIMESTAMP`: a date of three parts, a space, a `T` or a punctuation
/// character, then `h[:m[:s]]` separated by any punctuation, a fraction
/// after a `.` and an offset after the seconds; or the compact
/// `YYYYMMDDhhmmss[.f]` and `YYMMDDhhmmss[.f]`.
fn read_date_time(text: &str) -> Parsed {
    let (whole, fraction) = text.split_once('.').unwrap_or((text, ""));
    if whole.bytes().all(|byte| byte.is_ascii_digit())
        && fraction.bytes().all(|byte| byte.is_ascii_digit())
    {
        let split = match whole.len() {
            14 => 8,
            12 => 6,
            _ => return Parsed::Unknown,
        };
        let (day, clock) = whole.split_at(split);
        let raw = match read_date(day) {
            Parsed::Value(raw) => raw,
            other => return other,
        };
        let field = |range: std::ops::Range<usize>| number(&clock[range]);
        return date_time(raw, (field(0..2), field(2..4), field(4..6)), fraction, None);
    }
    let Some(parts) = parts(text) else {
        return Parsed::Unknown;
    };
    let (numbers, separators) = (&parts.numbers, &parts.separators);
    if numbers.len() < 3
        || !separators[..2]
            .iter()
            .all(|separator| punctuation(separator))
    {
        return Parsed::Unknown;
    }
    let day = match date(numbers[0], numbers[1], numbers[2]) {
        Parsed::Value(raw) => raw,
        other => return other,
    };
    if numbers.len() == 3 {
        return if separators[2].is_empty() {
            Parsed::Invalid
        } else {
            Parsed::Unknown
        };
    }
    let between = separators[2];
    let blank = between.bytes().all(|byte| byte.is_ascii_whitespace());
    if !(blank || between == "T" || punctuation(between)) {
        return Parsed::Unknown;
    }
    // Time: up to three parts separated by one punctuation character each,
    // then a fraction or an offset.
    let mut index = 3;
    let mut clock = Vec::new();
    loop {
        clock.push(number(numbers[index]));
        let separator = separators[index];
        index += 1;
        if clock.len() == 3 || separator.is_empty() || index == numbers.len() {
            break;
        }
        if !punctuation(separator) {
            return Parsed::Unknown;
        }
    }
    let (hours, minutes, seconds) = match clock.as_slice() {
        [h] => (*h, 0, 0),
        [h, m] => (*h, *m, 0),
        [h, m, s] => (*h, *m, *s),
        _ => return Parsed::Unknown,
    };
    let tail = &text[text.len() - rest_len(numbers, separators, index)..];
    if clock.len() < 3 {
        return if tail.is_empty() {
            date_time(day, (hours, minutes, seconds), "", None)
        } else {
            Parsed::Unknown
        };
    }
    let (fraction, tail) = match tail.strip_prefix('.') {
        Some(rest) => {
            let digits = rest
                .find(|character: char| !character.is_ascii_digit())
                .unwrap_or(rest.len());
            (&rest[..digits], &rest[digits..])
        }
        None => ("", tail),
    };
    if tail.is_empty() {
        return date_time(day, (hours, minutes, seconds), fraction, None);
    }
    match offset(tail) {
        Some(Some(offset)) => date_time(day, (hours, minutes, seconds), fraction, Some(offset)),
        Some(None) => Parsed::Invalid,
        None => Parsed::Unknown,
    }
}

/// Length of the text that follows the separator of the part at `index - 1`:
/// the separator itself, then the next parts.
fn rest_len(numbers: &[&str], separators: &[&str], index: usize) -> usize {
    separators[index - 1].len()
        + numbers[index..]
            .iter()
            .zip(&separators[index..])
            .map(|(number, separator)| number.len() + separator.len())
            .sum::<usize>()
}

/// Date and time of its parts, the hours below 24.
fn date_time(
    day: Raw,
    (hours, minutes, seconds): (u32, u32, u32),
    fraction: &str,
    offset: Option<&str>,
) -> Parsed {
    if hours > 23 || minutes > 59 || seconds > 59 {
        return Parsed::Invalid;
    }
    Parsed::Value(Raw {
        date: day.date,
        clock: Some(Clock {
            negative: false,
            hours,
            minutes,
            seconds,
        }),
        fraction: fraction.to_string(),
        offset: offset.map(str::to_string),
    })
}

/// Time zone offset of `text`, a sign, two digits, `:` and two digits:
/// `Some(None)` out of its range (`+13:60`, `+14:01`, `-00:00`), `None` for
/// another form.
fn offset(text: &str) -> Option<Option<&str>> {
    let bytes = text.as_bytes();
    let shape = bytes.len() == 6
        && matches!(bytes[0], b'+' | b'-')
        && bytes[1].is_ascii_digit()
        && bytes[2].is_ascii_digit()
        && bytes[3] == b':'
        && bytes[4].is_ascii_digit()
        && bytes[5].is_ascii_digit();
    if !shape {
        return None;
    }
    let (hours, minutes) = (number(&text[1..3]), number(&text[4..6]));
    let valid = minutes <= 59 && hours * 60 + minutes <= MAX_OFFSET && text != "-00:00";
    Some(valid.then_some(text))
}

/// Days of `month` in `year`, of the Gregorian calendar.
pub(crate) const fn days_in_month(year: u32, month: u32) -> u32 {
    match month {
        2 if year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400)) => {
            29
        }
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

#[cfg(test)]
mod tests;
