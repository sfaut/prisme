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
//! one text (`Concat`). The lines and the
//! columns follow their labels or the values
//! of a measure (`Order`), the lines take a form, and each axis shows the totals
//! it is given (`Layout`). Filters keep rows, by a list of values or a condition
//! on a column, at a precision of its dates, or on a measure at a level
//! (`filters`). The dialect of the Pivoteur, with its expressions, comes with the
//! next slices.
//!
//! The crate does not depend on its target: its tests run natively, and the
//! Worker of the page runs it compiled to WebAssembly (`crates/web-engine`).

mod dates;
mod filters;
mod groups;
mod measures;
mod table;

use std::cell::OnceCell;
use std::fmt;

use prisme_store::{Approx, Dictionary, Exact, NumberError, Store};

use crate::dates::DerivedLevel;
pub use crate::dates::Precision;
pub use crate::filters::{Comparison, Filter, Test};
pub use crate::measures::Value;
use crate::measures::{Extreme, Input, Joined, OrderKey, SortKey};
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
    /// Filters, which combine by `AND`.
    pub filters: Vec<Filter>,
}

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

/// Rank of each code, from the codes in their order.
fn ranks_of(order: &[u32]) -> Vec<u32> {
    let mut ranks = vec![0u32; order.len()];
    for (rank, &code) in order.iter().enumerate() {
        ranks[code as usize] = rank as u32;
    }
    ranks
}

/// Engine of one result: its rows, and the forms derived from its columns,
/// built once, the first time a table needs them.
pub struct Engine {
    /// Rows of the result.
    store: Store,
    /// Family of each column.
    families: Vec<Family>,
    /// Dictionary of each column.
    dictionaries: Vec<OnceCell<Dictionary>>,
    /// Numbers of each column of numbers, or the cell it cannot read.
    numbers: Vec<OnceCell<Result<Numbers, NumberError>>>,
    /// Rank of each value of each column, by code: given by the page, in the
    /// order of the grid, or the binary order of the texts, NULL first.
    ranks: Vec<OnceCell<Vec<u32>>>,
    /// Rank of each value of each column, by code, as `MIN` and `MAX` compare
    /// them.
    value_ranks: Vec<OnceCell<Vec<u32>>>,
    /// Levels of each column at each precision.
    derived: Vec<[OnceCell<DerivedLevel>; Precision::COUNT]>,
    /// Longest text of the cells of each column, in characters.
    longest: Vec<OnceCell<usize>>,
    /// Rows kept by the filter of the grid, in their order; `None` for all.
    kept: Option<Vec<u32>>,
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
            store,
            families,
            dictionaries: (0..columns).map(|_| OnceCell::new()).collect(),
            numbers: (0..columns).map(|_| OnceCell::new()).collect(),
            ranks: (0..columns).map(|_| OnceCell::new()).collect(),
            value_ranks: (0..columns).map(|_| OnceCell::new()).collect(),
            derived: (0..columns)
                .map(|_| std::array::from_fn(|_| OnceCell::new()))
                .collect(),
            longest: (0..columns).map(|_| OnceCell::new()).collect(),
            kept: None,
        }
    }

    /// Rows of the result.
    pub fn store(&self) -> &Store {
        &self.store
    }

    /// Dictionary of column `column`.
    fn dictionary(&self, column: usize) -> Result<&Dictionary, Error> {
        let cells = self
            .store
            .column(column)
            .ok_or(Error::UnknownColumn(column))?;
        Ok(self.dictionaries[column].get_or_init(|| Dictionary::of(cells)))
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
        self.ranks[column] = OnceCell::from(ranks);
        Ok(())
    }

    /// Ranks of the values of column `column`: those given by the page, or the
    /// binary order of the texts, NULL first.
    fn ranks(&self, column: usize) -> Result<&[u32], Error> {
        let dictionary = self.dictionary(column)?;
        let cells = self.store.column(column).expect("column of the dictionary");
        Ok(self.ranks[column].get_or_init(|| {
            let mut order: Vec<u32> = (0..dictionary.len() as u32).collect();
            order.sort_unstable_by_key(|&code| dictionary.value(cells, code));
            ranks_of(&order)
        }))
    }

    /// Ranks of the values of column `column`, as `MIN` and `MAX` compare
    /// them: numbers by value, a `NaN` after the others, and any other value
    /// by code points, the order of the bytes of UTF-8. The order of the page
    /// only ranks the groups.
    fn value_ranks(&self, measure: usize, column: usize) -> Result<&[u32], Error> {
        let dictionary = self.dictionary(column)?;
        let cells = self.store.column(column).expect("column of the dictionary");
        let numbers = if self.families[column].is_number() {
            Some(self.numbers(measure, column)?)
        } else {
            None
        };
        Ok(self.value_ranks[column].get_or_init(|| {
            let row = |code: u32| dictionary.first_rows[code as usize] as usize;
            let mut order: Vec<u32> = (0..dictionary.len() as u32).collect();
            match numbers {
                Some(Numbers::Exact(numbers)) => {
                    order.sort_by_key(|&code| numbers.values[row(code)]);
                }
                Some(Numbers::Float(numbers)) => {
                    let values = &numbers.values;
                    order.sort_by(|&a, &b| values[row(a)].total_cmp(&values[row(b)]));
                }
                None => order.sort_unstable_by_key(|&code| dictionary.value(cells, code)),
            }
            ranks_of(&order)
        }))
    }

    /// Level of column `column` at a precision.
    fn derived(&self, column: usize, precision: Precision) -> Result<&DerivedLevel, Error> {
        let dictionary = self.dictionary(column)?;
        let cells = self.store.column(column).expect("column of the dictionary");
        Ok(self.derived[column][precision as usize]
            .get_or_init(|| DerivedLevel::of(cells, dictionary, precision)))
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
                        .cells()
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
        let count = self.store.row_count();
        if let Some(&row) = rows.iter().flatten().find(|&&row| row as usize >= count) {
            return Err(Error::UnknownRow(row));
        }
        self.kept = rows;
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
            .get_or_init(|| match family {
                Family::Exact => Exact::of(cells).map(Numbers::Exact),
                _ => Approx::of(cells).map(Numbers::Float),
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
            Value::Exact(number, _) => SortKey::Exact(number),
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
        let all: Vec<u32>;
        let kept: &[u32] = match &self.kept {
            Some(rows) => rows,
            None => {
                all = (0..self.store.row_count() as u32).collect();
                &all
            }
        };
        let (row_levels, row_ranks) = self.levels(&request.rows)?;
        let (column_levels, column_ranks) = self.levels(&request.columns)?;
        let inputs = request
            .measures
            .iter()
            .enumerate()
            .map(|(index, measure)| self.input(index, measure))
            .collect::<Result<Vec<_>, _>>()?;
        let selection = self.keep_by_cells(&request.filters, kept)?;
        self.check_dates(&request.rows, &selection)?;
        self.check_dates(&request.columns, &selection)?;
        let measure_levels: Vec<Level> =
            request.measures.iter().filter_map(|m| m.level()).collect();
        self.check_dates(&measure_levels, &selection)?;
        let selection = self.keep_by_measures(&request.filters, selection)?;
        let selection = selection.as_ref();

        let levels: Vec<&Dictionary> = row_levels.iter().chain(&column_levels).copied().collect();
        let (finest, first_rows) = groups::finest(&levels, selection);
        let (rows, row_leaves) = groups::tree(&row_levels, &first_rows);
        let (columns, column_leaves) = groups::tree(&column_levels, &first_rows);
        let groups = groups::Groups::new(&rows, &columns, &row_leaves, &column_leaves);
        let states = inputs
            .iter()
            .enumerate()
            .map(|(measure, input)| {
                measures::accumulate(input, selection, &finest, groups.len(), &groups.merges)
                    .map_err(|_| Error::Overflow { measure })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let parts = table::Parts {
            groups,
            rows,
            columns,
            row_levels: request.rows.clone(),
            column_levels: request.columns.clone(),
            states,
            finest_groups: first_rows.len(),
            selected_rows: selection.len(),
        };
        Ok(Table::new(parts, &row_ranks, &column_ranks))
    }
}

#[cfg(test)]
mod tests;
