//! Dictionary of a column: a code per row, one code per distinct value, NULL
//! included. The groups of the Pivoteur are found on these codes: two cells
//! share a code exactly when their texts are equal, byte for byte, and NULL
//! has a code of its own, apart from the text `NULL` and from the empty text.
//!
//! The rows of a result arrive while the Pivoteur works on them: an `Encoder`
//! keeps the index of the texts of its dictionary, and gives the rows appended
//! later the codes of their values, new values taking the next codes. The
//! codes already given never change. The index holds the codes alone, and
//! reads their texts in the column (`hashbrown::HashTable`): it borrows
//! nothing, and lives as long as the dictionary.

use hashbrown::HashTable;
use hashbrown::hash_table::Entry;

use crate::column::TextColumn;
use crate::hash::fx_hash;

/// Codes of the rows of a column.
#[derive(Debug, Default)]
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
        let mut encoder = Encoder::default();
        encoder.extend(column, column.len());
        encoder.dictionary
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

/// Dictionary of a column under way, with the index of its texts.
#[derive(Debug, Default)]
pub struct Encoder {
    /// Codes of the rows encoded so far.
    dictionary: Dictionary,
    /// Code of each text, found by its hash and compared in the column.
    texts: HashTable<u32>,
    /// Code of NULL, once met.
    null: Option<u32>,
}

impl Encoder {
    /// Codes of the rows encoded so far.
    pub fn dictionary(&self) -> &Dictionary {
        &self.dictionary
    }

    /// Encodes the rows of the column after those already encoded, up to
    /// `end`, excluded. The column is the one encoded so far, grown since.
    ///
    /// # Panics
    ///
    /// If `end` goes beyond the rows of the column, or the column has more
    /// than 2^32 rows.
    pub fn extend(&mut self, column: &TextColumn, end: usize) {
        let Dictionary { codes, first_rows } = &mut self.dictionary;
        let start = codes.len();
        if end <= start {
            return;
        }
        codes.reserve(end - start);
        let text_of = |first_rows: &[u32], code: u32| {
            column
                .get(first_rows[code as usize] as usize)
                .expect("text of a code that is not NULL")
        };
        for (row, cell) in (start..end).zip(column.cells_in(start..end)) {
            let row = u32::try_from(row).expect("rows below 2^32");
            let next = first_rows.len() as u32;
            let code = match cell {
                Some(text) => {
                    let found = self.texts.entry(
                        fx_hash(text),
                        |&code| text_of(first_rows, code) == text,
                        |&code| fx_hash(text_of(first_rows, code)),
                    );
                    match found {
                        Entry::Occupied(entry) => *entry.get(),
                        Entry::Vacant(entry) => {
                            entry.insert(next);
                            first_rows.push(row);
                            next
                        }
                    }
                }
                None => *self.null.get_or_insert_with(|| {
                    first_rows.push(row);
                    next
                }),
            };
            codes.push(code);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn column(cells: &[Option<&str>]) -> TextColumn {
        let mut column = TextColumn::default();
        for &cell in cells {
            column.push(cell);
        }
        column
    }

    #[test]
    fn equal_texts_share_a_code_and_null_has_its_own() {
        let column = column(&[
            Some("Nord"),
            None,
            Some("NULL"),
            Some(""),
            Some("Nord"),
            None,
        ]);
        let dictionary = Dictionary::of(&column);
        assert_eq!(dictionary.codes, [0, 1, 2, 3, 0, 1]);
        assert_eq!(dictionary.len(), 4);
        let values: Vec<_> = (0..4).map(|code| dictionary.value(&column, code)).collect();
        assert_eq!(values, [Some("Nord"), None, Some("NULL"), Some("")]);
    }

    #[test]
    fn texts_differing_by_case_or_spaces_have_their_own_codes() {
        let column = column(&[Some("fr"), Some("FR"), Some("fr ")]);
        assert_eq!(Dictionary::of(&column).codes, [0, 1, 2]);
    }

    #[test]
    fn rows_appended_later_keep_the_codes_already_given() {
        let mut cells = column(&[Some("Nord"), None, Some("Sud")]);
        let mut encoder = Encoder::default();
        encoder.extend(&cells, 2);
        assert_eq!(encoder.dictionary().codes, [0, 1]);
        for cell in [Some("Est"), Some("Nord"), None] {
            cells.push(cell);
        }
        encoder.extend(&cells, cells.len());
        let whole = Dictionary::of(&cells);
        assert_eq!(encoder.dictionary().codes, whole.codes);
        assert_eq!(encoder.dictionary().first_rows, whole.first_rows);
        assert_eq!(whole.codes, [0, 1, 2, 3, 0, 1]);
    }

    #[test]
    fn many_values_appended_block_by_block_keep_their_codes() {
        // Enough values for the index to grow several times.
        let texts: Vec<String> = (0..5_000)
            .map(|value| format!("v{}", value % 3_000))
            .collect();
        let mut cells = TextColumn::default();
        let mut encoder = Encoder::default();
        for block in texts.chunks(700) {
            for text in block {
                cells.push(Some(text));
            }
            encoder.extend(&cells, cells.len());
        }
        let whole = Dictionary::of(&cells);
        assert_eq!(encoder.dictionary().codes, whole.codes);
        assert_eq!(whole.len(), 3_000);
    }
}
