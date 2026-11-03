//! Layout of a computed table: its lines and its columns, among the nodes of
//! each axis, which hold every subtotal and the grand total.
//!
//! - Lines, the grand total last. In compact and outline forms, a group has a
//!   line above those of its members, which stays empty without its subtotal.
//!   In tabular form, the line of a group comes after those of its members,
//!   and only with its subtotal.
//! - Columns: one group of columns per node of the columns, a group after
//!   its members (its subtotal to their right), the grand total last; one
//!   column per measure in each group. A total not shown has no column.
//!
//! The nodes of each level follow the ranks of their values, given by the
//! page in the order of the grid. A sort or another layout lays the same nodes
//! out again (`Order`, `Layout`), without computing anything.

use std::cmp::Ordering;

use crate::groups::{Groups, Tree};
use crate::measures::{SortKey, State, Value};
use crate::{AxisOrder, Engine, Error, Form, Layout, Level, Order};

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
    /// Number of levels it shares with the previous line: the labels that
    /// tabular form does not repeat.
    pub shared: usize,
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

/// Axis of a table.
#[derive(Clone, Copy)]
enum Axis {
    /// Rows: its nodes make the lines.
    Rows,
    /// Columns.
    Columns,
}

/// What orders the children of a node.
#[derive(Clone, Copy)]
struct Siblings<'a> {
    /// Rank of the value `code` of the level `level` (0 for the outermost):
    /// `ranks[level][code]`.
    ranks: &'a [&'a [u32]],
    /// Is each level in descending order? A level left out is ascending.
    descending: &'a [bool],
    /// Key of each node, and is their order descending?
    keys: Option<(&'a [SortKey], bool)>,
}

impl<'a> Siblings<'a> {
    /// The labels alone, in ascending order.
    fn by_labels(ranks: &'a [&'a [u32]]) -> Self {
        Siblings {
            ranks,
            descending: &[],
            keys: None,
        }
    }

    /// The order asked for an axis, with the keys of its nodes.
    fn of_axis(ranks: &'a [&'a [u32]], axis: &'a AxisOrder, keys: Option<&'a [SortKey]>) -> Self {
        let direction = axis.values.as_ref().map(|values| values.descending);
        Siblings {
            ranks,
            descending: &axis.descending,
            keys: keys.zip(direction),
        }
    }

    /// Compares two children of one node: by their keys, then by their labels.
    fn compare(&self, tree: &Tree, a: u32, b: u32) -> Ordering {
        let by_key = self.keys.map_or(Ordering::Equal, |(keys, descending)| {
            let order = keys[a as usize].compare(keys[b as usize]);
            if descending { order.reverse() } else { order }
        });
        by_key.then_with(|| {
            let (a, b) = (&tree.nodes[a as usize], &tree.nodes[b as usize]);
            let level = a.depth as usize - 1;
            let ranks = self.ranks[level];
            let order = ranks[a.code as usize].cmp(&ranks[b.code as usize]);
            if self.descending.get(level) == Some(&true) {
                order.reverse()
            } else {
                order
            }
        })
    }
}

/// Children of each node of a tree, in their order: those of node `n` are
/// `order[starts[n]..starts[n + 1]]`.
struct Children {
    /// Nodes other than the root, grouped by parent, in their order.
    order: Vec<u32>,
    /// Start of the children of each node in `order`, then its length.
    starts: Vec<u32>,
}

impl Children {
    /// Sorts the children of each node.
    fn new(tree: &Tree, siblings: Siblings) -> Self {
        let mut order: Vec<u32> = (1..tree.nodes.len() as u32).collect();
        order.sort_unstable_by(|&a, &b| {
            let (first, second) = (&tree.nodes[a as usize], &tree.nodes[b as usize]);
            first
                .parent
                .cmp(&second.parent)
                .then_with(|| siblings.compare(tree, a, b))
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

    /// Nodes, each one before its children, the root last.
    fn parents_first(&self) -> Vec<u32> {
        let mut lines = Vec::with_capacity(self.order.len() + 1);
        let mut stack: Vec<u32> = self.of(0).iter().rev().copied().collect();
        while let Some(node) = stack.pop() {
            lines.push(node);
            stack.extend(self.of(node).iter().rev());
        }
        lines.push(0);
        lines
    }

    /// Nodes, each one after its children, the root last.
    fn parents_last(&self) -> Vec<u32> {
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
    /// Levels of the rows, then of the columns.
    levels: Vec<Level>,
    /// Number of levels of the rows.
    row_levels: usize,
    /// Number of levels of the columns.
    column_levels: usize,
    /// Node of the rows of each line.
    lines: Vec<u32>,
    /// Node of the columns of each group of columns.
    column_nodes: Vec<u32>,
    /// Layout of the lines and of the columns.
    layout: Layout,
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
    /// Levels of the rows.
    pub row_levels: Vec<Level>,
    /// Levels of the columns.
    pub column_levels: Vec<Level>,
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
        let lines = Children::new(&parts.rows, Siblings::by_labels(row_ranks)).parents_first();
        let column_nodes =
            Children::new(&parts.columns, Siblings::by_labels(column_ranks)).parents_last();
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
            layout: Layout::default(),
            slots,
            states: parts.states,
            finest_groups: parts.finest_groups,
            selected_rows: parts.selected_rows,
        }
    }

    /// Lays the lines and the columns out again, in an order and a layout. The
    /// groups and their values stay as they are: nothing is computed again.
    pub fn lay_out(
        &mut self,
        engine: &Engine,
        order: &Order,
        layout: &Layout,
    ) -> Result<(), Error> {
        let (row_levels, column_levels) = self.levels.split_at(self.row_levels);
        let (_, row_ranks) = engine.levels(row_levels)?;
        let (_, column_ranks) = engine.levels(column_levels)?;
        let row_keys = self.keys(engine, &order.rows, Axis::Rows)?;
        let column_keys = self.keys(engine, &order.columns, Axis::Columns)?;
        let rows = Siblings::of_axis(&row_ranks, &order.rows, row_keys.as_deref());
        let columns = Siblings::of_axis(&column_ranks, &order.columns, column_keys.as_deref());
        let rows = Children::new(&self.rows, rows);
        let mut lines = match layout.form {
            Form::Compact | Form::Outline => rows.parents_first(),
            Form::Tabular => rows.parents_last(),
        };
        // A group keeps its line without its subtotal, except in tabular form,
        // where its label stands on the line of its first member.
        let group_lines = layout.form != Form::Tabular;
        lines.retain(|&node| {
            let depth = self.rows.nodes[node as usize].depth as usize;
            (group_lines && depth > 0) || layout.rows.show(depth, self.row_levels)
        });
        let mut column_nodes = Children::new(&self.columns, columns).parents_last();
        column_nodes.retain(|&node| {
            let depth = self.columns.nodes[node as usize].depth as usize;
            layout.columns.show(depth, self.column_levels)
        });
        self.lines = lines;
        self.column_nodes = column_nodes;
        self.layout = layout.clone();
        Ok(())
    }

    /// Key of each node of an axis: its value at the node of the other axis
    /// that the order names. `None` when no value ranks the axis, or when the
    /// table holds no such node or measure.
    fn keys(
        &self,
        engine: &Engine,
        order: &AxisOrder,
        axis: Axis,
    ) -> Result<Option<Vec<SortKey>>, Error> {
        let Some(values) = &order.values else {
            return Ok(None);
        };
        let Some(state) = self.states.get(values.measure) else {
            return Ok(None);
        };
        let (tree, other, first_level) = match axis {
            Axis::Rows => (&self.rows, &self.columns, self.row_levels),
            Axis::Columns => (&self.columns, &self.rows, 0),
        };
        let Some(target) = self.node_at(engine, other, first_level, &values.path) else {
            return Ok(None);
        };
        (0..tree.nodes.len() as u32)
            .map(|node| {
                let group = match axis {
                    Axis::Rows => self.groups.find(node, target),
                    Axis::Columns => self.groups.find(target, node),
                };
                let value = group.map_or(Value::Empty, |group| state.value(group));
                engine.sort_key(values.measure, value)
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Some)
    }

    /// Node of a tree at a path of values, `None` if the tree holds none. The
    /// levels of the tree start at `first_level`.
    fn node_at(
        &self,
        engine: &Engine,
        tree: &Tree,
        first_level: usize,
        path: &[Option<String>],
    ) -> Option<u32> {
        path.iter()
            .zip(1u32..)
            .try_fold(0, |parent, (value, depth)| {
                let level = first_level + depth as usize - 1;
                let node = tree.nodes.iter().position(|node| {
                    node.depth == depth
                        && node.parent == parent
                        && self.label(engine, level, node.row) == value.as_deref()
                })?;
                Some(node as u32)
            })
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

    /// Value of the node of a level, read by the engine.
    fn label<'a>(&self, engine: &'a Engine, level: usize, row: u32) -> Option<&'a str> {
        engine.level_value(self.levels[level], row)
    }

    /// Line `index`.
    ///
    /// # Panics
    ///
    /// If `index` is not a line of the table.
    pub fn line<'a>(&self, engine: &'a Engine, index: usize) -> Line<'a> {
        let node = self.rows.nodes[self.lines[index] as usize];
        let depth = node.depth as usize;
        let label = match depth {
            0 => Label::Total,
            _ => Label::Value(self.label(engine, depth - 1, node.row)),
        };
        Line {
            depth,
            label,
            shared: self.shared_levels(index),
        }
    }

    /// Number of levels line `index` shares with the previous one: the depth of
    /// the deepest node both come from.
    fn shared_levels(&self, index: usize) -> usize {
        let Some(previous) = index.checked_sub(1) else {
            return 0;
        };
        let nodes = &self.rows.nodes;
        let (mut line, mut other) = (self.lines[index], self.lines[previous]);
        while line != other {
            let (node, other_node) = (nodes[line as usize], nodes[other as usize]);
            if node.depth >= other_node.depth {
                line = node.parent;
            }
            if other_node.depth >= node.depth {
                other = other_node.parent;
            }
        }
        nodes[line as usize].depth as usize
    }

    /// Values of the levels of line `index`, the outermost first: those of its
    /// node and of its ancestors.
    ///
    /// # Panics
    ///
    /// If `index` is not a line of the table.
    pub fn line_path<'a>(&self, engine: &'a Engine, index: usize) -> Vec<Option<&'a str>> {
        let mut path = Vec::new();
        let mut node = self.rows.nodes[self.lines[index] as usize];
        while node.depth > 0 {
            path.push(self.label(engine, node.depth as usize - 1, node.row));
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
    pub fn header<'a>(&self, engine: &'a Engine, index: usize) -> Header<'a> {
        let node_id = self.column_nodes[index / self.slots];
        let mut path = Vec::new();
        let mut node = self.columns.nodes[node_id as usize];
        let mut id = node_id;
        while node.depth > 0 {
            let level = self.row_levels + node.depth as usize - 1;
            path.push((id, self.label(engine, level, node.row)));
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

    /// Value of the cell of line `line` and column `column`: empty on the line
    /// of a group whose subtotal is not shown.
    ///
    /// # Panics
    ///
    /// If the line or the column is not in the table.
    pub fn value(&self, line: usize, column: usize) -> Value {
        let row_node = self.lines[line];
        let depth = self.rows.nodes[row_node as usize].depth as usize;
        if !self.layout.rows.show(depth, self.row_levels) {
            return Value::Empty;
        }
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
