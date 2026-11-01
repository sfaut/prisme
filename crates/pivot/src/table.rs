//! Layout of a computed table, in its compact form: every subtotal and the
//! grand total on both axes.
//!
//! - Lines: one per node of the rows, the value of a group on its own line,
//!   above the lines of its members (its subtotal), the grand total last.
//! - Columns: one group of columns per node of the columns, a group after
//!   its members (its subtotal to their right), the grand total last; one
//!   column per measure in each group.
//!
//! The nodes of each level follow the ranks of their values, given by the
//! page in the order of the grid.

use prisme_store::Store;

use crate::groups::{Groups, Tree};
use crate::measures::{State, Value};

/// Label of a line or of a level of the header of a column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Label<'a> {
    /// Value of a level: its text, `None` for NULL.
    Value(Option<&'a str>),
    /// Total of the levels it covers.
    Total,
}

/// Line of the table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Line<'a> {
    /// Depth of its node: 0 for the grand total, the number of the level
    /// otherwise.
    pub depth: usize,
    /// Label: the value of its level, or `Total` for the grand total.
    pub label: Label<'a>,
}

/// Header of a column of the table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header<'a> {
    /// Node of the columns of the column.
    pub node: u32,
    /// Node and value of each level of the column, the outermost first: the
    /// columns that share a node at a level share its header cell there.
    pub path: Vec<(u32, Option<&'a str>)>,
    /// Is the column a subtotal or the grand total? Its header then reads
    /// `Total` on the level after its path.
    pub total: bool,
    /// Measure of the column, `None` without any measure.
    pub measure: Option<usize>,
}

/// Children of each node of a tree, in the order of the ranks of their
/// values: those of node `n` are `order[starts[n]..starts[n + 1]]`.
struct Children {
    /// Nodes other than the root, grouped by parent, in the order of the ranks.
    order: Vec<u32>,
    /// Start of the children of each node in `order`, then its length.
    starts: Vec<u32>,
}

impl Children {
    /// Sorts the children of each node.
    ///
    /// `ranks[level][code]` is the rank of the value `code` of the level
    /// `level` (0 for the outermost).
    fn new(tree: &Tree, ranks: &[&[u32]]) -> Self {
        let mut order: Vec<u32> = (1..tree.nodes.len() as u32).collect();
        order.sort_unstable_by_key(|&id| {
            let node = &tree.nodes[id as usize];
            (
                node.parent,
                ranks[node.depth as usize - 1][node.code as usize],
            )
        });
        let mut starts = vec![0u32; tree.nodes.len() + 1];
        for &id in &order {
            starts[tree.nodes[id as usize].parent as usize + 1] += 1;
        }
        for index in 1..starts.len() {
            starts[index] += starts[index - 1];
        }
        Children { order, starts }
    }

    /// Children of `node`.
    fn of(&self, node: u32) -> &[u32] {
        &self.order[self.starts[node as usize] as usize..self.starts[node as usize + 1] as usize]
    }

    /// Nodes in the order of the lines: each node before its children, the
    /// root last.
    fn lines(&self) -> Vec<u32> {
        let mut lines = Vec::with_capacity(self.order.len() + 1);
        let mut stack: Vec<u32> = self.of(0).iter().rev().copied().collect();
        while let Some(node) = stack.pop() {
            lines.push(node);
            stack.extend(self.of(node).iter().rev());
        }
        lines.push(0);
        lines
    }

    /// Nodes in the order of the columns: each node after its children, the
    /// root last.
    fn columns(&self) -> Vec<u32> {
        let mut columns = Vec::with_capacity(self.order.len() + 1);
        let mut stack = vec![(0u32, false)];
        while let Some((node, expanded)) = stack.pop() {
            let children = self.of(node);
            if expanded || children.is_empty() {
                columns.push(node);
            } else {
                stack.push((node, true));
                stack.extend(children.iter().rev().map(|&child| (child, false)));
            }
        }
        columns
    }
}

/// Computed table.
#[derive(Debug)]
pub struct Table {
    /// Groups, all sets included.
    groups: Groups,
    /// Tree of the rows.
    rows: Tree,
    /// Tree of the columns.
    columns: Tree,
    /// Columns of the result of the levels of the rows, then of the columns.
    levels: Vec<usize>,
    /// Number of levels of the rows.
    row_levels: usize,
    /// Number of levels of the columns.
    column_levels: usize,
    /// Node of the rows of each line.
    lines: Vec<u32>,
    /// Node of the columns of each group of columns.
    column_nodes: Vec<u32>,
    /// Columns per group of columns: one per measure, one without a measure
    /// when the table has levels of columns.
    slots: usize,
    /// States of each measure.
    states: Vec<State>,
    /// Number of finest groups.
    finest_groups: usize,
    /// Number of rows taken into account.
    selected_rows: usize,
}

/// Parts of a table computed by the engine.
pub struct Parts {
    /// Groups, all sets included.
    pub groups: Groups,
    /// Tree of the rows.
    pub rows: Tree,
    /// Tree of the columns.
    pub columns: Tree,
    /// Columns of the result of the levels of the rows.
    pub row_levels: Vec<usize>,
    /// Columns of the result of the levels of the columns.
    pub column_levels: Vec<usize>,
    /// States of each measure.
    pub states: Vec<State>,
    /// Number of finest groups.
    pub finest_groups: usize,
    /// Number of rows taken into account.
    pub selected_rows: usize,
}

impl Table {
    /// Lays out the parts of a table.
    ///
    /// `row_ranks` and `column_ranks` give the rank of each value of each
    /// level, by code.
    pub fn new(parts: Parts, row_ranks: &[&[u32]], column_ranks: &[&[u32]]) -> Self {
        let lines = Children::new(&parts.rows, row_ranks).lines();
        let column_nodes = Children::new(&parts.columns, column_ranks).columns();
        let (row_levels, column_levels) = (parts.row_levels.len(), parts.column_levels.len());
        let measures = parts.states.len();
        let slots = if column_levels > 0 {
            measures.max(1)
        } else {
            measures
        };
        let mut levels = parts.row_levels;
        levels.extend(parts.column_levels);
        Table {
            groups: parts.groups,
            rows: parts.rows,
            columns: parts.columns,
            levels,
            row_levels,
            column_levels,
            lines,
            column_nodes,
            slots,
            states: parts.states,
            finest_groups: parts.finest_groups,
            selected_rows: parts.selected_rows,
        }
    }

    /// Number of lines.
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    /// Number of columns of values, the labels of the lines apart.
    pub fn column_count(&self) -> usize {
        self.column_nodes.len() * self.slots
    }

    /// Number of finest groups: the distinct combinations of the values of
    /// all the levels.
    pub fn finest_groups(&self) -> usize {
        self.finest_groups
    }

    /// Number of rows taken into account.
    pub fn selected_rows(&self) -> usize {
        self.selected_rows
    }

    /// Number of levels of the rows.
    pub fn row_levels(&self) -> usize {
        self.row_levels
    }

    /// Number of levels of the columns.
    pub fn column_levels(&self) -> usize {
        self.column_levels
    }

    /// Value of the node of a level, read in the store.
    fn label<'a>(&self, store: &'a Store, level: usize, row: u32) -> Option<&'a str> {
        store
            .column(self.levels[level])
            .and_then(|column| column.get(row as usize))
    }

    /// Line `index`.
    ///
    /// # Panics
    ///
    /// If `index` is not a line of the table.
    pub fn line<'a>(&self, store: &'a Store, index: usize) -> Line<'a> {
        let node = self.rows.nodes[self.lines[index] as usize];
        let depth = node.depth as usize;
        let label = match depth {
            0 => Label::Total,
            _ => Label::Value(self.label(store, depth - 1, node.row)),
        };
        Line { depth, label }
    }

    /// Values of the levels of line `index`, the outermost first: those of its
    /// node and of its ancestors.
    ///
    /// # Panics
    ///
    /// If `index` is not a line of the table.
    pub fn line_path<'a>(&self, store: &'a Store, index: usize) -> Vec<Option<&'a str>> {
        let mut path = Vec::new();
        let mut node = self.rows.nodes[self.lines[index] as usize];
        while node.depth > 0 {
            path.push(self.label(store, node.depth as usize - 1, node.row));
            node = self.rows.nodes[node.parent as usize];
        }
        path.reverse();
        path
    }

    /// Header of column `index`.
    ///
    /// # Panics
    ///
    /// If `index` is not a column of the table.
    pub fn header<'a>(&self, store: &'a Store, index: usize) -> Header<'a> {
        let node_id = self.column_nodes[index / self.slots];
        let mut path = Vec::new();
        let mut node = self.columns.nodes[node_id as usize];
        let mut id = node_id;
        while node.depth > 0 {
            let level = self.row_levels + node.depth as usize - 1;
            path.push((id, self.label(store, level, node.row)));
            id = node.parent;
            node = self.columns.nodes[id as usize];
        }
        path.reverse();
        Header {
            node: node_id,
            total: path.len() < self.column_levels,
            path,
            measure: (!self.states.is_empty()).then_some(index % self.slots),
        }
    }

    /// Value of the cell of line `line` and column `column`.
    ///
    /// # Panics
    ///
    /// If the line or the column is not in the table.
    pub fn value(&self, line: usize, column: usize) -> Value {
        let row_node = self.lines[line];
        let column_node = self.column_nodes[column / self.slots];
        let measure = column % self.slots;
        match (
            self.groups.find(row_node, column_node),
            self.states.get(measure),
        ) {
            (Some(group), Some(state)) => state.value(group),
            _ => Value::Empty,
        }
    }
}
