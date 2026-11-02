//! Numbers of a column, read once from the text of its cells: exact numbers
//! (`integer` and `decimal` families) as integers scaled to the largest scale
//! of the column, floats (`float` family) as `f64`. No float ever carries an
//! exact number.
//!
//! Exact numbers hold on 128 bits, 38 digits. A text that is not a number,
//! or a number beyond 38 digits, is an error: a wrong value would lead to a
//! wrong total.

use std::cmp::Ordering;
use std::fmt;

use crate::column::TextColumn;

/// Most digits of an exact number on 128 bits.
const MAX_DIGITS: usize = 38;

/// Cell that a column of numbers cannot read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NumberError {
    /// The text of the cell of `row` is not a number.
    Invalid {
        /// Row of the cell.
        row: usize,
    },
    /// The number of the cell of `row` has more than 38 digits, once scaled
    /// to the scale of its column.
    TooLarge {
        /// Row of the cell.
        row: usize,
    },
}

impl fmt::Display for NumberError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NumberError::Invalid { row } => write!(formatter, "row {row}: not a number"),
            NumberError::TooLarge { row } => {
                write!(formatter, "row {row}: number beyond {MAX_DIGITS} digits")
            }
        }
    }
}

impl std::error::Error for NumberError {}

/// Exact numbers of a column, scaled to its largest scale: `12.5` and `3.25`
/// become 1250 and 325, with a scale of 2.
#[derive(Debug)]
pub struct Exact {
    /// Value of each row, 0 for NULL.
    pub values: Vec<i128>,
    /// Number of digits after the point.
    pub scale: u32,
}

/// Floats of a column.
#[derive(Debug)]
pub struct Approx {
    /// Value of each row, 0 for NULL.
    pub values: Vec<f64>,
}

/// Parts of an exact number written as text: `-12.50` gives a minus sign,
/// `12` and `50`.
struct Parts<'a> {
    /// Is the number negative?
    negative: bool,
    /// Digits before the point.
    whole: &'a [u8],
    /// Digits after the point.
    fraction: &'a [u8],
}

/// Splits an exact number: an optional sign, digits, then an optional point
/// followed by digits, at least one digit in all, as the servers write them.
fn parts(text: &str) -> Option<Parts<'_>> {
    let bytes = text.as_bytes();
    let (negative, unsigned) = match bytes.first() {
        Some(b'-') => (true, &bytes[1..]),
        Some(b'+') => (false, &bytes[1..]),
        _ => (false, bytes),
    };
    let (whole, fraction) = match unsigned.iter().position(|&byte| byte == b'.') {
        Some(point) => (&unsigned[..point], &unsigned[point + 1..]),
        None => (unsigned, &unsigned[..0]),
    };
    let digits = |part: &[u8]| part.iter().all(u8::is_ascii_digit);
    (digits(whole) && digits(fraction) && !(whole.is_empty() && fraction.is_empty())).then_some(
        Parts {
            negative,
            whole,
            fraction,
        },
    )
}

/// Value of the digits of an exact number scaled to `scale`, `None` beyond
/// 128 bits.
fn scaled(parts: &Parts, scale: usize) -> Option<i128> {
    let mut value = 0i128;
    let padding = scale - parts.fraction.len();
    let digits = parts.whole.iter().chain(parts.fraction);
    for &digit in digits.chain(std::iter::repeat_n(&b'0', padding)) {
        value = value
            .checked_mul(10)?
            .checked_add(i128::from(digit - b'0'))?;
    }
    if parts.negative {
        value.checked_neg()
    } else {
        Some(value)
    }
}

impl Exact {
    /// Reads the exact numbers of a column, scaled to the largest scale of
    /// its cells.
    pub fn of(column: &TextColumn) -> Result<Self, NumberError> {
        let mut scale = 0;
        for (row, cell) in column.cells().enumerate() {
            if let Some(text) = cell {
                let parts = parts(text).ok_or(NumberError::Invalid { row })?;
                scale = scale.max(parts.fraction.len());
            }
        }
        if scale > MAX_DIGITS {
            let row = column
                .cells()
                .position(|cell| {
                    cell.and_then(parts)
                        .is_some_and(|parts| parts.fraction.len() > MAX_DIGITS)
                })
                .unwrap_or(0);
            return Err(NumberError::TooLarge { row });
        }
        let mut values = Vec::with_capacity(column.len());
        for (row, cell) in column.cells().enumerate() {
            let value = match cell.and_then(parts) {
                Some(parts) => scaled(&parts, scale).ok_or(NumberError::TooLarge { row })?,
                None => 0,
            };
            values.push(value);
        }
        Ok(Exact {
            values,
            scale: scale as u32,
        })
    }
}

impl Approx {
    /// Reads the floats of a column. `NaN`, `Infinity` and `-Infinity`, which
    /// PostgreSQL writes, are read too.
    pub fn of(column: &TextColumn) -> Result<Self, NumberError> {
        let mut values = Vec::with_capacity(column.len());
        for (row, cell) in column.cells().enumerate() {
            let value = match cell {
                Some(text) => text
                    .parse::<f64>()
                    .map_err(|_| NumberError::Invalid { row })?,
                None => 0.0,
            };
            values.push(value);
        }
        Ok(Approx { values })
    }
}

/// Is a text an exact number, whatever its number of digits?
pub fn is_exact(text: &str) -> bool {
    parts(text).is_some()
}

/// Reads an exact number written as text, with its scale: `12.50` gives 1250
/// and 2. `None` for any other text, or beyond 128 bits.
pub fn parse_exact(text: &str) -> Option<(i128, u32)> {
    let parts = parts(text)?;
    let scale = parts.fraction.len();
    Some((scaled(&parts, scale)?, scale as u32))
}

/// Compares two exact numbers, each given with its scale.
pub fn compare_exact((a, a_scale): (i128, u32), (b, b_scale): (i128, u32)) -> Ordering {
    let raised = |value: i128, digits: u32| {
        if value == 0 {
            return Some(0);
        }
        10i128
            .checked_pow(digits)
            .and_then(|factor| value.checked_mul(factor))
    };
    let scale = a_scale.max(b_scale);
    // Only one of them is raised: beyond 128 bits, it is the larger in magnitude.
    match (raised(a, scale - a_scale), raised(b, scale - b_scale)) {
        (Some(a), Some(b)) => a.cmp(&b),
        (None, _) if a < 0 => Ordering::Less,
        (None, _) => Ordering::Greater,
        (_, None) if b < 0 => Ordering::Greater,
        (_, None) => Ordering::Less,
    }
}

/// Writes an exact number scaled to `scale`: `-1250` with a scale of 2 gives
/// `-12.50`.
pub fn format_exact(value: i128, scale: u32) -> String {
    let sign = if value < 0 { "-" } else { "" };
    let magnitude = value.unsigned_abs();
    if scale == 0 {
        return format!("{sign}{magnitude}");
    }
    let divisor = 10u128.pow(scale);
    format!(
        "{sign}{}.{:0width$}",
        magnitude / divisor,
        magnitude % divisor,
        width = scale as usize
    )
}

/// Writes a float in its shortest form that reads back the same value, with
/// an exponent outside 10^-6 to 10^21, as MySQL and JavaScript do: `0.1`,
/// `1e300`; `NaN`, `Infinity` and `-Infinity` as PostgreSQL writes them.
pub fn format_float(value: f64) -> String {
    if value.is_nan() {
        return "NaN".to_owned();
    }
    if value.is_infinite() {
        return if value > 0.0 { "Infinity" } else { "-Infinity" }.to_owned();
    }
    let magnitude = value.abs();
    if magnitude != 0.0 && !(1e-6..1e21).contains(&magnitude) {
        format!("{value:e}")
    } else {
        format!("{value}")
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_number_is_read_with_its_scale() {
        assert_eq!(super::parse_exact("12.50"), Some((1250, 2)));
        assert_eq!(super::parse_exact("-3"), Some((-3, 0)));
        assert_eq!(super::parse_exact("abc"), None);
        assert_eq!(super::parse_exact(""), None);
        assert_eq!(super::parse_exact(&"9".repeat(40)), None);
    }

    #[test]
    fn numbers_of_different_scales_compare_by_value() {
        use std::cmp::Ordering::{Equal, Greater, Less};

        use super::compare_exact;
        assert_eq!(compare_exact((1250, 2), (125, 1)), Equal);
        assert_eq!(compare_exact((1250, 2), (13, 0)), Less);
        assert_eq!(compare_exact((-1, 0), (0, 3)), Less);
        // Raised beyond 128 bits.
        assert_eq!(compare_exact((1, 0), (1, 45)), Greater);
        assert_eq!(compare_exact((-1, 0), (1, 45)), Less);
        assert_eq!(compare_exact((1, 45), (1, 0)), Less);
        assert_eq!(compare_exact((1, 45), (-1, 0)), Greater);
        assert_eq!(compare_exact((0, 0), (1, 45)), Less);
    }

    use super::*;

    fn column(cells: &[Option<&str>]) -> TextColumn {
        let mut column = TextColumn::default();
        for &cell in cells {
            column.push(cell);
        }
        column
    }

    #[test]
    fn exact_numbers_take_the_largest_scale_of_the_column() {
        let numbers = Exact::of(&column(&[
            Some("12.5"),
            None,
            Some("-3.25"),
            Some("7"),
            Some(".5"),
        ]))
        .unwrap();
        assert_eq!(numbers.scale, 2);
        assert_eq!(numbers.values, [1250, 0, -325, 700, 50]);
    }

    #[test]
    fn exact_numbers_hold_unsigned_64_bit_integers() {
        let numbers = Exact::of(&column(&[Some("18446744073709551615")])).unwrap();
        assert_eq!(numbers.values, [18_446_744_073_709_551_615]);
    }

    #[test]
    fn a_text_that_is_not_an_exact_number_is_an_error() {
        for text in ["", "-", ".", "1.2.3", "1e3", " 1", "abc", "NaN"] {
            assert_eq!(
                Exact::of(&column(&[Some("1"), Some(text)])).unwrap_err(),
                NumberError::Invalid { row: 1 },
                "{text:?}"
            );
        }
    }

    #[test]
    fn an_exact_number_beyond_38_digits_is_an_error() {
        let digits = "9".repeat(39);
        assert_eq!(
            Exact::of(&column(&[Some("1"), Some(&digits)])).unwrap_err(),
            NumberError::TooLarge { row: 1 }
        );
        // 37 digits before the point, scaled to 2 digits after it: 39 digits.
        let whole = "9".repeat(37);
        assert_eq!(
            Exact::of(&column(&[Some(&whole), Some("0.25")])).unwrap_err(),
            NumberError::TooLarge { row: 0 }
        );
    }

    #[test]
    fn floats_are_read_with_the_values_of_postgresql() {
        let numbers = Approx::of(&column(&[
            Some("0.1"),
            None,
            Some("-1e300"),
            Some("Infinity"),
            Some("NaN"),
        ]))
        .unwrap();
        assert_eq!(numbers.values[..4], [0.1, 0.0, -1e300, f64::INFINITY]);
        assert!(numbers.values[4].is_nan());
        assert_eq!(
            Approx::of(&column(&[Some("1,5")])).unwrap_err(),
            NumberError::Invalid { row: 0 }
        );
    }

    #[test]
    fn exact_numbers_are_written_with_their_scale() {
        assert_eq!(format_exact(-1250, 2), "-12.50");
        assert_eq!(format_exact(5, 3), "0.005");
        assert_eq!(format_exact(-5, 3), "-0.005");
        assert_eq!(format_exact(42, 0), "42");
        assert_eq!(format_exact(i128::MIN, 0), i128::MIN.to_string());
    }

    #[test]
    fn floats_are_written_in_their_shortest_form() {
        assert_eq!(format_float(0.1 + 0.2), "0.30000000000000004");
        assert_eq!(format_float(1.0), "1");
        assert_eq!(format_float(0.0), "0");
        assert_eq!(format_float(1e300), "1e300");
        assert_eq!(format_float(1.5e-7), "1.5e-7");
        assert_eq!(format_float(123_456.5), "123456.5");
        assert_eq!(format_float(f64::NEG_INFINITY), "-Infinity");
    }
}
