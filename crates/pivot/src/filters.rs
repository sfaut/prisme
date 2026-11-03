//! Filters of a table (F-19, F-21, F-44), in the order of the computation: the
//! filter of the grid has chosen the rows (`Engine::set_kept`), the filters on
//! cells keep some of them, then the filters on measures keep the rows of the
//! groups they accept. Several filters combine by `AND`: the filters on measures
//! all read the rows the filters on cells leave, whatever their order.
//!
//! Among the filters on cells, those that read a cell as it is come first: a
//! list of values, `IS NULL`. They stop nothing, and can leave out a cell that the
//! others cannot read: a comparison or a precision stops on a date outside of the
//! calendar, an hour outside of the day or a cell that is no number, when a row
//! still there holds it (A7). A date, an instant, an hour or a period compares
//! as its text once its form is checked, which is the order of time; any other
//! text by code points (D7), and numbers by value.
//!
//! A filter on a measure keeps or drops rows, and the table is computed from the
//! rows that remain: its totals count the groups that remain and add up, unlike
//! `GROUP BY ROLLUP … HAVING`, which computes them first
//! (`docs/SQL-COMPARISON.md`, section 3.6).
//!
//! A value compares as a condition of SQL does: NULL, like an empty group, gives
//! neither true nor false, and its row or its group leaves
//! (`docs/SQL-COMPARISON.md`, section 3.5). `IN` also reads NULL here: the list of
//! values writes it `col IS NULL`. It tells values apart as the levels group them,
//! by their text: a date outside of the calendar is a value like any other, which
//! a list can keep or leave out.

use std::borrow::Cow;
use std::cmp::Ordering;

use prisme_store::{FxMap, NumberError, compare_exact, is_exact, parse_exact};

use crate::dates::{comparable, is_label};
use crate::measures::{self, Value};
use crate::{Engine, Error, Family, Level, Measure, groups};

/// Filter of a table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Filter {
    /// Keeps the rows whose value is one of some values (`col IN (…)`), or none
    /// of them (`col NOT IN (…)`).
    Values {
        /// Column of the cells, and what its dates keep.
        level: Level,
        /// Values listed, `None` for NULL.
        values: Vec<Option<String>>,
        /// Does the filter keep the values left out of the list? As in SQL, a row
        /// whose value is NULL then leaves, and a NULL in the list leaves them all.
        excluded: bool,
    },
    /// Keeps the rows whose value passes a test.
    Column {
        /// Column of the cells, and what its dates keep.
        level: Level,
        /// Test of the value of each row.
        test: Test,
    },
    /// Keeps the rows of the groups whose measure passes a test.
    Measure {
        /// Measure compared, which the table need not show.
        measure: Measure,
        /// Levels of its groups, the outermost first: the rows that share their
        /// values make a group. Without a level, all the rows make one.
        levels: Vec<Level>,
        /// Test of the value of each group.
        test: Test,
    },
}

impl Filter {
    /// Can this filter on cells stop on a cell it cannot read?
    fn can_stop(&self) -> bool {
        match self {
            Filter::Values { level, .. } => level.precision.is_some(),
            Filter::Column { level, test } => level.precision.is_some() || test.compares(),
            Filter::Measure { .. } => false,
        }
    }
}

/// Test of a value. Its operands are texts in a filter, read by the kind of the
/// values once it applies: a number, or a text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Test<T = String> {
    /// `x = a`, `x < a`...
    Compare(Comparison, T),
    /// `x BETWEEN a AND b`: `x >= a AND x <= b`, its bounds included.
    Between(T, T),
    /// `x NOT BETWEEN a AND b`: `x < a OR x > b`.
    NotBetween(T, T),
    /// `x IS NULL`.
    IsNull,
    /// `x IS NOT NULL`.
    IsNotNull,
}

impl<T> Test<T> {
    /// Does the test compare its value to an operand?
    fn compares(&self) -> bool {
        !matches!(self, Test::IsNull | Test::IsNotNull)
    }

    /// Test with its operands read, `None` if one cannot be.
    fn read<'a, U>(&'a self, read: impl Fn(&'a T) -> Option<U>) -> Option<Test<U>> {
        Some(match self {
            Test::Compare(comparison, operand) => Test::Compare(*comparison, read(operand)?),
            Test::Between(lower, upper) => Test::Between(read(lower)?, read(upper)?),
            Test::NotBetween(lower, upper) => Test::NotBetween(read(lower)?, read(upper)?),
            Test::IsNull => Test::IsNull,
            Test::IsNotNull => Test::IsNotNull,
        })
    }

    /// Does a value pass the test?
    ///
    /// * `null`: is the value NULL?
    /// * `order`: order of the value and of an operand, `None` for a `NaN`.
    fn passes(&self, null: bool, order: impl Fn(&T) -> Option<Ordering>) -> bool {
        match self {
            Test::IsNull => null,
            Test::IsNotNull => !null,
            _ if null => false,
            Test::Compare(comparison, operand) => {
                order(operand).is_some_and(|found| comparison.accepts(found))
            }
            Test::Between(lower, upper) => {
                order(lower).is_some_and(Ordering::is_ge)
                    && order(upper).is_some_and(Ordering::is_le)
            }
            Test::NotBetween(lower, upper) => match (order(lower), order(upper)) {
                (Some(lower), Some(upper)) => lower.is_lt() || upper.is_gt(),
                _ => false,
            },
        }
    }
}

/// Comparison of a test.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Comparison {
    /// `=`.
    Equal,
    /// `<>`.
    NotEqual,
    /// `<`.
    Less,
    /// `<=`.
    LessOrEqual,
    /// `>`.
    Greater,
    /// `>=`.
    GreaterOrEqual,
}

impl Comparison {
    /// Does this order of a value and of the operand pass the comparison?
    fn accepts(self, order: Ordering) -> bool {
        match self {
            Comparison::Equal => order.is_eq(),
            Comparison::NotEqual => order.is_ne(),
            Comparison::Less => order.is_lt(),
            Comparison::LessOrEqual => order.is_le(),
            Comparison::Greater => order.is_gt(),
            Comparison::GreaterOrEqual => order.is_ge(),
        }
    }
}

/// Value of a test, or one of its operands, read by the kind of the values.
#[derive(Debug, Clone, Copy)]
enum Read<'a> {
    /// Exact number, with its scale.
    Exact((i128, u32)),
    /// Float.
    Float(f64),
    /// Text.
    Text(&'a str),
}

impl Read<'_> {
    /// Order of two values of one kind, `None` for a `NaN`, or for two kinds.
    fn order(self, other: Self) -> Option<Ordering> {
        match (self, other) {
            (Read::Exact(a), Read::Exact(b)) => Some(compare_exact(a, b)),
            (Read::Float(a), Read::Float(b)) => a.partial_cmp(&b),
            (Read::Text(a), Read::Text(b)) => Some(a.cmp(b)),
            _ => None,
        }
    }
}

/// Why a cell cannot be compared: it stops the computation.
#[derive(Debug, Clone, Copy)]
enum Unreadable {
    /// It is outside of the calendar, or of the day.
    Time,
    /// It is not a number.
    Number,
    /// It has more than 38 digits.
    Large,
}

impl Unreadable {
    /// Error of a row whose cell cannot be compared.
    fn error(self, column: usize, row: u32) -> Error {
        let row_index = row as usize;
        match self {
            Unreadable::Time => Error::InvalidDate { column, row },
            Unreadable::Number => Error::Number {
                column,
                error: NumberError::Invalid { row: row_index },
            },
            Unreadable::Large => Error::Number {
                column,
                error: NumberError::TooLarge { row: row_index },
            },
        }
    }
}

/// Reads a text by the family of its column.
fn read(family: Family, text: &str) -> Result<Read<'_>, Unreadable> {
    match family {
        Family::Exact => match parse_exact(text) {
            Some(number) => Ok(Read::Exact(number)),
            None if is_exact(text) => Err(Unreadable::Large),
            None => Err(Unreadable::Number),
        },
        Family::Float => text
            .parse()
            .map(Read::Float)
            .map_err(|_| Unreadable::Number),
        Family::Date | Family::Datetime | Family::Time => comparable(family, text)
            .map(Read::Text)
            .ok_or(Unreadable::Time),
        Family::Other => Ok(Read::Text(text)),
    }
}

impl Engine {
    /// Keeps the rows that the filters on cells keep, those that stop nothing
    /// first.
    pub(crate) fn keep_by_cells<'a>(
        &self,
        filters: &[Filter],
        rows: &'a [u32],
    ) -> Result<Cow<'a, [u32]>, Error> {
        let mut kept = Cow::Borrowed(rows);
        for stopping in [false, true] {
            for (index, filter) in filters.iter().enumerate() {
                if filter.can_stop() == stopping {
                    match filter {
                        Filter::Values {
                            level,
                            values,
                            excluded,
                        } => {
                            kept = self.keep_listed(*level, values, *excluded, &kept)?.into();
                        }
                        Filter::Column { level, test } => {
                            kept = self.keep_tested(index, *level, test, &kept)?.into();
                        }
                        Filter::Measure { .. } => {}
                    }
                }
            }
        }
        Ok(kept)
    }

    /// Keeps the rows whose value is, or is not, one of some values.
    fn keep_listed(
        &self,
        level: Level,
        values: &[Option<String>],
        excluded: bool,
        rows: &[u32],
    ) -> Result<Vec<u32>, Error> {
        let wanted: FxMap<Option<&str>, ()> =
            values.iter().map(|value| (value.as_deref(), ())).collect();
        let listed_null = wanted.contains_key(&None);
        self.keep_by_value(level, rows, |value| {
            Ok(if excluded {
                value.is_some() && !listed_null && !wanted.contains_key(&value)
            } else {
                wanted.contains_key(&value)
            })
        })
    }

    /// Keeps the rows whose value passes a test.
    fn keep_tested(
        &self,
        filter: usize,
        level: Level,
        test: &Test,
        rows: &[u32],
    ) -> Result<Vec<u32>, Error> {
        self.dictionary(level.column)?;
        let family = match level.precision {
            Some(_) => Family::Other,
            None => self.families[level.column],
        };
        let test = test
            .read(|text| match level.precision {
                Some(precision) => is_label(precision, text).then_some(Read::Text(text)),
                None => read(family, text).ok(),
            })
            .ok_or(Error::FilterOperand { filter })?;
        self.keep_by_value(level, rows, |value| match value {
            Some(cell) if test.compares() => {
                let cell = read(family, cell)?;
                Ok(test.passes(false, |&operand| cell.order(operand)))
            }
            _ => Ok(test.passes(value.is_none(), |_| None)),
        })
    }

    /// Keeps the rows whose value of a level a decision accepts, once per
    /// distinct value. Stops on the first row left whose cell cannot be read,
    /// or, at a precision, is outside of the calendar.
    ///
    /// * `decide`: decision on a value, `None` for NULL.
    fn keep_by_value(
        &self,
        level: Level,
        rows: &[u32],
        decide: impl Fn(Option<&str>) -> Result<bool, Unreadable>,
    ) -> Result<Vec<u32>, Error> {
        let Level { column, precision } = level;
        let dictionary = self.dictionary(column)?;
        let cells = self.store.column(column).expect("column of the dictionary");
        let (codes, decisions) = match precision {
            Some(precision) => {
                let derived = self.derived(column, precision)?;
                let decisions: Vec<_> = (0..derived.dictionary.len() as u32)
                    .map(|code| {
                        if derived.is_outside(code) {
                            Err(Unreadable::Time)
                        } else {
                            decide(derived.code_label(code))
                        }
                    })
                    .collect();
                (&derived.dictionary.codes, decisions)
            }
            None => {
                let decisions: Vec<_> = (0..dictionary.len() as u32)
                    .map(|code| decide(dictionary.value(cells, code)))
                    .collect();
                (&dictionary.codes, decisions)
            }
        };
        let mut kept = Vec::new();
        for &row in rows {
            match decisions[codes[row as usize] as usize] {
                Ok(true) => kept.push(row),
                Ok(false) => {}
                Err(unreadable) => return Err(unreadable.error(column, row)),
            }
        }
        Ok(kept)
    }

    /// Keeps the rows of the groups that the filters on measures accept.
    pub(crate) fn keep_by_measures<'a>(
        &self,
        filters: &[Filter],
        rows: Cow<'a, [u32]>,
    ) -> Result<Cow<'a, [u32]>, Error> {
        let mut keep: Option<Vec<bool>> = None;
        for (index, filter) in filters.iter().enumerate() {
            if let Filter::Measure {
                measure,
                levels,
                test,
            } = filter
            {
                let (group_of_row, accepted) =
                    self.accepted_groups(index, *measure, levels, test, &rows)?;
                let keep = keep.get_or_insert_with(|| vec![true; rows.len()]);
                for (slot, &group) in keep.iter_mut().zip(&group_of_row) {
                    *slot &= accepted[group as usize];
                }
            }
        }
        Ok(match keep {
            Some(keep) => rows
                .iter()
                .zip(&keep)
                .filter_map(|(&row, &kept)| kept.then_some(row))
                .collect(),
            None => rows,
        })
    }

    /// Groups of a filter on a measure.
    ///
    /// # Returns
    ///
    /// The group of each row given, and whether the test accepts each group.
    fn accepted_groups(
        &self,
        filter: usize,
        measure: Measure,
        levels: &[Level],
        test: &Test,
        rows: &[u32],
    ) -> Result<(Vec<u32>, Vec<bool>), Error> {
        let input = self.input(0, measure).map_err(|error| match error {
            Error::NotNumeric { .. } => Error::FilterNotNumeric { filter },
            other => other,
        })?;
        let test = test
            .read(|text| self.operand(measure, text))
            .ok_or(Error::FilterOperand { filter })?;
        self.check_dates(levels, rows)?;
        let (dictionaries, _) = self.levels(levels)?;
        let (group_of_row, first_rows) = groups::finest(&dictionaries, rows);
        let state = measures::accumulate(&input, rows, &group_of_row, first_rows.len(), &[])
            .map_err(|_| Error::FilterOverflow { filter })?;
        let accepted = (0..first_rows.len() as u32)
            .map(|group| {
                let value = state.value(group);
                let null = matches!(value, Value::Empty | Value::Null);
                if null || !test.compares() {
                    return Ok(test.passes(null, |_| None));
                }
                let value = self.read_value(value)?;
                Ok(test.passes(false, |&operand| value?.order(operand)))
            })
            .collect::<Result<_, Error>>()?;
        Ok((group_of_row, accepted))
    }

    /// Reads an operand by the kind of the values of a measure, whose column
    /// exists. `None` if it cannot compare with them.
    fn operand<'a>(&self, measure: Measure, text: &'a str) -> Option<Read<'a>> {
        let family = match measure {
            Measure::CountStar | Measure::Count(_) | Measure::CountDistinct(_) => Family::Exact,
            Measure::Sum(column)
            | Measure::SumDistinct(column)
            | Measure::Avg(column)
            | Measure::AvgDistinct(column)
            | Measure::Min(column)
            | Measure::Max(column) => self.families[column],
        };
        read(family, text).ok()
    }

    /// Reads a value of a measure that is not NULL, `None` for a cell gone. A
    /// date `MIN` or `MAX` chose outside of the calendar stops the computation.
    fn read_value(&self, value: Value) -> Result<Option<Read<'_>>, Error> {
        Ok(match value {
            Value::Count(count) => Some(Read::Exact((i128::from(count), 0))),
            Value::Exact(number, scale) => Some(Read::Exact((number, scale))),
            Value::Float(number) => Some(Read::Float(number)),
            Value::Cell(column, row) => {
                match self
                    .store
                    .column(column)
                    .and_then(|cells| cells.get(row as usize))
                {
                    Some(cell) => Some(
                        read(self.families[column], cell)
                            .map_err(|unreadable| unreadable.error(column, row))?,
                    ),
                    None => None,
                }
            }
            Value::Empty | Value::Null => None,
        })
    }
}
