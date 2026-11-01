//! Dictionary of a column: a code per row, one code per distinct value, NULL
//! included. The groups of the Pivoteur are found on these codes: two cells
//! share a code exactly when their texts are equal, byte for byte, and NULL
//! has a code of its own, apart from the text `NULL` and from the empty text.

use crate::column::TextColumn;
use crate::hash::FxMap;

/// Codes of the rows of a column.
#[derive(Debug)]
pub struct Dictionary {
    /// Code of each row.
    pub codes: Vec<u32>,
    /// First row of each code, in the order of their first appearance: the
    /// value of the code is the cell of that row.
    pub first_rows: Vec<u32>,
}

impl Dictionary {
    /// Encodes a column.
    ///
    /// # Panics
    ///
    /// If the column has more than 2^32 rows.
    pub fn of(column: &TextColumn) -> Self {
        let mut index: FxMap<&str, u32> = FxMap::default();
        let mut null_code = None;
        let mut first_rows: Vec<u32> = Vec::new();
        let mut codes = Vec::with_capacity(column.len());
        for (row, cell) in column.cells().enumerate() {
            let row = u32::try_from(row).expect("rows below 2^32");
            let new_code = || {
                first_rows.push(row);
                (first_rows.len() - 1) as u32
            };
            let code = match cell {
                Some(text) => *index.entry(text).or_insert_with(new_code),
                None => *null_code.get_or_insert_with(new_code),
            };
            codes.push(code);
        }
        Dictionary { codes, first_rows }
    }

    /// Number of distinct values, NULL included.
    pub fn len(&self) -> usize {
        self.first_rows.len()
    }

    /// Has the column no value?
    pub fn is_empty(&self) -> bool {
        self.first_rows.is_empty()
    }

    /// Value of `code`, read in its column: its text, `None` for NULL.
    ///
    /// # Panics
    ///
    /// If `code` is not a code of the dictionary.
    pub fn value<'a>(&self, column: &'a TextColumn, code: u32) -> Option<&'a str> {
        column.get(self.first_rows[code as usize] as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equal_texts_share_a_code_and_null_has_its_own() {
        let mut column = TextColumn::default();
        for cell in [
            Some("Nord"),
            None,
            Some("NULL"),
            Some(""),
            Some("Nord"),
            None,
        ] {
            column.push(cell);
        }
        let dictionary = Dictionary::of(&column);
        assert_eq!(dictionary.codes, [0, 1, 2, 3, 0, 1]);
        assert_eq!(dictionary.len(), 4);
        let values: Vec<_> = (0..4).map(|code| dictionary.value(&column, code)).collect();
        assert_eq!(values, [Some("Nord"), None, Some("NULL"), Some("")]);
    }

    #[test]
    fn texts_differing_by_case_or_spaces_have_their_own_codes() {
        let mut column = TextColumn::default();
        for cell in ["fr", "FR", "fr "] {
            column.push(Some(cell));
        }
        assert_eq!(Dictionary::of(&column).codes, [0, 1, 2]);
    }
}
