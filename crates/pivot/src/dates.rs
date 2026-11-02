//! Dates of a level of the Pivoteur: what a level keeps of the dates of its
//! column, its precision (year, quarter, month, week, day), one group per
//! period, in the order of time.
//!
//! A date is read in the text of its cell, by the civil calendar: the time and
//! the time zone that may follow it are left out, as they are written.
//!
//! The calendar holds the years 0001 to 9999, as in ANSI SQL. A cell outside of
//! it has no period, and stops the computation when a row taken into account
//! holds it: a date without a day in the calendar, which MySQL and MariaDB can
//! hold (`0000-00-00`, `2026-03-00`, `2026-02-30`) and which the servers read
//! each in their own way (`docs/SQL-COMPARISON.md`, section 3.12), a date before
//! the Christian era (PostgreSQL, DuckDB), `infinity`, any other text.

use prisme_store::{Dictionary, FxMap, TextColumn};

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
}

impl Precision {
    /// Number of precisions.
    pub const COUNT: usize = 5;
}

/// Period of a date: its year, then its rank in the year. Periods sort in the
/// order of time.
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

/// Date of the calendar written in a cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CellDate {
    /// Year.
    year: i64,
    /// Month.
    month: i64,
    /// Day.
    day: i64,
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
        in_calendar.then_some(CellDate { year, month, day })
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

    /// Period of the date at a precision.
    fn period(self, precision: Precision) -> Period {
        match precision {
            Precision::Year => (self.iso_week().0, 0),
            Precision::Quarter => (self.year, (self.month + 2) / 3),
            Precision::Month => (self.year, self.month),
            Precision::Week => self.iso_week(),
            Precision::Date => (self.year, self.month * 100 + self.day),
        }
    }
}

/// Label of a period.
fn label(precision: Precision, (year, rank): Period) -> String {
    match precision {
        Precision::Year => format!("{year:04}"),
        Precision::Quarter => format!("{year:04}-T{rank}"),
        Precision::Month => format!("{year:04}-{rank:02}"),
        Precision::Week => format!("{year:04}-S{rank:02}"),
        Precision::Date => format!("{year:04}-{:02}-{:02}", rank / 100, rank % 100),
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
                    .map_or(Slot::Outside, |date| Slot::Period(date.period(precision))),
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
        self.labels[self.dictionary.codes[row as usize] as usize].as_deref()
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
                    let date = CellDate { year, month, day };
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
            ] {
                let cells = [Some("2026-01-15"), None, Some(cell)];
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
    fn a_column_of_dates_of_the_calendar_has_no_cell_outside() {
        let cells = [Some("2026-01-15"), None];
        assert_eq!(level(&cells, Precision::Month).first_outside(&[0, 1]), None);
    }
}
