//! Codes of the rows of a dictionary, on the fewest bytes that hold them: one
//! byte while the dictionary holds 256 values at most, two up to 65,536,
//! four beyond. Most levels of a pivot table hold few values (a country, a
//! category, a month), and their codes then weigh a quarter of what four
//! bytes would (`docs/PERFORMANCES.md`, chapter 17). The codes widen once, in
//! place of their array, when the dictionary outgrows their width.

use crate::growth;

/// Code of each row of a dictionary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Codes {
    /// Codes below 256.
    U8(Vec<u8>),
    /// Codes below 65,536.
    U16(Vec<u16>),
    /// Any code.
    U32(Vec<u32>),
}

impl Default for Codes {
    fn default() -> Self {
        Codes::U8(Vec::new())
    }
}

impl From<Vec<u32>> for Codes {
    /// Codes on the fewest bytes that hold them all.
    fn from(codes: Vec<u32>) -> Self {
        let mut narrow = Codes::default();
        narrow.reserve(codes.len());
        for code in codes {
            narrow.push(code);
        }
        narrow
    }
}

impl Codes {
    /// Number of rows.
    pub fn len(&self) -> usize {
        match self {
            Codes::U8(codes) => codes.len(),
            Codes::U16(codes) => codes.len(),
            Codes::U32(codes) => codes.len(),
        }
    }

    /// Is there no row?
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Code of `row`.
    ///
    /// # Panics
    ///
    /// If `row` is not a row of the codes.
    #[inline]
    pub fn get(&self, row: usize) -> u32 {
        match self {
            Codes::U8(codes) => u32::from(codes[row]),
            Codes::U16(codes) => u32::from(codes[row]),
            Codes::U32(codes) => codes[row],
        }
    }

    /// Codes, on four bytes.
    pub fn to_vec(&self) -> Vec<u32> {
        (0..self.len()).map(|row| self.get(row)).collect()
    }

    /// Makes room for the codes of `additional` more rows.
    pub fn reserve(&mut self, additional: usize) {
        match self {
            Codes::U8(codes) => growth::reserve(codes, additional),
            Codes::U16(codes) => growth::reserve(codes, additional),
            Codes::U32(codes) => growth::reserve(codes, additional),
        }
    }

    /// Gives back the room reserved for the rows to come.
    pub fn shrink_to_fit(&mut self) {
        match self {
            Codes::U8(codes) => codes.shrink_to_fit(),
            Codes::U16(codes) => codes.shrink_to_fit(),
            Codes::U32(codes) => codes.shrink_to_fit(),
        }
    }

    /// Appends the code of a row, the codes widened first if it does not
    /// hold on their width.
    pub fn push(&mut self, code: u32) {
        match self {
            Codes::U8(codes) => match u8::try_from(code) {
                Ok(code) => growth::push(codes, code),
                Err(_) => {
                    *self = Codes::U16(widened(codes));
                    self.push(code);
                }
            },
            Codes::U16(codes) => match u16::try_from(code) {
                Ok(code) => growth::push(codes, code),
                Err(_) => {
                    *self = Codes::U32(widened(codes));
                    self.push(code);
                }
            },
            Codes::U32(codes) => growth::push(codes, code),
        }
    }
}

/// Codes on a wider type, with the room of the narrower ones.
fn widened<N: Copy, W: From<N>>(codes: &[N]) -> Vec<W> {
    let mut wide = Vec::with_capacity(codes.len() + codes.len() / 4 + 1);
    wide.extend(codes.iter().map(|&code| W::from(code)));
    wide
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_take_the_fewest_bytes_that_hold_them() {
        let mut codes = Codes::default();
        for code in 0..256 {
            codes.push(code);
        }
        assert!(matches!(codes, Codes::U8(_)));
        codes.push(256);
        assert!(matches!(codes, Codes::U16(_)));
        codes.push(65_535);
        assert!(matches!(codes, Codes::U16(_)));
        codes.push(65_536);
        assert!(matches!(codes, Codes::U32(_)));
        let mut expected: Vec<u32> = (0..257).collect();
        expected.extend([65_535, 65_536]);
        assert_eq!(codes.to_vec(), expected);
        assert_eq!(Codes::from(vec![3, 300]), Codes::U16(vec![3, 300]));
    }
}
