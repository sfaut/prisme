//! Line and column of a position, for the messages: the tokens carry byte
//! offsets, converted here when a message names a place in the query.

/// Line and column of a position, both from 1, the column in characters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Location {
    /// Line, from 1.
    pub line: usize,
    /// Column, in characters, from 1.
    pub column: usize,
}

/// Start of each line of a text, to find the line of a byte offset by
/// dichotomy.
#[derive(Debug, Clone)]
pub struct LineIndex {
    /// Byte offset of the first character of each line.
    line_starts: Vec<usize>,
}

impl LineIndex {
    /// Indexes the lines of `text`.
    pub fn new(text: &str) -> Self {
        let line_starts = std::iter::once(0)
            .chain(text.match_indices('\n').map(|(offset, _)| offset + 1))
            .collect();
        Self { line_starts }
    }

    /// Line and column of byte `offset` of `text`, the text indexed. The
    /// column counts the characters of its line before it: meant for the
    /// occasional position of a message, not for every token.
    pub fn location(&self, text: &str, offset: usize) -> Location {
        let line = self.line_starts.partition_point(|&start| start <= offset);
        let line_start = self.line_starts[line - 1];
        let offset = offset.min(text.len());
        Location {
            line,
            column: text[line_start..offset].chars().count() + 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locates_on_several_lines_in_characters() {
        let text = "SELECT 'é'\nFROM t";
        let index = LineIndex::new(text);
        assert_eq!(index.location(text, 0), Location { line: 1, column: 1 });
        // "'" after "é" (two bytes): byte 10, column 10.
        assert_eq!(
            index.location(text, 10),
            Location {
                line: 1,
                column: 10
            }
        );
        assert_eq!(index.location(text, 12), Location { line: 2, column: 1 });
        assert_eq!(
            index.location(text, text.len()),
            Location { line: 2, column: 7 }
        );
    }
}
