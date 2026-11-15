//! Engine of the Pivoteur, the pivot table under the result grid: groups the
//! rows of a result by the values of its levels, computes its measures with
//! every subtotal and total, then lays out the table, which the page reads
//! window by window.
//!
//! First slice of the lot 1 of its specification: a level is a column of the
//! result, its value as it is or what a precision keeps of its dates (`dates`);
//! the measures are `COUNT(*)`, `COUNT(col)`, `SUM(col)`, `AVG(col)`,
//! `MIN(col)` and `MAX(col)`, in ANSI SQL, and `COUNT`, `SUM` and `AVG` of
//! `DISTINCT col`; `COUNT(DISTINCT col)`, `MIN(col)` and `MAX(col)` read the
//! values of a level, hence the periods of a precision too. `GROUP_CONCAT(col)`,
//! of MariaDB, brought forward from the lot 3, joins the values of a level into
//! one text (`Concat`). A count, a sum or an average can show the share each
//! value makes of its row or column total (`ShowAs`). The lines and the
//! columns follow their labels or the values
//! of a measure (`Order`), the lines take a form, and each axis shows the totals
//! it is given (`Layout`). Filters keep rows, by a list of values or a condition
//! on a column, at a precision of its dates, or on a measure at a level
//! (`filters`). The dialect of the Pivoteur, with its expressions, comes with the
//! next slices.
//!
//! The rows of a result reach the engine as the page receives them: they are
//! appended to its store (`Engine::store_mut`), then taken into account
//! (`Engine::grow`). The forms already derived from the columns (dictionaries,
//! numbers, date levels) then read only the new rows, as a table is to follow
//! the reception (`docs/PERFORMANCES.md`, chapter 17). The engine keeps the
//! finest groups and the states of the last table it updates
//! (`Engine::update`), and its next update of the same table adds only the
//! rows taken in since.
//!
//! The crate does not depend on its target: its tests run natively, and the
//! Worker of the page runs it compiled to WebAssembly (`crates/web-engine`).

mod dates;
mod filters;
mod groups;
mod measures;
mod table;

use std::cell::OnceCell;
use std::cmp::Ordering;
use std::fmt;

use prisme_store::{Approx, Bounds, Dictionary, Encoder, Exact, NumberError, Store, TextColumn};

use crate::dates::DerivedLevel;
pub use crate::dates::Precision;
pub use crate::filters::{Comparison, Filter, Test};
pub use crate::measures::Value;
use crate::measures::{Extreme, Input, Joined, OrderKey, Overflow, Partial, SortKey, State};
pub use crate::table::{Header, Label, Line, Table, TextLengths};

/// Family of the type of a column, as far as the measures and the filters need
/// it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    /// Exact numbers: the `integer` and `decimal` families.
    Exact,
    /// Floats: the `float` family (`DOUBLE`).
    Float,
    /// Dates: the `date` family, which a filter compares in the order of time.
    Date,
    /// Instants: the `datetime` family, which a filter compares in the order of
    /// time.
    Datetime,
    /// Hours: the `time` family, which a filter compares in the order of time.
    Time,
    /// Any other family, which a filter compares by code points.
    Other,
}

impl Family {
    /// Does the column hold numbers, which `SUM` and `AVG` add up?
    fn is_number(self) -> bool {
        matches!(self, Family::Exact | Family::Float)
    }
}

/// Measure of a table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Measure {
    /// `COUNT(*)`: number of rows.
    CountStar,
    /// `COUNT(col)`: number of rows where the column is not NULL.
    Count(usize),
    /// `COUNT(DISTINCT col)`: number of distinct values of a level whose cells
    /// are not NULL, told apart as the levels group them: those of its column,
    /// or its periods.
    CountDistinct(Level),
    /// `SUM(col)`: sum of the values of a column of numbers that are not
    /// NULL; NULL without any.
    Sum(usize),
    /// `SUM(DISTINCT col)`: sum of the distinct numbers of a column, told
    /// apart by value; NULL without any.
    SumDistinct(usize),
    /// `AVG(col)`: average of the values of a column of numbers that are not
    /// NULL; NULL without any.
    Avg(usize),
    /// `AVG(DISTINCT col)`: average of the distinct numbers of a column, told
    /// apart by value; NULL without any.
    AvgDistinct(usize),
    /// `MIN(col)`: first value of a level whose cell is not NULL: numbers by
    /// value, periods in the order of time, the rest by code points; NULL
    /// without any.
    Min(Level),
    /// `MAX(col)`: last value of a level whose cell is not NULL, in the order
    /// of `MIN`; NULL without any.
    Max(Level),
    /// `GROUP_CONCAT(col)`: values of a level whose cells are not NULL,
    /// joined into one text (`Concat`); NULL without any.
    GroupConcat(Box<Concat>),
}

impl Measure {
    /// Level whose values the measure reads, for `COUNT(DISTINCT col)`,
    /// `MIN(col)`, `MAX(col)` and `GROUP_CONCAT(col)`; the others read the cells
    /// of their column.
    fn level(&self) -> Option<Level> {
        match self {
            Measure::CountDistinct(level) | Measure::Min(level) | Measure::Max(level) => {
                Some(*level)
            }
            Measure::GroupConcat(concat) => Some(concat.level),
            _ => None,
        }
    }
}

/// `GROUP_CONCAT` of a level, as MariaDB writes it: its values, joined into
/// one text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Concat {
    /// Level of the values: its column, at a precision of its dates or not.
    pub level: Level,
    /// Is each value taken once (`DISTINCT`), the first in the order of the
    /// values? The values are told apart as the levels group them: by their
    /// text, or by their period.
    pub distinct: bool,
    /// Keys of the order of the values (`ORDER BY`), the first one first. The
    /// values they tie, and all of them without a key, follow the order of the
    /// rows.
    pub order: Vec<ConcatKey>,
    /// Text between two values (`SEPARATOR`).
    pub separator: String,
    /// Values skipped first (`OFFSET`).
    pub offset: u64,
    /// Values kept at most after them (`LIMIT`), `None` for all of them.
    pub limit: Option<u64>,
    /// Longest text, in bytes of UTF-8. Beyond, the text keeps the whole
    /// values that hold in it, followed by the separator, `…` and the number
    /// of values left out in parentheses (`a, b, …(3)`), as
    /// `LISTAGG … ON OVERFLOW TRUNCATE '…' WITH COUNT` of the standard.
    pub max_bytes: usize,
}

/// Key of the order of the values of a `GROUP_CONCAT`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConcatKey {
    /// Column of the result, whose values compare as `MIN` and `MAX` compare
    /// them: numbers by value, any other value by code points.
    pub column: usize,
    /// Is the order descending? NULL comes first in ascending order, as in
    /// MariaDB, and last in descending order.
    pub descending: bool,
}

/// Level of a table: the values of a column, or what a precision keeps of
/// its dates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Level {
    /// Column of the result.
    pub column: usize,
    /// Precision of its dates, `None` for its values as they are.
    pub precision: Option<Precision>,
}

impl From<usize> for Level {
    /// Level of the values of a column, as they are.
    fn from(column: usize) -> Self {
        Level {
            column,
            precision: None,
        }
    }
}

/// Table asked of the engine.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Request {
    /// Levels of the rows, the outermost first.
    pub rows: Vec<Level>,
    /// Levels of the columns, the outermost first.
    pub columns: Vec<Level>,
    /// Measures, in the order of their zone.
    pub measures: Vec<Measure>,
    /// How each measure shows its values, by index; a measure left out shows
    /// them as they are. A filter on a measure always reads its values.
    pub show_as: Vec<ShowAs>,
    /// Filters, which combine by `AND`.
    pub filters: Vec<Filter>,
}

/// How a measure shows its values (F-48): as they are, or as the percentage
/// each one makes of a total. A share needs numbers that add up: it suits
/// counts, sums and averages, and divides the value of a cell by that of its
/// total, rounded away from zero, as `100.0 * value / total` would be. A total
/// of 0 or NULL gives NULL, as MySQL, MariaDB and SQLite divide by zero
/// (`div.by_zero`, `docs/SQL-COMPARISON.md`, section 3.4). The text of a share
/// ends with a narrow no-break space and `%` (`12.5 %`), and a sort compares
/// the shares as they are written.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ShowAs {
    /// The values as they are.
    #[default]
    Value,
    /// Share of the total of its line, the row total: the values of the
    /// columns of a line, the totals of the columns apart, add up to 100 %.
    RowShare {
        /// Digits after the point, at most `MAX_SHARE_DECIMALS`.
        decimals: u8,
    },
    /// Share of the total of its column, the column total: the values of the
    /// lines of a column, the totals of the rows apart, add up to 100 %.
    ColumnShare {
        /// Digits after the point, at most `MAX_SHARE_DECIMALS`.
        decimals: u8,
    },
}

/// Most digits after the point of a share.
pub const MAX_SHARE_DECIMALS: u8 = 6;

/// Order of the lines and of the columns of a table (C-11, C-13). A sort
/// changes its layout alone, its values staying as they are.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Order {
    /// Order of the nodes of the rows, hence of the lines.
    pub rows: AxisOrder,
    /// Order of the nodes of the columns.
    pub columns: AxisOrder,
}

/// Order of the nodes of an axis, among those of one parent: a sort keeps the
/// hierarchy, and a total stays in its place.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AxisOrder {
    /// Is each level in descending order, the outermost first? A level left
    /// out is ascending. The labels order the nodes, or those a value ties.
    pub descending: Vec<bool>,
    /// Values that rank the nodes before their labels.
    pub values: Option<ValueOrder>,
}

/// Layout of a table: the form of its lines and the totals shown on each axis.
/// A change lays the same nodes out again, without computing anything: each
/// total stays computed, and can still rank the nodes (`Order`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Layout {
    /// Form of the lines.
    pub form: Form,
    /// Totals of the rows.
    pub rows: Totals,
    /// Totals of the columns.
    pub columns: Totals,
}

/// Form of the lines of a table. The columns have none: their headers stack up.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Form {
    /// One column of labels, indented (C-07): a group has a line of its own,
    /// above its members, with its subtotal.
    #[default]
    Compact,
    /// One column of labels per level: a group has a line of its own, above its
    /// members, with its subtotal.
    Outline,
    /// One column of labels per level: a group starts on the line of its first
    /// member, and its subtotal has a line of its own, below its members.
    Tabular,
}

/// Totals shown on an axis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Totals {
    /// Is the grand total shown?
    pub grand_total: bool,
    /// Is the subtotal of each level shown, the outermost first? A level left
    /// out shows it, and the last level has none.
    pub subtotals: Vec<bool>,
}

impl Default for Totals {
    /// Every total, as `GROUP BY ROLLUP`.
    fn default() -> Self {
        Totals {
            grand_total: true,
            subtotals: Vec::new(),
        }
    }
}

impl Totals {
    /// Are the values of a node of depth `depth` shown, on an axis of `levels`
    /// levels? The only node of an axis without levels holds all its values: it
    /// stays whatever the setting.
    pub(crate) fn show(&self, depth: usize, levels: usize) -> bool {
        match depth {
            0 => self.grand_total || levels == 0,
            depth if depth < levels => self.subtotals.get(depth - 1) != Some(&false),
            _ => true,
        }
    }
}

/// Values of a node of the other axis, which rank the nodes of an axis: its
/// column of values for the lines, its line for the columns. An empty cell
/// comes with NULL, first in ascending order (D22). A node that the table no
/// longer holds, the filter having left it out, ranks nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValueOrder {
    /// Values of the levels of the node, the outermost first, `None` for NULL.
    /// A shorter path reads a subtotal, an empty one the grand total.
    pub path: Vec<Option<String>>,
    /// Measure, by its index.
    pub measure: usize,
    /// Is the order descending?
    pub descending: bool,
}

/// Error of the engine. A computation that fails gives no table: its values
/// would be wrong.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The result has no such column.
    UnknownColumn(usize),
    /// The result has no such row.
    UnknownRow(u32),
    /// The ranks of the values of a column do not match its dictionary.
    Ranks(usize),
    /// Measure `measure` adds up a column that does not hold numbers.
    NotNumeric {
        /// Measure, by its index.
        measure: usize,
    },
    /// A cell of a column of numbers is not a number, or is too large.
    Number {
        /// Column of the cell.
        column: usize,
        /// The problem of the cell.
        error: NumberError,
    },
    /// A value of measure `measure` goes beyond 128 bits.
    Overflow {
        /// Measure, by its index.
        measure: usize,
    },
    /// Measure `measure` is shown as a share, which its values cannot be:
    /// they do not add up (`MIN`, `MAX`, `GROUP_CONCAT`), or have more digits
    /// after the point than `MAX_SHARE_DECIMALS`.
    Share {
        /// Measure, by its index.
        measure: usize,
    },
    /// Filter `filter` adds up a column that does not hold numbers.
    FilterNotNumeric {
        /// Filter, by its index.
        filter: usize,
    },
    /// A value of the measure of filter `filter` goes beyond 128 bits.
    FilterOverflow {
        /// Filter, by its index.
        filter: usize,
    },
    /// An operand of filter `filter` cannot compare with its values.
    FilterOperand {
        /// Filter, by its index.
        filter: usize,
    },
    /// A cell that a precision or a comparison reads, in a row taken into
    /// account, is outside of the calendar, or of the day for an hour.
    InvalidDate {
        /// Column of the cell.
        column: usize,
        /// Row of the cell.
        row: u32,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::UnknownColumn(column) => write!(formatter, "no column {column}"),
            Error::UnknownRow(row) => write!(formatter, "no row {row}"),
            Error::Ranks(column) => write!(formatter, "ranks of column {column} inconsistent"),
            Error::NotNumeric { measure } => {
                write!(formatter, "measure {measure}: column without numbers")
            }
            Error::Number { column, error } => write!(formatter, "column {column}, {error}"),
            Error::Overflow { measure } => {
                write!(formatter, "measure {measure}: value beyond 128 bits")
            }
            Error::Share { measure } => {
                write!(formatter, "measure {measure}: no share of its values")
            }
            Error::FilterNotNumeric { filter } => {
                write!(formatter, "filter {filter}: column without numbers")
            }
            Error::FilterOverflow { filter } => {
                write!(formatter, "filter {filter}: value beyond 128 bits")
            }
            Error::FilterOperand { filter } => {
                write!(formatter, "filter {filter}: operand not comparable")
            }
            Error::InvalidDate { column, row } => {
                write!(
                    formatter,
                    "column {column}, row {row}: outside of the calendar"
                )
            }
        }
    }
}

impl std::error::Error for Error {}

/// Numbers of a column, read by its family.
enum Numbers {
    /// Exact numbers.
    Exact(Exact),
    /// Floats.
    Float(Approx),
}

impl Numbers {
    /// Reads the numbers of the rows of a column after those already read, up
    /// to `end`, excluded.
    fn extend(&mut self, cells: &TextColumn, end: usize) -> Result<(), NumberError> {
        match self {
            Numbers::Exact(numbers) => numbers.extend(cells, end),
            Numbers::Float(numbers) => numbers.extend(cells, end),
        }
    }
}

/// Shares of the values of measure `measure` (`ShowAs`): each group divided
/// by the group of its total, which `total` gives from the nodes of the rows
/// and of the columns of the group.
fn shares(
    state: &State,
    groups: &groups::Groups,
    measure: usize,
    decimals: u8,
    total: impl Fn(u32, u32) -> (u32, u32),
) -> Result<State, Error> {
    if decimals > MAX_SHARE_DECIMALS {
        return Err(Error::Share { measure });
    }
    // Every row of a group falls into its total: the total holds it.
    let totals: Vec<u32> = groups
        .pairs()
        .iter()
        .map(|&(row, column)| {
            let (row, column) = total(row, column);
            groups.find(row, column).expect("group of a total")
        })
        .collect();
    match state.shares(&totals, u32::from(decimals)) {
        Ok(Some(shares)) => Ok(shares),
        Ok(None) => Err(Error::Share { measure }),
        Err(Overflow) => Err(Error::Overflow { measure }),
    }
}

/// Rank of each code, from the codes in their order.
fn ranks_of(order: &[u32]) -> Vec<u32> {
    let mut ranks = vec![0u32; order.len()];
    for (rank, &code) in order.iter().enumerate() {
        ranks[code as usize] = rank as u32;
    }
    ranks
}

/// Codes of a column in the order of their values, and the rank of each code.
/// The codes a dictionary gains are sorted alone, then merged into the order
/// of the others: an equal value ranks after those already there, as a stable
/// sort of all the codes ranks it. Each new code finds its place by an
/// exponential search from that of the previous one: a merge compares
/// `n log(d / n)` values for `n` new codes among `d`, without reading the
/// values of all the codes.
///
/// Each code keeps, in the same order, the key that reads its value: the
/// place of its text, or the row of its number. A comparison reads the keys
/// one after the other, and the values alone in the column, without going
/// through the first row of each code (`docs/PERFORMANCES.md`, chapter 17).
#[derive(Debug)]
struct Ranking<K> {
    /// Codes, in the order of their values.
    order: Vec<u32>,
    /// Key of each code of `order`, in the same order.
    keys: Vec<K>,
    /// Rank of each code.
    ranks: Vec<u32>,
}

impl<K> Default for Ranking<K> {
    fn default() -> Self {
        Ranking {
            order: Vec::new(),
            keys: Vec::new(),
            ranks: Vec::new(),
        }
    }
}

impl<K> Ranking<K> {
    /// Drops the order and the keys of the codes, which only the values to
    /// come need, once the column has received its last row.
    fn seal(&mut self) {
        self.order = Vec::new();
        self.keys = Vec::new();
    }
}

impl<K: Copy> Ranking<K> {
    /// Ranks the codes after those already ranked, up to `codes`, excluded.
    ///
    /// # Arguments
    ///
    /// - `key`: key of a code;
    /// - `compare`: comparison of the values of two keys.
    fn extend(&mut self, codes: usize, key: impl Fn(u32) -> K, compare: impl Fn(K, K) -> Ordering) {
        let start = self.order.len() as u32;
        let end = codes as u32;
        if end <= start {
            return;
        }
        let mut new: Vec<(K, u32)> = (start..end).map(|code| (key(code), code)).collect();
        new.sort_by(|a, b| compare(a.0, b.0));
        let (old, old_keys) = (
            std::mem::take(&mut self.order),
            std::mem::take(&mut self.keys),
        );
        let mut order = Vec::with_capacity(codes);
        let mut keys = Vec::with_capacity(codes);
        let mut from = 0;
        for (new_key, code) in new {
            // The first code already ranked after the new one, past the
            // equal values: bounded by doubling steps, then found within.
            let rest = &old_keys[from..];
            let before = |&ranked: &K| compare(ranked, new_key).is_le();
            let mut bound = 1;
            while bound < rest.len() && before(&rest[bound - 1]) {
                bound *= 2;
            }
            let place = from + rest[..bound.min(rest.len())].partition_point(before);
            order.extend_from_slice(&old[from..place]);
            keys.extend_from_slice(&old_keys[from..place]);
            order.push(code);
            keys.push(new_key);
            from = place;
        }
        order.extend_from_slice(&old[from..]);
        keys.extend_from_slice(&old_keys[from..]);
        self.ranks = ranks_of(&order);
        self.order = order;
        self.keys = keys;
    }
}

/// Ranks the values of a column after those already ranked, in the binary
/// order of their texts, NULL first.
fn rank_texts(ranking: &mut Ranking<Bounds>, dictionary: &Dictionary, cells: &TextColumn) {
    ranking.extend(
        dictionary.len(),
        |code| cells.bounds(dictionary.first_rows[code as usize] as usize),
        |a, b| cells.text_of(a).cmp(&cells.text_of(b)),
    );
}

/// Ranks the values of a column of numbers after those already ranked, as
/// `MIN` and `MAX` compare them: by value, a `NaN` after the others. The key
/// of a value is the row of its number.
fn rank_numbers(ranking: &mut Ranking<u32>, dictionary: &Dictionary, numbers: &Numbers) {
    let key = |code: u32| dictionary.first_rows[code as usize];
    match numbers {
        Numbers::Exact(numbers) => ranking.extend(dictionary.len(), key, |a, b| {
            numbers.values[a as usize].cmp(&numbers.values[b as usize])
        }),
        Numbers::Float(numbers) => ranking.extend(dictionary.len(), key, |a, b| {
            numbers.values[a as usize].total_cmp(&numbers.values[b as usize])
        }),
    }
}

/// State of the last table an update computed, kept to add the rows taken in
/// since to its finest groups and to the states of its measures.
struct Accumulation {
    /// Request of the table.
    request: Request,
    /// Version of the rows kept by the filter of the grid that the table read.
    kept_version: u64,
    /// Rows read: the first ones of the result, or of the rows kept by the
    /// filter of the grid.
    read: usize,
    /// Rows the filters on cells left among them.
    selected: usize,
    /// Finest groups of these rows.
    finest: groups::Finest,
    /// States of the finest groups of each measure, all of them adding up;
    /// none for a table that reads all its rows each time.
    partials: Vec<Partial>,
}

/// Engine of one result: its rows, and the forms derived from its columns,
/// built once, the first time a table needs them, then extended to the rows
/// that arrive.
pub struct Engine {
    /// Rows of the result.
    store: Store,
    /// Rows taken into account: the first ones of the store, until `grow`
    /// takes in those appended since.
    rows: usize,
    /// Family of each column.
    families: Vec<Family>,
    /// Dictionary of each column, with the index of its texts.
    dictionaries: Vec<OnceCell<Encoder>>,
    /// Numbers of each column of numbers, or the cell it cannot read.
    numbers: Vec<OnceCell<Result<Numbers, NumberError>>>,
    /// Values of each column in the binary order of their texts, NULL first.
    text_ranks: Vec<OnceCell<Ranking<Bounds>>>,
    /// Rank of each value of each column, by code, in the order of the grid,
    /// when the page gave them.
    page_ranks: Vec<Option<Vec<u32>>>,
    /// Values of each column of numbers by value, as `MIN` and `MAX` compare
    /// them.
    value_ranks: Vec<OnceCell<Ranking<u32>>>,
    /// Levels of each column at each precision.
    derived: Vec<[OnceCell<DerivedLevel>; Precision::COUNT]>,
    /// Longest text of the cells of each column, in characters.
    longest: Vec<OnceCell<usize>>,
    /// Rows kept by the filter of the grid, in their order; `None` for all.
    kept: Option<Vec<u32>>,
    /// Version of the rows kept, which changes when the filter of the grid
    /// changes, and stays when it extends to the rows taken in.
    kept_version: u64,
    /// State of the last table `update` computed.
    accumulation: Option<Accumulation>,
    /// Has the result received its last row (`seal`)?
    sealed: bool,
}

impl Engine {
    /// Creates the engine of a result, with the family of each column.
    ///
    /// # Panics
    ///
    /// If the store does not have one column per family.
    pub fn new(store: Store, families: Vec<Family>) -> Self {
        assert_eq!(
            store.column_count(),
            families.len(),
            "one family per column"
        );
        let columns = families.len();
        Engine {
            rows: store.row_count(),
            store,
            families,
            dictionaries: (0..columns).map(|_| OnceCell::new()).collect(),
            numbers: (0..columns).map(|_| OnceCell::new()).collect(),
            text_ranks: (0..columns).map(|_| OnceCell::new()).collect(),
            page_ranks: vec![None; columns],
            value_ranks: (0..columns).map(|_| OnceCell::new()).collect(),
            derived: (0..columns)
                .map(|_| std::array::from_fn(|_| OnceCell::new()))
                .collect(),
            longest: (0..columns).map(|_| OnceCell::new()).collect(),
            kept: None,
            kept_version: 0,
            accumulation: None,
            sealed: false,
        }
    }

    /// Rows of the result.
    pub fn store(&self) -> &Store {
        &self.store
    }

    /// Rows of the result, to append the rows received since: they count once
    /// `grow` has taken them in.
    pub fn store_mut(&mut self) -> &mut Store {
        &mut self.store
    }

    /// Number of rows taken into account.
    pub fn rows(&self) -> usize {
        self.rows
    }

    /// Takes in the rows appended to the store since the engine was created or
    /// last grew: those that every column holds. The forms already built from
    /// the columns read only these rows. A column whose dictionary gains values
    /// ranks them among the others; the ranks the page gave it are dropped, as
    /// they do not rank the new values.
    ///
    /// # Returns
    ///
    /// The columns whose ranks given by the page were dropped: the page gives
    /// them again (`set_ranks`), or they follow the binary order of the texts.
    ///
    /// # Panics
    ///
    /// If the engine is sealed and rows were appended since.
    pub fn grow(&mut self) -> Vec<usize> {
        let (start, end) = (self.rows, self.store.row_count());
        let mut dropped = Vec::new();
        if end <= start {
            return dropped;
        }
        assert!(!self.sealed, "rows appended after the last one");
        self.rows = end;
        for column in 0..self.families.len() {
            let cells = self.store.column(column).expect("one column per family");
            let failed = match self.numbers[column].get_mut() {
                Some(Ok(numbers)) => numbers.extend(cells, end).err(),
                _ => None,
            };
            if let Some(error) = failed {
                self.numbers[column] = OnceCell::from(Err(error));
                self.value_ranks[column].take();
            }
            if let Some(encoder) = self.dictionaries[column].get_mut() {
                let values = encoder.dictionary().len();
                encoder.extend(cells, end);
                let dictionary = encoder.dictionary();
                if dictionary.len() > values && self.page_ranks[column].take().is_some() {
                    dropped.push(column);
                }
                if let Some(ranking) = self.text_ranks[column].get_mut() {
                    rank_texts(ranking, dictionary, cells);
                }
                let numbers = self.numbers[column]
                    .get()
                    .and_then(|numbers| numbers.as_ref().ok());
                if let (Some(ranking), Some(numbers)) =
                    (self.value_ranks[column].get_mut(), numbers)
                {
                    rank_numbers(ranking, dictionary, numbers);
                }
                for derived in self.derived[column].iter_mut() {
                    if let Some(derived) = derived.get_mut() {
                        derived.extend(cells, dictionary);
                    }
                }
            }
            if let Some(longest) = self.longest[column].get_mut() {
                let lengths = cells.cells_in(start..end).flatten();
                *longest = lengths
                    .map(|text| text.chars().count())
                    .fold(*longest, usize::max);
            }
        }
        dropped
    }

    /// Takes in the last rows of the result (`grow`), then drops what only
    /// the rows to come need: the indexes of the dictionaries, the order of
    /// the ranked values, the state of the last table, and the room the
    /// arrays reserved. The forms built later from the columns hold no more
    /// than these. A table is then computed from all the rows.
    ///
    /// # Returns
    ///
    /// The columns whose ranks given by the page were dropped (`grow`).
    pub fn seal(&mut self) -> Vec<usize> {
        let dropped = self.grow();
        self.sealed = true;
        self.accumulation = None;
        self.store.shrink_to_fit();
        for column in 0..self.families.len() {
            if let Some(encoder) = self.dictionaries[column].get_mut() {
                encoder.seal();
            }
            if let Some(Ok(numbers)) = self.numbers[column].get_mut() {
                match numbers {
                    Numbers::Exact(numbers) => numbers.values.shrink_to_fit(),
                    Numbers::Float(numbers) => numbers.values.shrink_to_fit(),
                }
            }
            if let Some(ranking) = self.text_ranks[column].get_mut() {
                ranking.seal();
            }
            if let Some(ranking) = self.value_ranks[column].get_mut() {
                ranking.seal();
            }
            for derived in self.derived[column].iter_mut() {
                if let Some(derived) = derived.get_mut() {
                    derived.seal();
                }
            }
        }
        dropped
    }

    /// Dictionary of column `column`.
    fn dictionary(&self, column: usize) -> Result<&Dictionary, Error> {
        let cells = self
            .store
            .column(column)
            .ok_or(Error::UnknownColumn(column))?;
        let encoder = self.dictionaries[column].get_or_init(|| {
            let mut encoder = Encoder::default();
            encoder.extend(cells, self.rows);
            if self.sealed {
                encoder.seal();
            }
            encoder
        });
        Ok(encoder.dictionary())
    }

    /// Distinct values of column `column`, by code: their text, `None` for
    /// NULL. The page ranks them in the order of the grid (`set_ranks`).
    pub fn distinct(&self, column: usize) -> Result<Vec<Option<&str>>, Error> {
        let dictionary = self.dictionary(column)?;
        let cells = &self.store.column(column).expect("column of the dictionary");
        Ok((0..dictionary.len() as u32)
            .map(|code| dictionary.value(cells, code))
            .collect())
    }

    /// Distinct values of a level, which the list of a filter offers: those of
    /// `distinct`, or the labels of its periods in the order of time, NULL first,
    /// without the cells outside of the calendar.
    pub fn distinct_at(&self, level: Level) -> Result<Vec<Option<&str>>, Error> {
        match level.precision {
            Some(precision) => Ok(self.derived(level.column, precision)?.listed()),
            None => self.distinct(level.column),
        }
    }

    /// Sets the rank of each distinct value of column `column`, by code, in
    /// the order of `distinct`.
    pub fn set_ranks(&mut self, column: usize, ranks: Vec<u32>) -> Result<(), Error> {
        let values = self.dictionary(column)?.len();
        let mut seen = vec![false; values];
        for &rank in &ranks {
            match seen.get_mut(rank as usize) {
                Some(slot) if !*slot => *slot = true,
                _ => return Err(Error::Ranks(column)),
            }
        }
        if ranks.len() != values {
            return Err(Error::Ranks(column));
        }
        self.page_ranks[column] = Some(ranks);
        Ok(())
    }

    /// Ranks of the values of column `column`: those given by the page, or the
    /// binary order of the texts, NULL first.
    fn ranks(&self, column: usize) -> Result<&[u32], Error> {
        match &self.page_ranks[column] {
            Some(ranks) => Ok(ranks),
            None => self.text_ranks(column),
        }
    }

    /// Ranks of the values of column `column` in the binary order of their
    /// texts, NULL first.
    fn text_ranks(&self, column: usize) -> Result<&[u32], Error> {
        let dictionary = self.dictionary(column)?;
        let cells = self.store.column(column).expect("column of the dictionary");
        let ranking = self.text_ranks[column].get_or_init(|| {
            let mut ranking = Ranking::default();
            rank_texts(&mut ranking, dictionary, cells);
            if self.sealed {
                ranking.seal();
            }
            ranking
        });
        Ok(&ranking.ranks)
    }

    /// Ranks of the values of column `column`, as `MIN` and `MAX` compare
    /// them: numbers by value, a `NaN` after the others, and any other value
    /// by code points, the order of the bytes of UTF-8, NULL first. The order
    /// of the page only ranks the groups.
    fn value_ranks(&self, measure: usize, column: usize) -> Result<&[u32], Error> {
        if !self.families[column].is_number() {
            return self.text_ranks(column);
        }
        let dictionary = self.dictionary(column)?;
        let numbers = self.numbers(measure, column)?;
        let ranking = self.value_ranks[column].get_or_init(|| {
            let mut ranking = Ranking::default();
            rank_numbers(&mut ranking, dictionary, numbers);
            if self.sealed {
                ranking.seal();
            }
            ranking
        });
        Ok(&ranking.ranks)
    }

    /// Level of column `column` at a precision.
    fn derived(&self, column: usize, precision: Precision) -> Result<&DerivedLevel, Error> {
        let dictionary = self.dictionary(column)?;
        let cells = self.store.column(column).expect("column of the dictionary");
        Ok(self.derived[column][precision as usize].get_or_init(|| {
            let mut level = DerivedLevel::of(cells, dictionary, precision);
            if self.sealed {
                level.seal();
            }
            level
        }))
    }

    /// Code of each row of a level: one per value of its column, or one per
    /// period of its dates.
    fn level_dictionary(&self, level: Level) -> Result<&Dictionary, Error> {
        match level.precision {
            Some(precision) => Ok(&self.derived(level.column, precision)?.dictionary),
            None => self.dictionary(level.column),
        }
    }

    /// Rank of each code of a level as `MIN` and `MAX` of measure `measure`
    /// compare them: its values (`value_ranks`), or its periods in the order of
    /// time.
    fn extreme_ranks(&self, measure: usize, level: Level) -> Result<&[u32], Error> {
        match level.precision {
            Some(precision) => Ok(&self.derived(level.column, precision)?.ranks),
            None => self.value_ranks(measure, level.column),
        }
    }

    /// Stops on the first selected row whose cell a precision of these levels
    /// cannot read.
    fn check_dates(&self, levels: &[Level], selection: &[u32]) -> Result<(), Error> {
        for &Level { column, precision } in levels {
            if let Some(precision) = precision
                && let Some(row) = self.derived(column, precision)?.first_outside(selection)
            {
                return Err(Error::InvalidDate { column, row });
            }
        }
        Ok(())
    }

    /// Code of each row and rank of each code of some levels.
    fn levels(&self, levels: &[Level]) -> Result<(Vec<&Dictionary>, Vec<&[u32]>), Error> {
        let mut dictionaries = Vec::with_capacity(levels.len());
        let mut ranks = Vec::with_capacity(levels.len());
        for &Level { column, precision } in levels {
            match precision {
                Some(precision) => {
                    let derived = self.derived(column, precision)?;
                    dictionaries.push(&derived.dictionary);
                    ranks.push(derived.ranks.as_slice());
                }
                None => {
                    dictionaries.push(self.dictionary(column)?);
                    ranks.push(self.ranks(column)?);
                }
            }
        }
        Ok((dictionaries, ranks))
    }

    /// Value of a level at the first row of one of its nodes: the cell of its
    /// column, or the label of its period, `None` for NULL.
    pub(crate) fn level_value(&self, level: Level, row: u32) -> Option<&str> {
        match level.precision {
            Some(precision) => self
                .derived(level.column, precision)
                .ok()
                .and_then(|derived| derived.label(row)),
            None => self
                .store
                .column(level.column)
                .and_then(|cells| cells.get(row as usize)),
        }
    }

    /// Longest text of the values of a level, in characters: of the cells of
    /// its column, or of the labels of its periods. No `MIN` or `MAX` of the
    /// level is longer; `usize::MAX` for a level that cannot be read.
    pub(crate) fn longest_text(&self, level: Level) -> usize {
        match level.precision {
            Some(precision) => self
                .derived(level.column, precision)
                .map_or(usize::MAX, |derived| derived.longest),
            None => *self.longest[level.column].get_or_init(|| {
                self.store.column(level.column).map_or(usize::MAX, |cells| {
                    cells
                        .cells_in(0..self.rows)
                        .flatten()
                        .map(|text| text.chars().count())
                        .max()
                        .unwrap_or(0)
                })
            }),
        }
    }

    /// Keeps only some rows, those the filter of the grid keeps; `None` keeps
    /// them all.
    pub fn set_kept(&mut self, rows: Option<Vec<u32>>) -> Result<(), Error> {
        if let Some(&row) = rows
            .iter()
            .flatten()
            .find(|&&row| row as usize >= self.rows)
        {
            return Err(Error::UnknownRow(row));
        }
        self.kept = rows;
        self.kept_version += 1;
        Ok(())
    }

    /// Adds rows to those the filter of the grid keeps, after them: those it
    /// keeps among the rows taken in since. Without a filter, every row is
    /// kept already.
    pub fn extend_kept(&mut self, rows: &[u32]) -> Result<(), Error> {
        if let Some(&row) = rows.iter().find(|&&row| row as usize >= self.rows) {
            return Err(Error::UnknownRow(row));
        }
        if let Some(kept) = &mut self.kept {
            kept.extend_from_slice(rows);
        }
        Ok(())
    }

    /// Numbers of column `column`, read by its family.
    fn numbers(&self, measure: usize, column: usize) -> Result<&Numbers, Error> {
        let cells = self
            .store
            .column(column)
            .ok_or(Error::UnknownColumn(column))?;
        let family = self.families[column];
        if !family.is_number() {
            return Err(Error::NotNumeric { measure });
        }
        self.numbers[column]
            .get_or_init(|| {
                let mut numbers = match family {
                    Family::Exact => Numbers::Exact(Exact::default()),
                    _ => Numbers::Float(Approx::default()),
                };
                numbers.extend(cells, self.rows).map(|()| numbers)
            })
            .as_ref()
            .map_err(|error| Error::Number {
                column,
                error: error.clone(),
            })
    }

    /// Validity of the cells of column `column`.
    fn validity(&self, column: usize) -> Result<&[bool], Error> {
        self.store
            .column(column)
            .map(|cells| cells.validity())
            .ok_or(Error::UnknownColumn(column))
    }

    /// Numbers of column `column`, as `SUM` and `AVG` add them up.
    fn addends(&self, index: usize, column: usize) -> Result<Input<'_>, Error> {
        Ok(match self.numbers(index, column)? {
            Numbers::Exact(numbers) => Input::Exact(numbers, self.validity(column)?),
            Numbers::Float(numbers) => Input::Float(numbers, self.validity(column)?),
        })
    }

    /// Numbers of column `column`, each added once per group.
    fn distinct_addends(&self, index: usize, column: usize) -> Result<Input<'_>, Error> {
        let numbers = self.numbers(index, column)?;
        let valid = self.validity(column)?;
        let dictionary = self.dictionary(column)?;
        Ok(match numbers {
            Numbers::Exact(numbers) => Input::DistinctExact(numbers, valid, dictionary),
            Numbers::Float(numbers) => Input::DistinctFloat(numbers, valid, dictionary),
        })
    }

    /// Values of a level, as `MIN` or, with `largest`, `MAX` compares them.
    fn extreme(&self, index: usize, level: Level, largest: bool) -> Result<Input<'_>, Error> {
        Ok(Input::Extreme(Extreme {
            level,
            dictionary: self.level_dictionary(level)?,
            ranks: self.extreme_ranks(index, level)?,
            valid: self.validity(level.column)?,
            largest,
        }))
    }

    /// Values a `GROUP_CONCAT` joins, with the keys of their order.
    fn joined<'a>(&'a self, index: usize, concat: &'a Concat) -> Result<Input<'a>, Error> {
        let keys = concat
            .order
            .iter()
            .map(|key| {
                Ok(OrderKey {
                    codes: &self.dictionary(key.column)?.codes,
                    ranks: self.value_ranks(index, key.column)?,
                    valid: self.validity(key.column)?,
                    descending: key.descending,
                })
            })
            .collect::<Result<Vec<_>, Error>>()?;
        Ok(Input::Joined(Joined {
            engine: self,
            concat,
            codes: &self.level_dictionary(concat.level)?.codes,
            valid: self.validity(concat.level.column)?,
            keys,
        }))
    }

    /// Values measure `measure` reads.
    fn input<'a>(&'a self, index: usize, measure: &'a Measure) -> Result<Input<'a>, Error> {
        Ok(match *measure {
            Measure::CountStar => Input::Rows,
            Measure::Count(column) => Input::Cells(self.validity(column)?),
            Measure::CountDistinct(level) => Input::Distinct(
                &self.level_dictionary(level)?.codes,
                self.validity(level.column)?,
            ),
            Measure::Sum(column) => self.addends(index, column)?,
            Measure::SumDistinct(column) => self.distinct_addends(index, column)?,
            Measure::Avg(column) => Input::Average(Box::new(self.addends(index, column)?)),
            Measure::AvgDistinct(column) => {
                Input::Average(Box::new(self.distinct_addends(index, column)?))
            }
            Measure::Min(level) => self.extreme(index, level, false)?,
            Measure::Max(level) => self.extreme(index, level, true)?,
            Measure::GroupConcat(ref concat) => self.joined(index, concat)?,
        })
    }

    /// What a sort compares of a value of measure `measure`: a value of a
    /// level by its rank, as `MIN` and `MAX` compare it, a text by code points.
    pub(crate) fn sort_key<'v>(
        &self,
        measure: usize,
        value: Value<'v>,
    ) -> Result<SortKey<'v>, Error> {
        Ok(match value {
            Value::Empty | Value::Null => SortKey::Missing,
            Value::Count(count) => SortKey::Exact(i128::from(count)),
            Value::Exact(number, _) | Value::Share(number, _) => SortKey::Exact(number),
            Value::Float(number) => SortKey::Float(number),
            Value::Cell(level, row) => {
                let code = self.level_dictionary(level)?.codes[row as usize];
                let ranks = self.extreme_ranks(measure, level)?;
                SortKey::Exact(i128::from(ranks[code as usize]))
            }
            Value::Text(text) => SortKey::Text(text),
        })
    }

    /// Computes a table over the rows kept and its filters leave, in the order
    /// of the labels. As in SQL, the rows that a filter on cells leaves out stop
    /// nothing, and those of a group a filter on a measure drops have been read:
    /// a date outside of the calendar in one of them stops a precision of a
    /// level or of a measure.
    pub fn compute(&self, request: &Request) -> Result<Table, Error> {
        self.accumulate(request, None, false)
            .map(|(table, _)| table)
    }

    /// Computes a table as `compute` does, and keeps its finest groups and the
    /// states of its measures: the next update of the same request adds only
    /// the rows taken in since, and those the filter of the grid keeps among
    /// them (`extend_kept`). The table is the one `compute` gives.
    ///
    /// A new request, a new filter of the grid (`set_kept`), or exact numbers
    /// of a finer scale among the new rows start from all the rows again; so
    /// does each update of a table whose measures do not add up (distinct
    /// values, `GROUP_CONCAT`) or that filters on a measure: they read all its
    /// rows. A refused update keeps no state, nor does a sealed engine
    /// (`seal`): no row is to come.
    pub fn update(&mut self, request: &Request) -> Result<Table, Error> {
        let previous = self.accumulation.take();
        let (table, accumulation) = self.accumulate(request, previous, !self.sealed)?;
        self.accumulation = accumulation;
        Ok(table)
    }

    /// Adds the rows to read to the state of a table, the previous one when
    /// it holds, and computes the table.
    ///
    /// # Returns
    ///
    /// The table, and, if `keep` and its measures all add up, its state.
    fn accumulate(
        &self,
        request: &Request,
        previous: Option<Accumulation>,
        keep: bool,
    ) -> Result<(Table, Option<Accumulation>), Error> {
        let (row_levels, row_ranks) = self.levels(&request.rows)?;
        let (column_levels, column_ranks) = self.levels(&request.columns)?;
        let inputs = request
            .measures
            .iter()
            .enumerate()
            .map(|(index, measure)| self.input(index, measure))
            .collect::<Result<Vec<_>, _>>()?;
        let filters_measures = request
            .filters
            .iter()
            .any(|filter| matches!(filter, Filter::Measure { .. }));
        let partials: Option<Vec<Partial>> = if filters_measures {
            None
        } else {
            inputs.iter().map(Partial::of).collect()
        };
        let levels: Vec<&Dictionary> = row_levels.iter().chain(&column_levels).copied().collect();
        let mut accumulation = previous
            .filter(|previous| {
                partials.as_ref().is_some_and(|partials| {
                    previous.request == *request
                        && previous.kept_version == self.kept_version
                        && previous
                            .partials
                            .iter()
                            .map(Partial::scale)
                            .eq(partials.iter().map(Partial::scale))
                })
            })
            .unwrap_or_else(|| Accumulation {
                request: request.clone(),
                kept_version: self.kept_version,
                read: 0,
                selected: 0,
                finest: groups::Finest::new(levels.len()),
                partials: partials.clone().unwrap_or_default(),
            });

        let all: Vec<u32>;
        let (to_read, read) = match &self.kept {
            Some(rows) => (&rows[accumulation.read..], rows.len()),
            None => {
                all = (accumulation.read as u32..self.rows as u32).collect();
                (all.as_slice(), self.rows)
            }
        };
        let selection = self.keep_by_cells(&request.filters, to_read)?;
        self.check_dates(&request.rows, &selection)?;
        self.check_dates(&request.columns, &selection)?;
        let measure_levels: Vec<Level> =
            request.measures.iter().filter_map(|m| m.level()).collect();
        self.check_dates(&measure_levels, &selection)?;
        let selection = self.keep_by_measures(&request.filters, selection)?;
        let selection = selection.as_ref();

        let finest = accumulation.finest.add(&levels, selection);
        let finest_groups = accumulation.finest.len();
        for (measure, (partial, input)) in accumulation.partials.iter_mut().zip(&inputs).enumerate()
        {
            partial
                .add(input, selection, &finest, finest_groups)
                .map_err(|_| Error::Overflow { measure })?;
        }
        accumulation.read = read;
        accumulation.selected += selection.len();
        let keep = keep && partials.is_some();
        let partials = if keep {
            accumulation.partials.clone()
        } else {
            std::mem::take(&mut accumulation.partials)
        };

        let first_rows = &accumulation.finest.first_rows;
        let (rows, row_leaves) = groups::tree(&row_levels, first_rows);
        let (columns, column_leaves) = groups::tree(&column_levels, first_rows);
        let groups = groups::Groups::new(&rows, &columns, &row_leaves, &column_leaves);
        let mut partials = partials.into_iter();
        let states = inputs
            .iter()
            .enumerate()
            .map(|(measure, input)| {
                let state = match partials.next() {
                    Some(partial) => partial.state(input, groups.len(), &groups.merges),
                    None => measures::accumulate(
                        input,
                        selection,
                        &finest,
                        groups.len(),
                        &groups.merges,
                    ),
                }
                .map_err(|_| Error::Overflow { measure })?;
                match request.show_as.get(measure).copied().unwrap_or_default() {
                    ShowAs::Value => Ok(state),
                    ShowAs::RowShare { decimals } => {
                        shares(&state, &groups, measure, decimals, |row, _| (row, 0))
                    }
                    ShowAs::ColumnShare { decimals } => {
                        shares(&state, &groups, measure, decimals, |_, column| (0, column))
                    }
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        let parts = table::Parts {
            groups,
            rows,
            columns,
            row_levels: request.rows.clone(),
            column_levels: request.columns.clone(),
            states,
            finest_groups,
            selected_rows: accumulation.selected,
        };
        let table = Table::new(parts, &row_ranks, &column_ranks);
        Ok((table, keep.then_some(accumulation)))
    }
}

#[cfg(test)]
mod streaming_tests;
#[cfg(test)]
mod tests;
