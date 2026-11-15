//! Groups of a table: the finest ones, found through a hash table, the tree
//! of each axis, then the groups of the subtotals and of the totals, each
//! set built from a finer one (the method of the benchmark of the engine,
//! `docs/PERFORMANCES.md`, chapter 9).
//!
//! The finest groups keep their index (`Finest`): the rows that arrive later
//! join the groups already found, which keep their numbers, or make new ones
//! after them. The key of a group packs the codes of its levels side by side,
//! each on the bits its dictionary needs, in 64 bits or 128; a dictionary
//! that outgrows them widens the keys, built again from the first row of
//! each group.
//!
//! A group is a pair of nodes, one per axis: the node of the rows that share
//! the values of the first levels of the rows, the node of the columns that
//! share those of the first levels of the columns. The pairs of nodes of all
//! depths make up every subtotal and total, as `GROUP BY ROLLUP` on each
//! axis.

use std::hash::Hash;
use std::ops::{BitOr, Shl};

use prisme_store::{Dictionary, FxMap};

/// Node of the tree of an axis: the rows that share the values of the
/// levels of the axis down to its depth. Node 0 is the root, all the rows.
#[derive(Debug, Clone, Copy)]
pub struct Node {
    /// Parent node; 0 for the root itself.
    pub parent: u32,
    /// Code of the value of the node in the dictionary of its level.
    pub code: u32,
    /// First row of the node: its value is the cell of its level there.
    pub row: u32,
    /// Depth: 0 for the root, then the number of the level.
    pub depth: u32,
}

/// Tree of an axis.
#[derive(Debug)]
pub struct Tree {
    /// Nodes, the root first.
    pub nodes: Vec<Node>,
}

/// Bits that hold the codes of a dictionary of `codes` codes: none for a
/// single code.
fn bits(codes: usize) -> u32 {
    usize::BITS - codes.saturating_sub(1).leading_zeros()
}

/// Key of a finest group in which the codes of its levels hold side by side.
trait Packed: Copy + Eq + Hash + From<u32> + Shl<u32, Output = Self> + BitOr<Output = Self> {
    /// Key without any code.
    const ZERO: Self;
}

impl Packed for u64 {
    const ZERO: Self = 0;
}

impl Packed for u128 {
    const ZERO: Self = 0;
}

/// Packed key of each row: the codes of its levels side by side, the first
/// level in the high bits, each on its width. The widths add up to the bits
/// of the key at most.
fn packed_keys<K: Packed>(levels: &[&Dictionary], widths: &[u32], rows: &[u32]) -> Vec<K> {
    let mut keys = vec![K::ZERO; rows.len()];
    for (level, &width) in levels.iter().zip(widths) {
        for (key, &row) in keys.iter_mut().zip(rows) {
            *key = (*key << width) | K::from(level.codes[row as usize]);
        }
    }
    keys
}

/// Codes of the levels of a row, the first level first.
fn row_codes(levels: &[&Dictionary], row: u32) -> Vec<u32> {
    levels
        .iter()
        .map(|level| level.codes[row as usize])
        .collect()
}

/// Group of each key, the keys of each row given in order; a key met for the
/// first time makes a new group, whose first row is its row.
fn find<K: Eq + Hash>(
    index: &mut FxMap<K, u32>,
    keys: impl Iterator<Item = K>,
    rows: &[u32],
    first_rows: &mut Vec<u32>,
) -> Vec<u32> {
    let mut groups = Vec::with_capacity(rows.len());
    for (key, &row) in keys.zip(rows) {
        let next = first_rows.len() as u32;
        groups.push(*index.entry(key).or_insert_with(|| {
            first_rows.push(row);
            next
        }));
    }
    groups
}

/// Index of the finest groups, by the bits their keys need.
#[derive(Debug)]
enum Keys {
    /// Group of each key, its codes packed into 64 bits.
    Narrow(FxMap<u64, u32>),
    /// Group of each key, its codes packed into 128 bits.
    Wide(FxMap<u128, u32>),
    /// Group of the codes of each row, when they do not hold in 128 bits.
    Codes(FxMap<Vec<u32>, u32>),
}

/// Finest groups of a table: the combinations of the codes of their rows on
/// all the levels, numbered in the order of their first appearance.
#[derive(Debug)]
pub struct Finest {
    /// Width of the codes of each level in a packed key: the bits of the codes
    /// of its dictionary when the keys were last built.
    widths: Vec<u32>,
    /// Group of each combination of codes.
    keys: Keys,
    /// First row of each group.
    pub first_rows: Vec<u32>,
}

impl Finest {
    /// Finest groups of no row, on `levels` levels.
    pub fn new(levels: usize) -> Self {
        Finest {
            widths: vec![0; levels],
            keys: Keys::Narrow(FxMap::default()),
            first_rows: Vec::new(),
        }
    }

    /// Number of groups.
    pub fn len(&self) -> usize {
        self.first_rows.len()
    }

    /// Finds the finest group of each row, a combination of codes met for the
    /// first time making a new group.
    ///
    /// # Panics
    ///
    /// If the levels are not those of the groups.
    pub fn add(&mut self, levels: &[&Dictionary], rows: &[u32]) -> Vec<u32> {
        assert_eq!(levels.len(), self.widths.len(), "the levels of the groups");
        let widths: Vec<u32> = levels.iter().map(|level| bits(level.len())).collect();
        if widths.iter().zip(&self.widths).any(|(new, old)| new > old) {
            self.widen(levels, widths);
        }
        let Finest {
            widths,
            keys,
            first_rows,
        } = self;
        match keys {
            Keys::Narrow(index) => {
                let keys = packed_keys::<u64>(levels, widths, rows);
                find(index, keys.into_iter(), rows, first_rows)
            }
            Keys::Wide(index) => {
                let keys = packed_keys::<u128>(levels, widths, rows);
                find(index, keys.into_iter(), rows, first_rows)
            }
            Keys::Codes(index) => {
                let keys = rows.iter().map(|&row| row_codes(levels, row));
                find(index, keys, rows, first_rows)
            }
        }
    }

    /// Builds the keys of the groups again, on the widths their levels need
    /// now: packed into 64 bits, or 128, or beyond, the codes of each group end
    /// to end. Groups whose codes are end to end stay so.
    fn widen(&mut self, levels: &[&Dictionary], widths: Vec<u32>) {
        self.widths = widths;
        if matches!(self.keys, Keys::Codes(_)) {
            return;
        }
        let rows = &self.first_rows;
        let groups = 0..rows.len() as u32;
        self.keys = match self.widths.iter().sum::<u32>() {
            0..=64 => {
                let keys = packed_keys::<u64>(levels, &self.widths, rows);
                Keys::Narrow(keys.into_iter().zip(groups).collect())
            }
            65..=128 => {
                let keys = packed_keys::<u128>(levels, &self.widths, rows);
                Keys::Wide(keys.into_iter().zip(groups).collect())
            }
            _ => {
                let keys = rows.iter().map(|&row| row_codes(levels, row));
                Keys::Codes(keys.zip(groups).collect())
            }
        };
    }
}

/// Builds the tree of an axis from the first rows of the finest groups.
///
/// # Returns
///
/// The tree, and the deepest node of each finest group (the root if the
/// axis has no level).
pub fn tree(levels: &[&Dictionary], first_rows: &[u32]) -> (Tree, Vec<u32>) {
    let mut nodes = vec![Node {
        parent: 0,
        code: 0,
        row: 0,
        depth: 0,
    }];
    let mut index: FxMap<u64, u32> = FxMap::default();
    let mut leaves = Vec::with_capacity(first_rows.len());
    for &row in first_rows {
        let mut node = 0u32;
        for (depth, level) in (1..).zip(levels) {
            let code = level.codes[row as usize];
            let parent = node;
            let next = nodes.len() as u32;
            node = *index
                .entry((u64::from(parent) << 32) | u64::from(code))
                .or_insert_with(|| {
                    nodes.push(Node {
                        parent,
                        code,
                        row,
                        depth,
                    });
                    next
                });
        }
        leaves.push(node);
    }
    (Tree { nodes }, leaves)
}

/// Groups of a set, merged into the groups of a coarser set.
#[derive(Debug)]
pub struct Merge {
    /// First group of the source set; its groups follow one another.
    pub source: u32,
    /// Group of the coarser set of each group of the source set, in order.
    pub parent_of: Vec<u32>,
}

/// Every group of a table: the finest ones first, then those of each set of
/// subtotals or totals, in the order of the merges.
#[derive(Debug)]
pub struct Groups {
    /// Node of the rows and node of the columns of each group.
    pairs: Vec<(u32, u32)>,
    /// Group of each pair of nodes.
    index: FxMap<u64, u32>,
    /// Merges, each from a set already complete into a coarser one.
    pub merges: Vec<Merge>,
}

/// Key of a pair of nodes.
fn pair_key(row: u32, column: u32) -> u64 {
    (u64::from(row) << 32) | u64::from(column)
}

impl Groups {
    /// Builds the sets of groups of a table, from the deepest nodes of each
    /// finest group: the set that keeps `r` levels of rows and `c` levels of
    /// columns comes from the one with one more level of columns, or, when
    /// it keeps them all, from the one with one more level of rows.
    ///
    /// The grand total is always there, without rows too, as the empty
    /// grouping set of SQL: `COUNT(*)` then gives 0, `SUM` NULL.
    pub fn new(rows: &Tree, columns: &Tree, row_leaves: &[u32], column_leaves: &[u32]) -> Self {
        let row_depth = rows.nodes.iter().map(|node| node.depth).max().unwrap_or(0) as usize;
        let column_depth = columns
            .nodes
            .iter()
            .map(|node| node.depth)
            .max()
            .unwrap_or(0) as usize;
        let mut pairs: Vec<(u32, u32)> = row_leaves
            .iter()
            .copied()
            .zip(column_leaves.iter().copied())
            .collect();
        let mut index: FxMap<u64, u32> = FxMap::default();
        index.reserve(pairs.len());
        for (group, &(row, column)) in pairs.iter().enumerate() {
            index.insert(pair_key(row, column), group as u32);
        }
        let mut sets = vec![vec![(0u32, 0u32); column_depth + 1]; row_depth + 1];
        sets[row_depth][column_depth] = (0, pairs.len() as u32);
        let mut merges = Vec::new();
        for kept_rows in (0..=row_depth).rev() {
            for kept_columns in (0..=column_depth).rev() {
                if (kept_rows, kept_columns) == (row_depth, column_depth) {
                    continue;
                }
                let from_columns = kept_columns < column_depth;
                let (start, count) = if from_columns {
                    sets[kept_rows][kept_columns + 1]
                } else {
                    sets[kept_rows + 1][kept_columns]
                };
                let first = pairs.len() as u32;
                let mut parent_of = Vec::with_capacity(count as usize);
                for group in start..start + count {
                    let (row, column) = pairs[group as usize];
                    let parent = if from_columns {
                        (row, columns.nodes[column as usize].parent)
                    } else {
                        (rows.nodes[row as usize].parent, column)
                    };
                    let next = pairs.len() as u32;
                    parent_of.push(*index.entry(pair_key(parent.0, parent.1)).or_insert_with(
                        || {
                            pairs.push(parent);
                            next
                        },
                    ));
                }
                sets[kept_rows][kept_columns] = (first, pairs.len() as u32 - first);
                merges.push(Merge {
                    source: start,
                    parent_of,
                });
            }
        }
        let next = pairs.len() as u32;
        index.entry(pair_key(0, 0)).or_insert_with(|| {
            pairs.push((0, 0));
            next
        });
        Groups {
            pairs,
            index,
            merges,
        }
    }

    /// Number of groups, all sets included.
    pub fn len(&self) -> usize {
        self.pairs.len()
    }

    /// Node of the rows and node of the columns of each group, by group.
    pub fn pairs(&self) -> &[(u32, u32)] {
        &self.pairs
    }

    /// Group of a node of the rows and a node of the columns, `None` if no
    /// row falls into both.
    pub fn find(&self, row: u32, column: u32) -> Option<u32> {
        self.index.get(&pair_key(row, column)).copied()
    }
}

#[cfg(test)]
mod tests {
    use prisme_store::{Encoder, TextColumn};

    use super::*;

    fn dictionary(cells: &[&str]) -> Dictionary {
        let mut column = TextColumn::default();
        for &cell in cells {
            column.push(Some(cell));
        }
        Dictionary::of(&column)
    }

    /// Finest group of each row, and the first row of each group.
    fn finest(levels: &[&Dictionary], rows: &[u32]) -> (Vec<u32>, Vec<u32>) {
        let mut finest = Finest::new(levels.len());
        let groups = finest.add(levels, rows);
        (groups, finest.first_rows)
    }

    #[test]
    fn rows_with_the_same_codes_share_their_finest_group() {
        let region = dictionary(&["Nord", "Nord", "Sud", "Nord"]);
        let year = dictionary(&["2025", "2026", "2025", "2025"]);
        let (groups, first_rows) = finest(&[&region, &year], &[0, 1, 2, 3]);
        assert_eq!(groups, [0, 1, 2, 0]);
        assert_eq!(first_rows, [0, 1, 2]);
    }

    #[test]
    fn only_the_selected_rows_are_grouped() {
        let region = dictionary(&["Nord", "Sud", "Nord"]);
        let (groups, first_rows) = finest(&[&region], &[1, 2]);
        assert_eq!(groups, [0, 1]);
        assert_eq!(first_rows, [1, 2]);
    }

    #[test]
    fn codes_beyond_128_bits_are_kept_end_to_end() {
        // 2^22 codes per level over six levels: 132 bits. Only the number of
        // codes and the codes of the rows matter.
        let level = |codes: [u32; 3]| Dictionary {
            codes: codes.to_vec(),
            first_rows: vec![0; 1 << 22],
        };
        let levels = [
            level([0, 1, 4_000_000]),
            level([1, 2, 5]),
            level([2, 3, 7]),
            level([3, 3, 3]),
            level([4, 4, 4]),
            level([5, 5, 5]),
        ];
        let levels: Vec<&Dictionary> = levels.iter().collect();
        // Four of them hold in 128 bits.
        let mut wide = Finest::new(4);
        assert_eq!(wide.add(&levels[..4], &[0, 1, 0, 2]), [0, 1, 0, 2]);
        assert!(matches!(wide.keys, Keys::Wide(_)));
        let mut finest = Finest::new(6);
        assert_eq!(finest.add(&levels, &[0, 1, 0]), [0, 1, 0]);
        assert!(matches!(finest.keys, Keys::Codes(_)));
        assert_eq!(finest.add(&levels, &[2, 1]), [2, 1]);
        assert_eq!(finest.first_rows, [0, 1, 2]);
    }

    #[test]
    fn keys_widen_when_a_dictionary_outgrows_their_bits() {
        let mut column = TextColumn::default();
        let mut cells = Encoder::default();
        let mut finest = Finest::new(2);
        let year = dictionary(&["2025"; 9]);
        let mut groups = Vec::new();
        for (block, texts) in [["a", "b", "a"], ["c", "d", "e"], ["a", "e", "f"]]
            .iter()
            .enumerate()
        {
            for text in texts {
                column.push(Some(text));
            }
            cells.extend(&column, column.len());
            let rows: Vec<u32> = (block as u32 * 3..block as u32 * 3 + 3).collect();
            groups.extend(finest.add(&[cells.dictionary(), &year], &rows));
        }
        assert_eq!(groups, [0, 1, 0, 2, 3, 4, 0, 4, 5]);
        assert_eq!(finest.widths, [3, 0]);
    }

    #[test]
    fn rows_read_later_join_the_groups_found_before() {
        let region = dictionary(&["Nord", "Sud", "Nord", "Est", "Sud"]);
        let mut finest = Finest::new(1);
        assert_eq!(finest.add(&[&region], &[0, 1]), [0, 1]);
        assert_eq!(finest.add(&[&region], &[2, 3, 4]), [0, 2, 1]);
        assert_eq!(finest.first_rows, [0, 1, 3]);
        // Without a level, all the rows make one group.
        let mut all = Finest::new(0);
        assert_eq!(all.add(&[], &[0, 1, 2]), [0, 0, 0]);
        assert_eq!(all.len(), 1);
    }

    #[test]
    fn each_set_merges_into_a_coarser_one_down_to_the_grand_total() {
        let region = dictionary(&["Nord", "Nord", "Sud"]);
        let year = dictionary(&["2025", "2026", "2025"]);
        let (_, first_rows) = finest(&[&region, &year], &[0, 1, 2]);
        let (rows, row_leaves) = tree(&[&region], &first_rows);
        let (columns, column_leaves) = tree(&[&year], &first_rows);
        let groups = Groups::new(&rows, &columns, &row_leaves, &column_leaves);
        // 3 finest groups, 2 regions, 2 years, the grand total.
        assert_eq!(groups.len(), 8);
        assert_eq!(groups.merges.len(), 3);
        assert!(groups.find(0, 0).is_some());
        let sud = row_leaves[2];
        let year_2026 = column_leaves[1];
        assert!(groups.find(sud, year_2026).is_none());
    }

    #[test]
    fn the_grand_total_is_there_without_rows() {
        let empty = dictionary(&[]);
        let (_, first_rows) = finest(&[&empty], &[]);
        let (rows, row_leaves) = tree(&[&empty], &first_rows);
        let (columns, column_leaves) = tree(&[], &first_rows);
        let groups = Groups::new(&rows, &columns, &row_leaves, &column_leaves);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups.find(0, 0), Some(0));
    }
}
