//! Engine of the Pivoteur, the pivot table under the result grid: groups the
//! rows of a result by the values of its levels, computes its measures with
//! every subtotal and total, then lays out the table, which the page reads
//! window by window.
//!
//! First slice of the lot 1 of its specification: a level is a column of the
//! result, its value as it is; the measures are `COUNT(*)`, `COUNT(col)` and
//! `SUM(col)`, in ANSI SQL. The dialect of the Pivoteur, with its
//! expressions, comes with the next slices.
//!
//! The crate does not depend on its target: its tests run natively, and the
//! Worker of the page runs it compiled to WebAssembly (`crates/web-engine`).

mod groups;
mod measures;
mod table;

use std::cell::OnceCell;
use std::fmt;

use prisme_store::{Approx, Dictionary, Exact, NumberError, Store};

use crate::measures::Input;
pub use crate::measures::Value;
pub use crate::table::{Header, Label, Line, Table};

/// Family of the type of a column, as far as the measures need it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    /// Exact numbers: the `integer` and `decimal` families.
    Exact,
    /// Floats: the `float` family (`DOUBLE`).
    Float,
    /// Any other family, which `SUM` refuses.
    Other,
}

/// Measure of a table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Measure {
    /// `COUNT(*)`: number of rows.
    CountStar,
    /// `COUNT(col)`: number of rows where the column is not NULL.
    Count(usize),
    /// `SUM(col)`: sum of the values of a column of numbers that are not
    /// NULL; NULL without any.
    Sum(usize),
}

/// Table asked of the engine.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Request {
    /// Columns of the levels of the rows, the outermost first.
    pub rows: Vec<usize>,
    /// Columns of the levels of the columns, the outermost first.
    pub columns: Vec<usize>,
    /// Measures, in the order of their zone.
    pub measures: Vec<Measure>,
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
    /// Measure `measure` sums a column that does not hold numbers.
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
    /// The sum of measure `measure` goes beyond 128 bits.
    Overflow {
        /// Measure, by its index.
        measure: usize,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::UnknownColumn(column) => write!(formatter, "no column {column}"),
            Error::UnknownRow(row) => write!(formatter, "no row {row}"),
            Error::Ranks(column) => write!(formatter, "ranks of column {column} inconsistent"),
            Error::NotNumeric { measure } => {
                write!(
                    formatter,
                    "measure {measure}: sum of a column without numbers"
                )
            }
            Error::Number { column, error } => write!(formatter, "column {column}, {error}"),
            Error::Overflow { measure } => {
                write!(formatter, "measure {measure}: sum beyond 128 bits")
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
            let mut ranks = vec![0u32; order.len()];
            for (rank, &code) in order.iter().enumerate() {
                ranks[code as usize] = rank as u32;
            }
            ranks
        }))
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
        if family == Family::Other {
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

    /// Values measure `measure` reads.
    fn input(&self, index: usize, measure: Measure) -> Result<Input<'_>, Error> {
        let validity = |column: usize| {
            self.store
                .column(column)
                .map(|cells| cells.validity())
                .ok_or(Error::UnknownColumn(column))
        };
        Ok(match measure {
            Measure::CountStar => Input::Rows,
            Measure::Count(column) => Input::Cells(validity(column)?),
            Measure::Sum(column) => match self.numbers(index, column)? {
                Numbers::Exact(numbers) => Input::Exact(numbers, validity(column)?),
                Numbers::Float(numbers) => Input::Float(numbers, validity(column)?),
            },
        })
    }

    /// Computes a table over the rows kept.
    pub fn compute(&self, request: &Request) -> Result<Table, Error> {
        let all: Vec<u32>;
        let selection: &[u32] = match &self.kept {
            Some(rows) => rows,
            None => {
                all = (0..self.store.row_count() as u32).collect();
                &all
            }
        };
        let row_levels = request
            .rows
            .iter()
            .map(|&column| self.dictionary(column))
            .collect::<Result<Vec<_>, _>>()?;
        let column_levels = request
            .columns
            .iter()
            .map(|&column| self.dictionary(column))
            .collect::<Result<Vec<_>, _>>()?;
        let inputs = request
            .measures
            .iter()
            .enumerate()
            .map(|(index, &measure)| self.input(index, measure))
            .collect::<Result<Vec<_>, _>>()?;
        let ranks = |columns: &[usize]| {
            columns
                .iter()
                .map(|&column| self.ranks(column))
                .collect::<Result<Vec<_>, _>>()
        };
        let (row_ranks, column_ranks) = (ranks(&request.rows)?, ranks(&request.columns)?);

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
