//! Groups of a table: the finest ones, found through a hash table, the tree
//! of each axis, then the groups of the subtotals and of the totals, each
//! set built from a finer one (the method of the benchmark of the engine,
//! `docs/PERFORMANCES.md`, chapter 9).
//!
//! A group is a pair of nodes, one per axis: the node of the rows that share
//! the values of the first levels of the rows, the node of the columns that
//! share those of the first levels of the columns. The pairs of nodes of all
//! depths make up every subtotal and total, as `GROUP BY ROLLUP` on each
//! axis.

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

/// Numbers the selected rows by their finest group, the combination of
/// their codes on all the levels.
///
/// The codes of a row, read in a mixed radix, give the key of its group. If
/// the product of the numbers of codes goes beyond 64 bits, the keys so far
/// are first numbered through a hash table, which keeps them below the
/// number of rows: two groups never share a key.
///
/// # Returns
///
/// The finest group of each selected row, and the first row of each group.
pub fn finest(levels: &[&Dictionary], selection: &[u32]) -> (Vec<u32>, Vec<u32>) {
    let mut keys = vec![0u64; selection.len()];
    let mut radix = 1u64;
    for level in levels {
        let codes = level.len() as u64;
        radix = match radix.checked_mul(codes) {
            Some(product) => product,
            None => number(&mut keys) * codes,
        };
        for (key, &row) in keys.iter_mut().zip(selection) {
            *key = *key * codes + u64::from(level.codes[row as usize]);
        }
    }
    let mut index: FxMap<u64, u32> = FxMap::default();
    let mut first_rows = Vec::new();
    let mut groups = Vec::with_capacity(selection.len());
    for (&key, &row) in keys.iter().zip(selection) {
        let next = first_rows.len() as u32;
        groups.push(*index.entry(key).or_insert_with(|| {
            first_rows.push(row);
            next
        }));
    }
    (groups, first_rows)
}

/// Replaces keys by their numbers, in the order of their first appearance,
/// and returns the number of distinct keys.
fn number(keys: &mut [u64]) -> u64 {
    let mut index: FxMap<u64, u64> = FxMap::default();
    for key in keys.iter_mut() {
        let next = index.len() as u64;
        *key = *index.entry(*key).or_insert(next);
    }
    index.len() as u64
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

    /// Group of a node of the rows and a node of the columns, `None` if no
    /// row falls into both.
    pub fn find(&self, row: u32, column: u32) -> Option<u32> {
        self.index.get(&pair_key(row, column)).copied()
    }
}

#[cfg(test)]
mod tests {
    use prisme_store::TextColumn;

    use super::*;

    fn dictionary(cells: &[&str]) -> Dictionary {
        let mut column = TextColumn::default();
        for &cell in cells {
            column.push(Some(cell));
        }
        Dictionary::of(&column)
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
    fn keys_beyond_64_bits_are_numbered_first() {
        // 2^22 codes per level over three levels: 2^66 combinations. Only the
        // number of codes and the codes of the selected rows matter.
        let level = |codes: [u32; 3]| Dictionary {
            codes: codes.to_vec(),
            first_rows: vec![0; 1 << 22],
        };
        let levels = [level([0, 1, 4_000_000]), level([1, 2, 5]), level([2, 3, 7])];
        let (groups, first_rows) = finest(&[&levels[0], &levels[1], &levels[2]], &[0, 1, 0, 2]);
        assert_eq!(groups, [0, 1, 0, 2]);
        assert_eq!(first_rows, [0, 1, 2]);
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
