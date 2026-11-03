//! Measures of a table: each row feeds the state of its finest group, then
//! the states of each set of groups merge into those of the coarser set.
//! Subtotals and totals are therefore computed from the rows, never from the
//! cells of the table.
//!
//! - `COUNT(*)` counts the rows, `COUNT(col)` the rows where the column is
//!   not NULL.
//! - `COUNT(DISTINCT col)` counts the distinct values of the column that are
//!   not NULL, told apart as the levels group them (`Dictionary`). A count
//!   of distinct values does not add up: each set of groups counts the
//!   pairs of a group and a value, without duplicates, that it takes from
//!   the finer set it merges.
//! - `SUM(col)` adds the values that are not NULL; without any, it is NULL.
//!   Exact numbers add up on 128 bits, and going beyond is an error; floats
//!   add up as `DOUBLE`.
//! - `SUM(DISTINCT col)` adds each number once per group, from the same
//!   pairs. Numbers are told apart by value: `1.0` and `1.00` are one number.
//! - `AVG(col)` and `AVG(DISTINCT col)` divide these sums by their number of
//!   values, once merged. An average of exact numbers is exact, with four
//!   more digits after the point than its column, rounded away from zero; an
//!   average of floats is a `DOUBLE`.
//! - `MIN(col)` and `MAX(col)` keep the first or the last value of each
//!   group, in the order of the column, and give its cell as it is; at a
//!   precision, the first or the last period, in the order of time, and give
//!   its label.
//!
//! The behaviours of the engines these rules follow are measured in
//! `docs/SQL-COMPARISON.md`, section 3.6 (cases `agg.*`).

use std::cmp::Ordering;
use std::hash::Hash;

use prisme_store::{Approx, Dictionary, Exact, FxMap};

use crate::groups::Merge;
use crate::{Engine, Level};

/// Digits an average of exact numbers adds after the point, as MySQL and
/// MariaDB do.
const AVERAGE_DIGITS: u32 = 4;

/// Code or row of a group without a value (`MIN`, `MAX`). The rows of a
/// result stay below it.
const NO_VALUE: u32 = u32::MAX;

/// Values a measure reads.
pub enum Input<'a> {
    /// The rows themselves (`COUNT(*)`).
    Rows,
    /// Validity of the cells of a column (`COUNT(col)`).
    Cells(&'a [bool]),
    /// Codes of the values of a level, by row, and the validity of the cells
    /// of its column (`COUNT(DISTINCT col)`).
    Distinct(&'a [u32], &'a [bool]),
    /// Exact numbers of a column and their validity.
    Exact(&'a Exact, &'a [bool]),
    /// Floats of a column and their validity.
    Float(&'a Approx, &'a [bool]),
    /// Exact numbers of a column, their validity and the dictionary of the
    /// column, each number being added once per group.
    DistinctExact(&'a Exact, &'a [bool], &'a Dictionary),
    /// Floats of a column, their validity and the dictionary of the column,
    /// each number being added once per group.
    DistinctFloat(&'a Approx, &'a [bool], &'a Dictionary),
    /// Average of the numbers an input adds up.
    Average(Box<Input<'a>>),
    /// Values `MIN` or `MAX` compares.
    Extreme(Extreme<'a>),
}

/// Values `MIN` or `MAX` compares.
#[derive(Clone, Copy)]
pub struct Extreme<'a> {
    /// Level of the values: its column, at a precision of its dates or not.
    pub level: Level,
    /// Codes of the values of the level.
    pub dictionary: &'a Dictionary,
    /// Rank of each code, in the order of the comparison.
    pub ranks: &'a [u32],
    /// Validity of the cells of the column.
    pub valid: &'a [bool],
    /// Keeps the last value (`MAX`), or the first one (`MIN`)?
    pub largest: bool,
}

/// Exact number beyond 128 bits.
#[derive(Debug, PartialEq, Eq)]
pub struct Overflow;

/// States of a measure, one per group.
#[derive(Debug)]
pub enum State {
    /// Counts.
    Count(Vec<u64>),
    /// Sums or averages of exact numbers, scaled to `scale`, and number of
    /// values added.
    Exact {
        /// Sum or average of each group.
        sums: Vec<i128>,
        /// Number of values added in each group.
        counts: Vec<u64>,
        /// Number of digits after the point.
        scale: u32,
    },
    /// Sums or averages of floats, and number of values added.
    Float {
        /// Sum or average of each group.
        sums: Vec<f64>,
        /// Number of values added in each group.
        counts: Vec<u64>,
    },
    /// Values of a level (`MIN`, `MAX`).
    Cells {
        /// Level of the values.
        level: Level,
        /// Row of the cell of each group, `NO_VALUE` without one.
        rows: Vec<u32>,
    },
}

/// Value of a measure for a cell of the table.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Value {
    /// No row falls into the cell: it stays empty.
    Empty,
    /// The measure is NULL.
    Null,
    /// Count.
    Count(u64),
    /// Exact number, scaled to the scale given with it.
    Exact(i128, u32),
    /// Float.
    Float(f64),
    /// Value of a level at a row of the result: its cell, or the label of its
    /// period.
    Cell(Level, u32),
}

impl Value {
    /// Text of the value: `None` for an empty cell or NULL.
    pub fn text(self, engine: &Engine) -> Option<String> {
        match self {
            Value::Empty | Value::Null => None,
            Value::Count(count) => Some(count.to_string()),
            Value::Exact(value, scale) => Some(prisme_store::format_exact(value, scale)),
            Value::Float(value) => Some(prisme_store::format_float(value)),
            Value::Cell(level, row) => engine.level_value(level, row).map(str::to_owned),
        }
    }
}

/// What a sort compares of a value of a measure.
#[derive(Debug, Clone, Copy)]
pub enum SortKey {
    /// Empty cell or NULL, smaller than any value (D22).
    Missing,
    /// Count, exact number, or rank of a cell: the values of a measure share
    /// their scale.
    Exact(i128),
    /// Float, a `NaN` after the others.
    Float(f64),
}

impl SortKey {
    /// Compares two keys of a measure.
    pub fn compare(self, other: Self) -> Ordering {
        match (self, other) {
            (SortKey::Missing, SortKey::Missing) => Ordering::Equal,
            (SortKey::Missing, _) => Ordering::Less,
            (_, SortKey::Missing) => Ordering::Greater,
            (SortKey::Exact(a), SortKey::Exact(b)) => a.cmp(&b),
            (SortKey::Float(a), SortKey::Float(b)) => a.total_cmp(&b),
            // A measure gives a single kind of number: any order is total.
            (SortKey::Exact(_), SortKey::Float(_)) => Ordering::Less,
            (SortKey::Float(_), SortKey::Exact(_)) => Ordering::Greater,
        }
    }
}

/// Rows a measure reads, with the groups they feed.
#[derive(Clone, Copy)]
struct Feed<'a> {
    /// Rows taken into account.
    selection: &'a [u32],
    /// Finest group of each selected row.
    finest: &'a [u32],
    /// Number of groups, all sets included.
    groups: usize,
    /// Merges of the sets of groups, in their order.
    merges: &'a [Merge],
}

/// Adds the states of each set of groups into those of the coarser set.
fn merge_counts(counts: &mut [u64], merges: &[Merge]) {
    for merge in merges {
        for (offset, &parent) in merge.parent_of.iter().enumerate() {
            counts[parent as usize] += counts[merge.source as usize + offset];
        }
    }
}

/// Counts the selected rows of each group, those whose cell is valid if
/// `valid` is given.
fn count(valid: Option<&[bool]>, feed: Feed) -> Vec<u64> {
    let mut counts = vec![0u64; feed.groups];
    match valid {
        Some(valid) => {
            for (&row, &group) in feed.selection.iter().zip(feed.finest) {
                counts[group as usize] += u64::from(valid[row as usize]);
            }
        }
        None => {
            for &group in feed.finest {
                counts[group as usize] += 1;
            }
        }
    }
    merge_counts(&mut counts, feed.merges);
    counts
}

/// Pair of a group and of the code of a value, in one word, the group in the
/// high half: sorting the pairs gathers those of each group.
fn pair(group: u32, code: u32) -> u64 {
    (u64::from(group) << 32) | u64::from(code)
}

/// Removes the duplicate pairs, then visits the remaining ones: the group
/// and the code of each distinct value of each group.
fn visit_pairs(
    pairs: &mut Vec<u64>,
    visit: &mut impl FnMut(usize, usize) -> Result<(), Overflow>,
) -> Result<(), Overflow> {
    pairs.sort_unstable();
    pairs.dedup();
    for &pair in pairs.iter() {
        visit((pair >> 32) as usize, (pair as u32) as usize)?;
    }
    Ok(())
}

/// Visits the distinct valid codes of each group: the pairs of the finest
/// groups and of the codes of their rows, then, merge after merge, those
/// of each coarser set, taken from the pairs of the set it merges.
///
/// A set is known by its first group: the finest set starts at 0, and a
/// coarser set at the parent of the first group it merges, its groups
/// being numbered in that order.
fn visit_distinct(
    code_of: impl Fn(usize) -> u32,
    valid: &[bool],
    feed: Feed,
    mut visit: impl FnMut(usize, usize) -> Result<(), Overflow>,
) -> Result<(), Overflow> {
    let mut pairs: Vec<u64> = feed
        .selection
        .iter()
        .zip(feed.finest)
        .filter(|&(&row, _)| valid[row as usize])
        .map(|(&row, &group)| pair(group, code_of(row as usize)))
        .collect();
    visit_pairs(&mut pairs, &mut visit)?;
    let mut sets: FxMap<u32, Vec<u64>> = FxMap::default();
    sets.insert(0, pairs);
    for merge in feed.merges {
        let (Some(source), Some(&first)) = (sets.get(&merge.source), merge.parent_of.first())
        else {
            continue;
        };
        let mut pairs: Vec<u64> = source
            .iter()
            .map(|&child| {
                let group = (child >> 32) as u32;
                pair(
                    merge.parent_of[(group - merge.source) as usize],
                    child as u32,
                )
            })
            .collect();
        visit_pairs(&mut pairs, &mut visit)?;
        sets.insert(first, pairs);
    }
    Ok(())
}

/// Number that adds up.
trait Addend: Copy + Default {
    /// Sum of two numbers, an error when it does not hold.
    fn plus(self, other: Self) -> Result<Self, Overflow>;
}

impl Addend for i128 {
    fn plus(self, other: Self) -> Result<Self, Overflow> {
        self.checked_add(other).ok_or(Overflow)
    }
}

impl Addend for f64 {
    fn plus(self, other: Self) -> Result<Self, Overflow> {
        Ok(self + other)
    }
}

/// Adds the valid numbers of each group.
fn sum<T: Addend>(values: &[T], valid: &[bool], feed: Feed) -> Result<Vec<T>, Overflow> {
    let mut sums = vec![T::default(); feed.groups];
    for (&row, &group) in feed.selection.iter().zip(feed.finest) {
        if valid[row as usize] {
            let sum = &mut sums[group as usize];
            *sum = sum.plus(values[row as usize])?;
        }
    }
    for merge in feed.merges {
        for (offset, &parent) in merge.parent_of.iter().enumerate() {
            let value = sums[merge.source as usize + offset];
            let sum = &mut sums[parent as usize];
            *sum = sum.plus(value)?;
        }
    }
    Ok(sums)
}

/// Adds each distinct valid number of each group once, and counts them.
///
/// Two texts of the dictionary can write one number (`1.0` and `1.00`):
/// `key` tells the numbers apart, and the pairs read a code per number.
fn sum_distinct<T: Addend, K: Eq + Hash>(
    values: &[T],
    key: impl Fn(T) -> K,
    valid: &[bool],
    dictionary: &Dictionary,
    feed: Feed,
) -> Result<(Vec<T>, Vec<u64>), Overflow> {
    let mut index: FxMap<K, u32> = FxMap::default();
    let mut numbers = Vec::new();
    let number_of_text: Vec<u32> = dictionary
        .first_rows
        .iter()
        .map(|&row| {
            let value = values[row as usize];
            *index.entry(key(value)).or_insert_with(|| {
                numbers.push(value);
                (numbers.len() - 1) as u32
            })
        })
        .collect();
    let mut sums = vec![T::default(); feed.groups];
    let mut counts = vec![0u64; feed.groups];
    visit_distinct(
        |row| number_of_text[dictionary.codes[row] as usize],
        valid,
        feed,
        |group, number| {
            sums[group] = sums[group].plus(numbers[number])?;
            counts[group] += 1;
            Ok(())
        },
    )?;
    Ok((sums, counts))
}

/// Key of a float under `DISTINCT`: 0 and -0 are one value, and so are the
/// `NaN`.
fn float_key(value: f64) -> u64 {
    if value == 0.0 {
        0
    } else if value.is_nan() {
        f64::NAN.to_bits()
    } else {
        value.to_bits()
    }
}

/// Average of exact numbers, with `AVERAGE_DIGITS` more digits after the
/// point, rounded away from zero.
///
/// The division comes first: only an average that does not hold is an error.
fn average(sum: i128, count: u64) -> Result<i128, Overflow> {
    let count = i128::from(count);
    let factor = 10i128.pow(AVERAGE_DIGITS);
    // Below `count`, a word of 64 bits, the remainder holds once scaled.
    let remainder = (sum % count) * factor;
    let mut fraction = remainder / count;
    if (remainder % count).abs() * 2 >= count {
        fraction += remainder.signum();
    }
    (sum / count)
        .checked_mul(factor)
        .and_then(|whole| whole.checked_add(fraction))
        .ok_or(Overflow)
}

/// Keeps, in each group, the valid value that comes first in the order of
/// `extreme`, or last with `largest`, and gives the row of its cell.
fn extreme_rows(extreme: Extreme, feed: Feed) -> Vec<u32> {
    let Extreme {
        dictionary,
        ranks,
        valid,
        largest,
        ..
    } = extreme;
    let keep = |best: &mut u32, code: u32| {
        if code != NO_VALUE
            && (*best == NO_VALUE || (ranks[code as usize] > ranks[*best as usize]) == largest)
        {
            *best = code;
        }
    };
    let mut best = vec![NO_VALUE; feed.groups];
    for (&row, &group) in feed.selection.iter().zip(feed.finest) {
        if valid[row as usize] {
            keep(&mut best[group as usize], dictionary.codes[row as usize]);
        }
    }
    for merge in feed.merges {
        for (offset, &parent) in merge.parent_of.iter().enumerate() {
            let code = best[merge.source as usize + offset];
            keep(&mut best[parent as usize], code);
        }
    }
    best.into_iter()
        .map(|code| {
            let first_row = dictionary.first_rows.get(code as usize);
            first_row.copied().unwrap_or(NO_VALUE)
        })
        .collect()
}

/// Computes the states of a measure over the selected rows.
///
/// # Arguments
///
/// - `selection`: rows taken into account;
/// - `finest`: finest group of each selected row;
/// - `groups`: number of groups, all sets included;
/// - `merges`: merges of the sets of groups, in their order.
pub fn accumulate(
    input: &Input,
    selection: &[u32],
    finest: &[u32],
    groups: usize,
    merges: &[Merge],
) -> Result<State, Overflow> {
    let feed = Feed {
        selection,
        finest,
        groups,
        merges,
    };
    match *input {
        Input::Rows => Ok(State::Count(count(None, feed))),
        Input::Cells(valid) => Ok(State::Count(count(Some(valid), feed))),
        Input::Distinct(codes, valid) => {
            let mut counts = vec![0u64; groups];
            visit_distinct(
                |row| codes[row],
                valid,
                feed,
                |group, _| {
                    counts[group] += 1;
                    Ok(())
                },
            )?;
            Ok(State::Count(counts))
        }
        Input::Exact(numbers, valid) => Ok(State::Exact {
            sums: sum(&numbers.values, valid, feed)?,
            counts: count(Some(valid), feed),
            scale: numbers.scale,
        }),
        Input::Float(numbers, valid) => Ok(State::Float {
            sums: sum(&numbers.values, valid, feed)?,
            counts: count(Some(valid), feed),
        }),
        Input::DistinctExact(numbers, valid, dictionary) => {
            let (sums, counts) =
                sum_distinct(&numbers.values, |value| value, valid, dictionary, feed)?;
            Ok(State::Exact {
                sums,
                counts,
                scale: numbers.scale,
            })
        }
        Input::DistinctFloat(numbers, valid, dictionary) => {
            let (sums, counts) = sum_distinct(&numbers.values, float_key, valid, dictionary, feed)?;
            Ok(State::Float { sums, counts })
        }
        Input::Average(ref input) => {
            accumulate(input, selection, finest, groups, merges)?.averaged()
        }
        Input::Extreme(extreme) => Ok(State::Cells {
            level: extreme.level,
            rows: extreme_rows(extreme, feed),
        }),
    }
}

impl State {
    /// Divides the sums by their number of values.
    fn averaged(self) -> Result<State, Overflow> {
        match self {
            State::Exact {
                sums,
                counts,
                scale,
            } => {
                let scale = scale + AVERAGE_DIGITS;
                // Beyond, the digits after the point do not hold on 128 bits.
                10i128.checked_pow(scale).ok_or(Overflow)?;
                let sums = sums
                    .iter()
                    .zip(&counts)
                    .map(|(&sum, &count)| {
                        if count == 0 {
                            Ok(0)
                        } else {
                            average(sum, count)
                        }
                    })
                    .collect::<Result<_, _>>()?;
                Ok(State::Exact {
                    sums,
                    counts,
                    scale,
                })
            }
            State::Float { sums, counts } => {
                let sums = sums
                    .iter()
                    .zip(&counts)
                    .map(|(&sum, &count)| sum / count as f64)
                    .collect();
                Ok(State::Float { sums, counts })
            }
            state => Ok(state),
        }
    }

    /// Value of the measure for `group`.
    pub fn value(&self, group: u32) -> Value {
        let group = group as usize;
        match self {
            State::Count(counts) => Value::Count(counts[group]),
            State::Exact { counts, .. } | State::Float { counts, .. } if counts[group] == 0 => {
                Value::Null
            }
            State::Exact { sums, scale, .. } => Value::Exact(sums[group], *scale),
            State::Float { sums, .. } => Value::Float(sums[group]),
            State::Cells { level, rows } => match rows[group] {
                NO_VALUE => Value::Null,
                row => Value::Cell(*level, row),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use prisme_store::Store;

    use super::*;

    /// Engine without a column: it writes the numbers of a measure.
    fn no_engine() -> Engine {
        Engine::new(Store::new(0), Vec::new())
    }

    /// Groups 0 and 1 merge into group 2.
    fn merges() -> Vec<Merge> {
        vec![Merge {
            source: 0,
            parent_of: vec![2, 2],
        }]
    }

    #[test]
    fn counts_merge_into_the_coarser_groups() {
        let valid = [true, false, true];
        let state =
            accumulate(&Input::Cells(&valid), &[0, 1, 2], &[0, 0, 1], 3, &merges()).unwrap();
        let values: Vec<_> = (0..3).map(|group| state.value(group)).collect();
        assert_eq!(values, [Value::Count(1), Value::Count(1), Value::Count(2)]);
        let state = accumulate(&Input::Rows, &[0, 1, 2], &[0, 0, 1], 3, &merges()).unwrap();
        assert_eq!(state.value(2), Value::Count(3));
    }

    #[test]
    fn distinct_values_are_counted_again_in_the_coarser_groups() {
        // Codes 0, 1, 0 and a NULL (code 2, not valid): group 0 holds 0,
        // group 1 holds 1, 0 and NULL; group 2 merges them.
        let codes = [0, 1, 0, 2];
        let valid = [true, true, true, false];
        let state = accumulate(
            &Input::Distinct(&codes, &valid),
            &[0, 1, 2, 3],
            &[0, 1, 1, 1],
            3,
            &merges(),
        )
        .unwrap();
        let values: Vec<_> = (0..3).map(|group| state.value(group)).collect();
        assert_eq!(values, [Value::Count(1), Value::Count(2), Value::Count(2)]);
    }

    #[test]
    fn a_set_merged_twice_feeds_both_coarser_sets() {
        // Set {0, 1} merges into {2, 3}, then again into {4}: the second
        // merge reads the pairs of the first set, not those of {2, 3}.
        let merges = [
            Merge {
                source: 0,
                parent_of: vec![2, 3],
            },
            Merge {
                source: 0,
                parent_of: vec![4, 4],
            },
        ];
        let codes = [7, 7, 8];
        let valid = [true; 3];
        let state = accumulate(
            &Input::Distinct(&codes, &valid),
            &[0, 1, 2],
            &[0, 1, 1],
            5,
            &merges,
        )
        .unwrap();
        let values: Vec<_> = (0..5).map(|group| state.value(group)).collect();
        assert_eq!(values, [1, 2, 1, 2, 2].map(Value::Count),);
    }

    #[test]
    fn a_sum_without_a_value_is_null() {
        let numbers = Exact {
            values: vec![1250, 0, 325],
            scale: 2,
        };
        let valid = [true, false, true];
        let state = accumulate(
            &Input::Exact(&numbers, &valid),
            &[0, 1, 2],
            &[0, 1, 0],
            3,
            &merges(),
        )
        .unwrap();
        assert_eq!(state.value(0), Value::Exact(1575, 2));
        assert_eq!(state.value(1), Value::Null);
        assert_eq!(state.value(2), Value::Exact(1575, 2));
        assert_eq!(state.value(0).text(&no_engine()).as_deref(), Some("15.75"));
    }

    #[test]
    fn an_exact_sum_beyond_128_bits_is_an_error() {
        let numbers = Exact {
            values: vec![i128::MAX, 1],
            scale: 0,
        };
        let valid = [true, true];
        // In the same group, then only when merging.
        for finest in [[0, 0], [0, 1]] {
            let result = accumulate(
                &Input::Exact(&numbers, &valid),
                &[0, 1],
                &finest,
                3,
                &merges(),
            );
            assert_eq!(result.unwrap_err(), Overflow);
        }
    }

    #[test]
    fn floats_add_up_as_double() {
        let numbers = Approx {
            values: vec![0.1, 0.2],
        };
        let valid = [true, true];
        let state = accumulate(
            &Input::Float(&numbers, &valid),
            &[0, 1],
            &[0, 1],
            3,
            &merges(),
        )
        .unwrap();
        assert_eq!(state.value(2), Value::Float(0.1 + 0.2));
        assert_eq!(
            state.value(2).text(&no_engine()).as_deref(),
            Some("0.30000000000000004")
        );
    }

    #[test]
    fn distinct_numbers_are_told_apart_by_value_and_added_again_when_merging() {
        // 1.0 and 1.00 are one number, in one group as across two groups;
        // the NULL (code 3, not valid) is left out.
        let numbers = Exact {
            values: vec![100, 100, 200, 0],
            scale: 2,
        };
        let valid = [true, true, true, false];
        let dictionary = Dictionary {
            codes: vec![0, 1, 2, 3],
            first_rows: vec![0, 1, 2, 3],
        };
        let input = Input::DistinctExact(&numbers, &valid, &dictionary);
        let state = accumulate(&input, &[0, 1, 2, 3], &[0, 0, 1, 1], 3, &merges()).unwrap();
        let values: Vec<_> = (0..3).map(|group| state.value(group)).collect();
        assert_eq!(values, [100, 200, 300].map(|sum| Value::Exact(sum, 2)));
        let state = accumulate(&input, &[0, 1, 2, 3], &[0, 1, 0, 1], 3, &merges()).unwrap();
        let values: Vec<_> = (0..3).map(|group| state.value(group)).collect();
        assert_eq!(values, [300, 100, 300].map(|sum| Value::Exact(sum, 2)));
    }

    #[test]
    fn distinct_floats_take_zero_and_nan_once() {
        let numbers = Approx {
            values: vec![0.0, -0.0, 1.5, f64::NAN, f64::NAN],
        };
        let valid = [true; 5];
        let dictionary = Dictionary {
            codes: vec![0, 1, 2, 3, 3],
            first_rows: vec![0, 1, 2, 3],
        };
        let input = Input::DistinctFloat(&numbers, &valid, &dictionary);
        let state = accumulate(&input, &[0, 1, 2], &[0, 0, 0], 3, &merges()).unwrap();
        assert_eq!(state.value(0), Value::Float(1.5));
        let average = Input::Average(Box::new(input));
        let state = accumulate(&average, &[0, 1, 2], &[0, 0, 0], 3, &merges()).unwrap();
        assert_eq!(state.value(2), Value::Float(0.75));
        let state = accumulate(&average, &[3, 4], &[0, 0], 3, &merges()).unwrap();
        let Value::Float(value) = state.value(2) else {
            panic!("a float");
        };
        assert!(value.is_nan());
    }

    #[test]
    fn an_average_of_exact_numbers_has_four_more_digits_rounded_away_from_zero() {
        assert_eq!(average(3, 2), Ok(15_000));
        assert_eq!(average(5, 3), Ok(16_667));
        assert_eq!(average(-5, 3), Ok(-16_667));
        assert_eq!(average(1, 32), Ok(313));
        assert_eq!(average(-1, 32), Ok(-313));
        assert_eq!(average(i128::MAX, 1), Err(Overflow));
        // The sum holds, and so does its average, whatever the number of values.
        assert_eq!(
            average(i128::MAX, 10_000),
            Ok(i128::MAX / 10_000 * 10_000 + i128::MAX % 10_000)
        );
    }

    #[test]
    fn a_total_averages_the_values_of_its_group_and_not_the_averages() {
        let numbers = Exact {
            values: vec![1, 2, 6],
            scale: 0,
        };
        let valid = [true; 3];
        let input = Input::Average(Box::new(Input::Exact(&numbers, &valid)));
        let state = accumulate(&input, &[0, 1, 2], &[0, 0, 1], 3, &merges()).unwrap();
        let values: Vec<_> = (0..3).map(|group| state.value(group)).collect();
        assert_eq!(
            values,
            [15_000, 60_000, 30_000].map(|average| Value::Exact(average, 4))
        );

        let numbers = Approx {
            values: vec![1.0, 2.0, 6.0],
        };
        let input = Input::Average(Box::new(Input::Float(&numbers, &valid)));
        let state = accumulate(&input, &[0, 1, 2], &[0, 0, 1], 3, &merges()).unwrap();
        assert_eq!(state.value(2), Value::Float(3.0));
    }

    #[test]
    fn an_average_without_a_value_is_null() {
        let exact = Exact {
            values: vec![0],
            scale: 2,
        };
        let float = Approx { values: vec![0.0] };
        let valid = [false];
        for input in [Input::Exact(&exact, &valid), Input::Float(&float, &valid)] {
            let input = Input::Average(Box::new(input));
            let state = accumulate(&input, &[0], &[0], 3, &merges()).unwrap();
            assert_eq!(state.value(2), Value::Null);
        }
    }

    #[test]
    fn an_average_beyond_38_digits_after_the_point_is_an_error() {
        let valid = [true];
        for (scale, result) in [(34, true), (35, false)] {
            let numbers = Exact {
                values: vec![1],
                scale,
            };
            let input = Input::Average(Box::new(Input::Exact(&numbers, &valid)));
            let state = accumulate(&input, &[0], &[0], 3, &merges());
            assert_eq!(state.is_ok(), result, "scale {scale}");
        }
    }

    #[test]
    fn extremes_keep_the_cell_of_each_group_and_merge() {
        // Ranks by code: 1, 3, 0, 2; the NULL (code 4, not valid) is left out,
        // although its rank comes first.
        let dictionary = Dictionary {
            codes: vec![0, 1, 2, 3, 4],
            first_rows: vec![0, 1, 2, 3, 4],
        };
        let ranks = [2, 3, 1, 4, 0];
        let valid = [true, true, true, true, false];
        let extreme = |largest| {
            Input::Extreme(Extreme {
                level: Level::from(7),
                dictionary: &dictionary,
                ranks: &ranks,
                valid: &valid,
                largest,
            })
        };
        let selection = [0, 1, 2, 3, 4];
        let finest = [0, 0, 1, 1, 1];
        let values = |largest| {
            let state = accumulate(&extreme(largest), &selection, &finest, 3, &merges()).unwrap();
            [0, 1, 2].map(|group| state.value(group))
        };
        let cell = |row| Value::Cell(Level::from(7), row);
        assert_eq!(values(false), [0, 2, 2].map(cell));
        assert_eq!(values(true), [1, 3, 3].map(cell));

        let state = accumulate(&extreme(false), &[4], &[0], 3, &merges()).unwrap();
        assert_eq!(state.value(2), Value::Null);
    }
}
