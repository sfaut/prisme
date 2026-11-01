//! Text column: the cells of one column of a result, end to end, as the
//! server sent them. This is the layout of the copy the page makes
//! (`bench/pivot/js/transfer.mjs`): UTF-8 text, the offset of each cell, the
//! validity of each cell.

use std::fmt;

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

    /// Validity of each row: `false` for NULL.
    pub fn validity(&self) -> &[bool] {
        &self.valid
    }

    /// Cells, in the order of the rows.
    pub fn cells(&self) -> impl Iterator<Item = Option<&str>> {
        self.valid
            .iter()
            .zip(self.offsets.windows(2))
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
            self.text.push_str(text);
        }
        let end = u32::try_from(self.text.len()).expect("text of the column below 4 GiB");
        self.offsets.push(end);
        self.valid.push(cell.is_some());
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
        self.text.push_str(text);
        self.offsets
            .extend(offsets[1..].iter().map(|&offset| start + offset));
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
