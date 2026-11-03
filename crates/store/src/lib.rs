//! Store of the rows of a result, in columns: the text of each cell as the
//! server sent it, then the forms derived from it, built once when a reader
//! needs them (dictionary of the distinct values, exact numbers, floats).
//!
//! The rows of a result never change once received: the Pivoteur's Worker
//! copies them once into the store (`docs/QUESTIONS.md`, section 2, single
//! store of the results), then reads them as often as its tables need. The
//! crate does not depend on its target: its tests run natively, and the
//! Worker runs it compiled to WebAssembly (`crates/web-engine`).

mod column;
mod dictionary;
mod hash;
mod numbers;

pub use column::{BlockError, TextColumn};
pub use dictionary::Dictionary;
pub use hash::FxMap;
pub use numbers::{
    Approx, Exact, NumberError, compare_exact, format_exact, format_float, is_exact, parse_exact,
};

/// Rows of a result, one text column per column of the result.
#[derive(Debug, Default)]
pub struct Store {
    /// Columns, in the order of the result.
    columns: Vec<TextColumn>,
}

impl Store {
    /// Creates an empty store of `columns` columns.
    pub fn new(columns: usize) -> Self {
        Store {
            columns: (0..columns).map(|_| TextColumn::default()).collect(),
        }
    }

    /// Number of columns.
    pub fn column_count(&self) -> usize {
        self.columns.len()
    }

    /// Number of rows: that of the shortest column, the rows arriving column
    /// by column.
    pub fn row_count(&self) -> usize {
        self.columns.iter().map(TextColumn::len).min().unwrap_or(0)
    }

    /// Column `index`, `None` if there is none.
    pub fn column(&self, index: usize) -> Option<&TextColumn> {
        self.columns.get(index)
    }

    /// Column `index`, to append cells to it; `None` if there is none.
    pub fn column_mut(&mut self, index: usize) -> Option<&mut TextColumn> {
        self.columns.get_mut(index)
    }

    /// Appends a row, one cell per column (`None` for NULL).
    ///
    /// # Panics
    ///
    /// If the row does not have one cell per column.
    pub fn push_row(&mut self, cells: &[Option<&str>]) {
        assert_eq!(cells.len(), self.columns.len(), "one cell per column");
        for (column, &cell) in self.columns.iter_mut().zip(cells) {
            column.push(cell);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_are_appended_column_by_column() {
        let mut store = Store::new(2);
        store.push_row(&[Some("Nord"), None]);
        store.push_row(&[Some("Sud"), Some("30")]);
        assert_eq!(store.column_count(), 2);
        assert_eq!(store.row_count(), 2);
        let second = store.column(1).unwrap();
        assert_eq!(second.get(0), None);
        assert_eq!(second.get(1), Some("30"));
        assert!(store.column(2).is_none());
    }

    #[test]
    fn the_shortest_column_gives_the_number_of_rows() {
        let mut store = Store::new(2);
        store.column_mut(0).unwrap().push(Some("Nord"));
        assert_eq!(store.row_count(), 0);
    }
}
