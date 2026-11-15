//! Text column: the cells of one column of a result, end to end, as the
//! server sent them. This is the layout of the copy the page makes
//! (`bench/pivot/js/transfer.mjs`): UTF-8 text, the offset of each cell, the
//! validity of each cell.

use std::fmt;
use std::ops::Range;

use crate::growth;

/// Cells of one column of a result.
#[derive(Debug)]
pub struct TextColumn {
    /// Text of the cells, end to end.
    text: String,
    /// Offset of each cell in `text`, then the end of the last one: cell
    /// `row` runs from `offsets[row]` to `offsets[row + 1]`.
    offsets: Vec<u32>,
    /// Validity of each cell: `false` for NULL.
    valid: Vec<bool>,
}

impl Default for TextColumn {
    fn default() -> Self {
        TextColumn {
            text: String::new(),
            offsets: vec![0],
            valid: Vec::new(),
        }
    }
}

/// Block of cells refused by `TextColumn::append`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockError {
    /// The offsets do not match the rows and the bytes of the block.
    Offsets,
    /// The bytes are not UTF-8, or an offset cuts a character.
    Utf8,
    /// The text of the column would exceed 4 GiB, the reach of its offsets.
    TooLong,
}

impl fmt::Display for BlockError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            BlockError::Offsets => "offsets of the block inconsistent with its rows",
            BlockError::Utf8 => "cells of the block not in UTF-8",
            BlockError::TooLong => "text of the column beyond 4 GiB",
        })
    }
}

impl std::error::Error for BlockError {}

/// Place of the text of a cell in the text of its column (`TextColumn::bounds`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bounds {
    /// Offset of its first byte.
    start: u32,
    /// Offset after its last byte.
    end: u32,
}

impl Bounds {
    /// Place of NULL, which has no text: no text ends before it starts.
    const NULL: Bounds = Bounds { start: 1, end: 0 };
}

impl TextColumn {
    /// Number of rows.
    pub fn len(&self) -> usize {
        self.valid.len()
    }

    /// Has the column no row?
    pub fn is_empty(&self) -> bool {
        self.valid.is_empty()
    }

    /// Cell of `row`: its text, `None` for NULL.
    ///
    /// # Panics
    ///
    /// If `row` is not a row of the column.
    pub fn get(&self, row: usize) -> Option<&str> {
        self.valid[row]
            .then(|| &self.text[self.offsets[row] as usize..self.offsets[row + 1] as usize])
    }

    /// Place of the cell of `row` in the text of the column, which rows
    /// appended later leave as it is: the text of its value, read again by
    /// `text_of` without going through the offsets and the validity of the
    /// rows.
    ///
    /// # Panics
    ///
    /// If `row` is not a row of the column.
    pub fn bounds(&self, row: usize) -> Bounds {
        if self.valid[row] {
            Bounds {
                start: self.offsets[row],
                end: self.offsets[row + 1],
            }
        } else {
            Bounds::NULL
        }
    }

    /// Text of a cell from its place (`bounds`), `None` for NULL.
    pub fn text_of(&self, bounds: Bounds) -> Option<&str> {
        (bounds != Bounds::NULL).then(|| &self.text[bounds.start as usize..bounds.end as usize])
    }

    /// Validity of each row: `false` for NULL.
    pub fn validity(&self) -> &[bool] {
        &self.valid
    }

    /// Cells, in the order of the rows.
    pub fn cells(&self) -> impl Iterator<Item = Option<&str>> {
        self.cells_in(0..self.len())
    }

    /// Cells of some rows, in their order: those appended since a reader
    /// last read the column, without going through the others.
    ///
    /// # Panics
    ///
    /// If the rows go beyond those of the column.
    pub fn cells_in(&self, rows: Range<usize>) -> impl Iterator<Item = Option<&str>> {
        self.valid[rows.clone()]
            .iter()
            .zip(self.offsets[rows.start..rows.end + 1].windows(2))
            .map(|(&valid, bounds)| {
                valid.then(|| &self.text[bounds[0] as usize..bounds[1] as usize])
            })
    }

    /// Appends a cell, `None` for NULL.
    ///
    /// # Panics
    ///
    /// If the text of the column would exceed 4 GiB.
    pub fn push(&mut self, cell: Option<&str>) {
        if let Some(text) = cell {
            growth::reserve_text(&mut self.text, text.len());
            self.text.push_str(text);
        }
        let end = u32::try_from(self.text.len()).expect("text of the column below 4 GiB");
        growth::push(&mut self.offsets, end);
        growth::push(&mut self.valid, cell.is_some());
    }

    /// Appends a block of cells copied by the page: `bytes`, the UTF-8 text
    /// of the cells end to end; `offsets`, one per row plus the end of the
    /// last cell, counted from the start of `bytes`; `valid`, one byte per
    /// row, 0 for NULL. A refused block leaves the column unchanged.
    pub fn append(
        &mut self,
        bytes: &[u8],
        offsets: &[u32],
        valid: &[u8],
    ) -> Result<(), BlockError> {
        let consistent = offsets.len() == valid.len() + 1
            && offsets.first() == Some(&0)
            && offsets.last().map(|&end| end as usize) == Some(bytes.len())
            && offsets.windows(2).all(|pair| pair[0] <= pair[1]);
        if !consistent {
            return Err(BlockError::Offsets);
        }
        let text = std::str::from_utf8(bytes).map_err(|_| BlockError::Utf8)?;
        if !offsets
            .iter()
            .all(|&offset| text.is_char_boundary(offset as usize))
        {
            return Err(BlockError::Utf8);
        }
        let start = u32::try_from(self.text.len()).map_err(|_| BlockError::TooLong)?;
        start
            .checked_add(u32::try_from(bytes.len()).map_err(|_| BlockError::TooLong)?)
            .ok_or(BlockError::TooLong)?;
        growth::reserve_text(&mut self.text, text.len());
        self.text.push_str(text);
        growth::reserve(&mut self.offsets, valid.len());
        self.offsets
            .extend(offsets[1..].iter().map(|&offset| start + offset));
        growth::reserve(&mut self.valid, valid.len());
        self.valid.extend(valid.iter().map(|&byte| byte != 0));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cells_are_read_back_as_pushed() {
        let mut column = TextColumn::default();
        column.push(Some("Lille"));
        column.push(None);
        column.push(Some(""));
        column.push(Some("Nîmes"));
        assert_eq!(column.len(), 4);
        let cells: Vec<_> = column.cells().collect();
        assert_eq!(cells, [Some("Lille"), None, Some(""), Some("Nîmes")]);
        assert_eq!(column.validity(), [true, false, true, true]);
    }

    #[test]
    fn the_cells_of_some_rows_are_read_alone() {
        let mut column = TextColumn::default();
        for cell in [Some("a"), None, Some("bc"), Some("")] {
            column.push(cell);
        }
        let cells: Vec<_> = column.cells_in(1..3).collect();
        assert_eq!(cells, [None, Some("bc")]);
        assert_eq!(column.cells_in(4..4).count(), 0);
    }

    #[test]
    fn the_place_of_a_cell_reads_its_text_after_rows_are_appended() {
        let mut column = TextColumn::default();
        for cell in [Some("Lille"), None, Some("")] {
            column.push(cell);
        }
        let bounds: Vec<Bounds> = (0..3).map(|row| column.bounds(row)).collect();
        column.push(Some("Nîmes"));
        let texts: Vec<_> = bounds.iter().map(|&place| column.text_of(place)).collect();
        // NULL keeps apart from the empty text.
        assert_eq!(texts, [Some("Lille"), None, Some("")]);
        assert_eq!(column.text_of(column.bounds(3)), Some("Nîmes"));
    }

    #[test]
    fn a_block_goes_after_the_cells_already_there() {
        let mut column = TextColumn::default();
        column.push(Some("Nord"));
        column
            .append("SudÉté".as_bytes(), &[0, 3, 3, 8], &[1, 0, 1])
            .unwrap();
        let cells: Vec<_> = column.cells().collect();
        assert_eq!(cells, [Some("Nord"), Some("Sud"), None, Some("Été")]);
    }

    #[test]
    fn an_inconsistent_block_is_refused() {
        let mut column = TextColumn::default();
        assert_eq!(
            column.append(b"ab", &[0, 1], &[1, 1]),
            Err(BlockError::Offsets)
        );
        assert_eq!(
            column.append(b"ab", &[0, 1, 3], &[1, 1]),
            Err(BlockError::Offsets)
        );
        assert_eq!(
            column.append(b"ab", &[0, 2, 1], &[1, 1]),
            Err(BlockError::Offsets)
        );
        assert_eq!(column.append(&[0xff], &[0, 1], &[1]), Err(BlockError::Utf8));
        assert_eq!(
            column.append("é".as_bytes(), &[0, 1, 2], &[1, 1]),
            Err(BlockError::Utf8)
        );
        assert!(column.is_empty());
    }
}
