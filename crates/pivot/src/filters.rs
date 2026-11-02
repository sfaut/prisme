//! Filters of a table (F-19, F-21, F-44), in the order of the computation: the
//! filter of the grid has chosen the rows (`Engine::set_kept`), the filters on
//! values keep some of them, then the filters on measures keep the rows of the
//! groups they accept. Several filters combine by `AND`: the filters on measures
//! all read the rows the filters on values leave, whatever their order.
//!
//! A filter on a measure keeps or drops rows, and the table is computed from the
//! rows that remain: its totals count the groups that remain and add up, unlike
//! `GROUP BY ROLLUP … HAVING`, which computes them first
//! (`docs/SQL-COMPARISON.md`, section 3.6).
//!
//! A value compares as a condition of SQL does: NULL, like an empty group, gives
//! neither true nor false, and its group leaves. `IN` also reads NULL here: the
//! list of values writes it `col IS NULL`. It tells values apart as the levels
//! group them, by their text: a date outside of the calendar is a value like any
//! other, which a list can keep or leave out.

use std::borrow::Cow;
use std::cmp::Ordering;

use prisme_store::{FxMap, compare_exact, parse_exact};

use crate::measures::{self, Value};
use crate::{Engine, Error, Family, Level, Measure, groups};

/// Filter of a table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Filter {
    /// Keeps the rows whose cell is one of some values (`col IN (…)`).
    Values {
        /// Column of the cells.
        column: usize,
        /// Values kept, `None` for NULL.
        values: Vec<Option<String>>,
    },
    /// Keeps the rows of the groups whose measure passes a comparison.
    Measure {
        /// Measure compared, which the table need not show.
        measure: Measure,
        /// Levels of its groups, the outermost first: the rows that share their
        /// values make a group. Without a level, all the rows make one.
        levels: Vec<Level>,
        /// Comparison of the value of each group to the operand.
        comparison: Comparison,
        /// Value compared to, read by the kind of the values of the measure: a
        /// number, or a text compared by code points (D7).
        operand: String,
    },
}

/// Comparison of a filter on a measure.
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

/// Operand of a filter, read by the kind of the values of its measure.
#[derive(Debug, Clone, Copy)]
enum Operand<'a> {
    /// Exact number, with its scale.
    Exact((i128, u32)),
    /// Float.
    Float(f64),
    /// Text.
    Text(&'a str),
}

impl Engine {
    /// Keeps the rows that the filters on values keep.
    pub(crate) fn keep_by_values<'a>(
        &self,
        filters: &[Filter],
        rows: &'a [u32],
    ) -> Result<Cow<'a, [u32]>, Error> {
        let mut kept = Cow::Borrowed(rows);
        for filter in filters {
            if let Filter::Values { column, values } = filter {
                let allowed = self.allowed_codes(*column, values)?;
                let codes = &self.dictionary(*column)?.codes;
                kept = kept
                    .iter()
                    .copied()
                    .filter(|&row| allowed[codes[row as usize] as usize])
                    .collect();
            }
        }
        Ok(kept)
    }

    /// Is each code of a column one of some values?
    fn allowed_codes(&self, column: usize, values: &[Option<String>]) -> Result<Vec<bool>, Error> {
        let dictionary = self.dictionary(column)?;
        let cells = self.store.column(column).expect("column of the dictionary");
        let wanted: FxMap<Option<&str>, ()> =
            values.iter().map(|value| (value.as_deref(), ())).collect();
        Ok((0..dictionary.len() as u32)
            .map(|code| wanted.contains_key(&dictionary.value(cells, code)))
            .collect())
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
                comparison,
                operand,
            } = filter
            {
                let (group_of_row, accepted) =
                    self.accepted_groups(index, *measure, levels, *comparison, operand, &rows)?;
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
    /// The group of each row given, and whether the comparison accepts each
    /// group.
    fn accepted_groups(
        &self,
        filter: usize,
        measure: Measure,
        levels: &[Level],
        comparison: Comparison,
        operand: &str,
        rows: &[u32],
    ) -> Result<(Vec<u32>, Vec<bool>), Error> {
        let input = self.input(0, measure).map_err(|error| match error {
            Error::NotNumeric { .. } => Error::FilterNotNumeric { filter },
            other => other,
        })?;
        let operand = self
            .operand(measure, operand)
            .ok_or(Error::FilterOperand { filter })?;
        self.check_dates(levels, rows)?;
        let (dictionaries, _) = self.levels(levels)?;
        let (group_of_row, first_rows) = groups::finest(&dictionaries, rows);
        let state = measures::accumulate(&input, rows, &group_of_row, first_rows.len(), &[])
            .map_err(|_| Error::FilterOverflow { filter })?;
        let accepted = (0..first_rows.len() as u32)
            .map(|group| {
                self.order_of(state.value(group), operand)
                    .is_some_and(|order| comparison.accepts(order))
            })
            .collect();
        Ok((group_of_row, accepted))
    }

    /// Reads an operand by the kind of the values of a measure, whose column
    /// exists. `None` if it cannot compare with them.
    fn operand<'a>(&self, measure: Measure, text: &'a str) -> Option<Operand<'a>> {
        let family = match measure {
            Measure::CountStar | Measure::Count(_) | Measure::CountDistinct(_) => Family::Exact,
            Measure::Sum(column)
            | Measure::SumDistinct(column)
            | Measure::Avg(column)
            | Measure::AvgDistinct(column)
            | Measure::Min(column)
            | Measure::Max(column) => self.families[column],
        };
        match family {
            Family::Exact => parse_exact(text).map(Operand::Exact),
            Family::Float => text.parse().ok().map(Operand::Float),
            Family::Other => Some(Operand::Text(text)),
        }
    }

    /// Order of a value of a measure and of an operand, `None` for NULL or an
    /// empty group, or a `NaN`.
    fn order_of(&self, value: Value, operand: Operand) -> Option<Ordering> {
        match (value, operand) {
            (Value::Count(count), Operand::Exact(number)) => {
                Some(compare_exact((i128::from(count), 0), number))
            }
            (Value::Exact(value, scale), Operand::Exact(number)) => {
                Some(compare_exact((value, scale), number))
            }
            (Value::Float(value), Operand::Float(number)) => value.partial_cmp(&number),
            (Value::Cell(column, row), operand) => {
                let cell = self.store.column(column)?.get(row as usize)?;
                match operand {
                    Operand::Exact(number) => Some(compare_exact(parse_exact(cell)?, number)),
                    Operand::Float(number) => cell.parse::<f64>().ok()?.partial_cmp(&number),
                    Operand::Text(text) => Some(cell.cmp(text)),
                }
            }
            _ => None,
        }
    }
}
