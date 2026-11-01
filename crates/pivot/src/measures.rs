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

use prisme_store::{Approx, Exact, FxMap};

use crate::groups::Merge;

/// Values a measure reads.
pub enum Input<'a> {
    /// The rows themselves (`COUNT(*)`).
    Rows,
    /// Validity of the cells of a column (`COUNT(col)`).
    Cells(&'a [bool]),
    /// Codes of the values of a column, by row, and their validity
    /// (`COUNT(DISTINCT col)`).
    Distinct(&'a [u32], &'a [bool]),
    /// Exact numbers of a column and their validity.
    Exact(&'a Exact, &'a [bool]),
    /// Floats of a column and their validity.
    Float(&'a Approx, &'a [bool]),
}

/// Sum of exact numbers beyond 128 bits.
#[derive(Debug, PartialEq, Eq)]
pub struct Overflow;

/// States of a measure, one per group.
#[derive(Debug)]
pub enum State {
    /// Counts.
    Count(Vec<u64>),
    /// Sums of exact numbers, scaled to `scale`, and number of values added.
    Exact {
        /// Sum of each group.
        sums: Vec<i128>,
        /// Number of values added in each group.
        counts: Vec<u64>,
        /// Number of digits after the point.
        scale: u32,
    },
    /// Sums of floats, and number of values added.
    Float {
        /// Sum of each group.
        sums: Vec<f64>,
        /// Number of values added in each group.
        counts: Vec<u64>,
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
}

impl Value {
    /// Text of the value: `None` for an empty cell or NULL.
    pub fn text(self) -> Option<String> {
        match self {
            Value::Empty | Value::Null => None,
            Value::Count(count) => Some(count.to_string()),
            Value::Exact(value, scale) => Some(prisme_store::format_exact(value, scale)),
            Value::Float(value) => Some(prisme_store::format_float(value)),
        }
    }
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
fn count(
    valid: Option<&[bool]>,
    selection: &[u32],
    finest: &[u32],
    groups: usize,
    merges: &[Merge],
) -> Vec<u64> {
    let mut counts = vec![0u64; groups];
    match valid {
        Some(valid) => {
            for (&row, &group) in selection.iter().zip(finest) {
                counts[group as usize] += u64::from(valid[row as usize]);
            }
        }
        None => {
            for &group in finest {
                counts[group as usize] += 1;
            }
        }
    }
    merge_counts(&mut counts, merges);
    counts
}

/// Pair of a group and of the code of a value, in one word, the group in the
/// high half: sorting the pairs gathers those of each group.
fn pair(group: u32, code: u32) -> u64 {
    (u64::from(group) << 32) | u64::from(code)
}

/// Removes the duplicate pairs, then counts the remaining ones of each
/// group: its distinct values.
fn count_pairs(pairs: &mut Vec<u64>, counts: &mut [u64]) {
    pairs.sort_unstable();
    pairs.dedup();
    for &pair in pairs.iter() {
        counts[(pair >> 32) as usize] += 1;
    }
}

/// Counts the distinct valid codes of each group: the pairs of the finest
/// groups and of the codes of their rows, then, merge after merge, those
/// of each coarser set, taken from the pairs of the set it merges.
///
/// A set is known by its first group: the finest set starts at 0, and a
/// coarser set at the parent of the first group it merges, its groups
/// being numbered in that order.
fn count_distinct(
    codes: &[u32],
    valid: &[bool],
    selection: &[u32],
    finest: &[u32],
    groups: usize,
    merges: &[Merge],
) -> Vec<u64> {
    let mut counts = vec![0u64; groups];
    let mut pairs: Vec<u64> = selection
        .iter()
        .zip(finest)
        .filter(|&(&row, _)| valid[row as usize])
        .map(|(&row, &group)| pair(group, codes[row as usize]))
        .collect();
    count_pairs(&mut pairs, &mut counts);
    let mut sets: FxMap<u32, Vec<u64>> = FxMap::default();
    sets.insert(0, pairs);
    for merge in merges {
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
        count_pairs(&mut pairs, &mut counts);
        sets.insert(first, pairs);
    }
    counts
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
    match *input {
        Input::Rows => Ok(State::Count(count(None, selection, finest, groups, merges))),
        Input::Cells(valid) => Ok(State::Count(count(
            Some(valid),
            selection,
            finest,
            groups,
            merges,
        ))),
        Input::Distinct(codes, valid) => Ok(State::Count(count_distinct(
            codes, valid, selection, finest, groups, merges,
        ))),
        Input::Exact(numbers, valid) => {
            let mut sums = vec![0i128; groups];
            for (&row, &group) in selection.iter().zip(finest) {
                if valid[row as usize] {
                    let sum = &mut sums[group as usize];
                    *sum = sum
                        .checked_add(numbers.values[row as usize])
                        .ok_or(Overflow)?;
                }
            }
            for merge in merges {
                for (offset, &parent) in merge.parent_of.iter().enumerate() {
                    let value = sums[merge.source as usize + offset];
                    let sum = &mut sums[parent as usize];
                    *sum = sum.checked_add(value).ok_or(Overflow)?;
                }
            }
            Ok(State::Exact {
                sums,
                counts: count(Some(valid), selection, finest, groups, merges),
                scale: numbers.scale,
            })
        }
        Input::Float(numbers, valid) => {
            let mut sums = vec![0f64; groups];
            for (&row, &group) in selection.iter().zip(finest) {
                if valid[row as usize] {
                    sums[group as usize] += numbers.values[row as usize];
                }
            }
            for merge in merges {
                for (offset, &parent) in merge.parent_of.iter().enumerate() {
                    sums[parent as usize] += sums[merge.source as usize + offset];
                }
            }
            Ok(State::Float {
                sums,
                counts: count(Some(valid), selection, finest, groups, merges),
            })
        }
    }
}

impl State {
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
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(state.value(0).text().as_deref(), Some("15.75"));
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
            state.value(2).text().as_deref(),
            Some("0.30000000000000004")
        );
    }
}
