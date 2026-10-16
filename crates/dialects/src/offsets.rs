//! Byte offsets of the positions given by the tokenizer.
//!
//! `sqlparser` gives each token a line and a column, both counted in
//! characters from 1. The text is edited in bytes, so each position is
//! converted. An `OffsetIndex` is built once per text, in a single pass: a
//! conversion that scans its line each time made a long single-line query
//! quadratic (16 KB took 5 s to beautify).

use sqlparser::tokenizer::Location;

/// Positions of the characters of a text, to convert the tokenizer's line and
/// column into byte offsets in constant time.
pub(crate) struct OffsetIndex {
    /// Index, in characters, of the first character of each line.
    line_starts: Vec<usize>,
    /// Byte offset of each character, followed by the length of the text.
    byte_of_char: Vec<usize>,
}

impl OffsetIndex {
    /// Indexes the characters of `sql`.
    pub(crate) fn new(sql: &str) -> Self {
        let mut line_starts = vec![0];
        let mut byte_of_char = Vec::with_capacity(sql.len());
        for (byte, character) in sql.char_indices() {
            byte_of_char.push(byte);
            if character == '\n' {
                line_starts.push(byte_of_char.len());
            }
        }
        byte_of_char.push(sql.len());
        Self {
            line_starts,
            byte_of_char,
        }
    }

    /// Byte offset of a tokenizer position, `None` if it is outside the text.
    ///
    /// A column right after the last character of the text is valid (end of the
    /// last token). The line and the column of `location` count from 1, in
    /// characters.
    pub(crate) fn offset(&self, location: Location) -> Option<usize> {
        let line = usize::try_from(location.line).ok()?.checked_sub(1)?;
        let column = usize::try_from(location.column).ok()?.checked_sub(1)?;
        let line_start = *self.line_starts.get(line)?;
        self.byte_of_char.get(line_start + column).copied()
    }
}

#[cfg(test)]
mod tests {
    use sqlparser::tokenizer::Location;

    use super::OffsetIndex;

    /// Tokenizer position, from its line and column.
    fn at(line: u64, column: u64) -> Location {
        Location { line, column }
    }

    #[test]
    fn converts_positions_on_several_lines() {
        let index = OffsetIndex::new("SELECT 1\nFROM t");
        assert_eq!(index.offset(at(1, 1)), Some(0));
        assert_eq!(index.offset(at(1, 8)), Some(7));
        assert_eq!(index.offset(at(2, 1)), Some(9));
        assert_eq!(index.offset(at(2, 6)), Some(14));
    }

    #[test]
    fn counts_columns_in_characters_not_bytes() {
        // "é" takes two bytes for one column: "a", column 4, is at byte 4,
        // not at byte 3 as a column minus one would give.
        let index = OffsetIndex::new("é, a");
        assert_eq!(index.offset(at(1, 2)), Some(2));
        assert_eq!(index.offset(at(1, 4)), Some(4));
    }

    #[test]
    fn accepts_the_end_of_the_text_and_refuses_beyond() {
        let index = OffsetIndex::new("ab");
        assert_eq!(index.offset(at(1, 3)), Some(2));
        assert_eq!(index.offset(at(1, 4)), None);
        assert_eq!(index.offset(at(2, 1)), None);
        assert_eq!(index.offset(at(0, 1)), None);
    }
}
