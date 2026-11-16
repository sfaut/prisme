//! Numbers of a column, read once from the text of its cells: exact numbers
//! (`integer` and `decimal` families) as integers scaled to the largest scale
//! of the column, floats (`float` family) as `f64`. No float ever carries an
//! exact number.
//!
//! Exact numbers hold on 128 bits, 38 digits. A text that is not a number,
//! or a number beyond 38 digits, is an error: a wrong value would lead to a
//! wrong total. They are kept on 64 bits while they all hold there, as most
//! columns do, and on 128 bits from the first one that does not: half the
//! bytes for the numbers of a large result (`docs/PERFORMANCES.md`, chapter
//! 17).
//!
//! The numbers of a column extend to the rows appended to it since they were
//! read (`extend`). A finer scale among them scales the numbers already read
//! again, as a reading of the whole column would.

use std::cmp::Ordering;
use std::fmt;

use crate::column::TextColumn;
use crate::growth;

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
#[derive(Debug, Default)]
pub struct Exact {
    /// Value of each row, 0 for NULL.
    pub values: ExactValues,
    /// Number of digits after the point.
    pub scale: u32,
}

/// Values of exact numbers: on 64 bits while they all hold there, on 128
/// bits from the first one that does not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExactValues {
    /// Every value holds on 64 bits.
    Narrow(Vec<i64>),
    /// A value at least does not hold on 64 bits.
    Wide(Vec<i128>),
}

impl Default for ExactValues {
    fn default() -> Self {
        ExactValues::Narrow(Vec::new())
    }
}

impl From<Vec<i128>> for ExactValues {
    /// Values on 64 bits if they all hold there.
    fn from(values: Vec<i128>) -> Self {
        let mut exact = ExactValues::default();
        exact.reserve(values.len());
        for value in values {
            exact.push(value);
        }
        exact
    }
}

impl ExactValues {
    /// Number of values.
    pub fn len(&self) -> usize {
        match self {
            ExactValues::Narrow(values) => values.len(),
            ExactValues::Wide(values) => values.len(),
        }
    }

    /// Is there no value?
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Value of `row`.
    ///
    /// # Panics
    ///
    /// If `row` is not a row of the values.
    pub fn get(&self, row: usize) -> i128 {
        match self {
            ExactValues::Narrow(values) => i128::from(values[row]),
            ExactValues::Wide(values) => values[row],
        }
    }

    /// Values, on 128 bits.
    pub fn to_vec(&self) -> Vec<i128> {
        (0..self.len()).map(|row| self.get(row)).collect()
    }

    /// Gives back the room reserved for the values to come.
    pub fn shrink_to_fit(&mut self) {
        match self {
            ExactValues::Narrow(values) => values.shrink_to_fit(),
            ExactValues::Wide(values) => values.shrink_to_fit(),
        }
    }

    /// Makes room for `additional` more values.
    fn reserve(&mut self, additional: usize) {
        match self {
            ExactValues::Narrow(values) => growth::reserve(values, additional),
            ExactValues::Wide(values) => growth::reserve(values, additional),
        }
    }

    /// Moves the values to 128 bits.
    fn widen(&mut self) {
        if let ExactValues::Narrow(values) = self {
            let mut wide = Vec::with_capacity(values.capacity());
            wide.extend(values.iter().map(|&value| i128::from(value)));
            *self = ExactValues::Wide(wide);
        }
    }

    /// Appends a value, on 128 bits if it does not hold on 64.
    fn push(&mut self, value: i128) {
        if let ExactValues::Narrow(values) = self {
            match i64::try_from(value) {
                Ok(narrow) => return growth::push(values, narrow),
                Err(_) => self.widen(),
            }
        }
        if let ExactValues::Wide(values) = self {
            growth::push(values, value);
        }
    }

    /// Multiplies every value by `factor`, on 128 bits if a product does not
    /// hold on 64.
    ///
    /// # Returns
    ///
    /// The first row whose product does not hold on 128 bits, as an error.
    fn scale_by(&mut self, factor: i128) -> Result<(), usize> {
        if let ExactValues::Narrow(values) = self {
            let fits = i64::try_from(factor).ok().is_some_and(|factor| {
                values
                    .iter()
                    .all(|value| value.checked_mul(factor).is_some())
            });
            if fits {
                let factor = factor as i64;
                values.iter_mut().for_each(|value| *value *= factor);
                return Ok(());
            }
            self.widen();
        }
        if let ExactValues::Wide(values) = self {
            for (row, value) in values.iter_mut().enumerate() {
                *value = value.checked_mul(factor).ok_or(row)?;
            }
        }
        Ok(())
    }
}

/// Floats of a column.
#[derive(Debug, Default)]
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
        let mut numbers = Exact::default();
        numbers.extend(column, column.len())?;
        Ok(numbers)
    }

    /// Reads the numbers of the rows of the column after those already read,
    /// up to `end`, excluded, and scales them all to the largest scale. The
    /// error is the one a reading of the whole column gives: a text that is
    /// not a number first, then the first number beyond 38 digits once
    /// scaled. After an error, the numbers are to be dropped.
    ///
    /// # Panics
    ///
    /// If `end` goes beyond the rows of the column.
    pub fn extend(&mut self, column: &TextColumn, end: usize) -> Result<(), NumberError> {
        let start = self.values.len();
        if end <= start {
            return Ok(());
        }
        let mut scale = self.scale as usize;
        for (row, cell) in (start..end).zip(column.cells_in(start..end)) {
            if let Some(text) = cell {
                let parts = parts(text).ok_or(NumberError::Invalid { row })?;
                scale = scale.max(parts.fraction.len());
            }
        }
        if scale > MAX_DIGITS {
            let row = (start..end)
                .zip(column.cells_in(start..end))
                .find(|&(_, cell)| {
                    cell.and_then(parts)
                        .is_some_and(|parts| parts.fraction.len() > MAX_DIGITS)
                })
                .map_or(start, |(row, _)| row);
            return Err(NumberError::TooLarge { row });
        }
        if scale > self.scale as usize {
            // Below 39 digits, the factor holds on 128 bits.
            let factor = 10i128.pow((scale - self.scale as usize) as u32);
            self.values
                .scale_by(factor)
                .map_err(|row| NumberError::TooLarge { row })?;
            self.scale = scale as u32;
        }
        self.values.reserve(end - start);
        for (row, cell) in (start..end).zip(column.cells_in(start..end)) {
            let value = match cell.and_then(parts) {
                Some(parts) => scaled(&parts, scale).ok_or(NumberError::TooLarge { row })?,
                None => 0,
            };
            self.values.push(value);
        }
        Ok(())
    }
}

impl Approx {
    /// Reads the floats of a column. `NaN`, `Infinity` and `-Infinity`, which
    /// PostgreSQL writes, are read too.
    pub fn of(column: &TextColumn) -> Result<Self, NumberError> {
        let mut numbers = Approx::default();
        numbers.extend(column, column.len())?;
        Ok(numbers)
    }

    /// Reads the floats of the rows of the column after those already read,
    /// up to `end`, excluded. After an error, the numbers are to be dropped.
    ///
    /// # Panics
    ///
    /// If `end` goes beyond the rows of the column.
    pub fn extend(&mut self, column: &TextColumn, end: usize) -> Result<(), NumberError> {
        let start = self.values.len();
        if end <= start {
            return Ok(());
        }
        growth::reserve(&mut self.values, end - start);
        for (row, cell) in (start..end).zip(column.cells_in(start..end)) {
            let value = match cell {
                Some(text) => text
                    .parse::<f64>()
                    .map_err(|_| NumberError::Invalid { row })?,
                None => 0.0,
            };
            self.values.push(value);
        }
        Ok(())
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
    let mut text = String::new();
    write_float(&mut text, value).expect("a string takes any text");
    text
}

/// Number of characters of the text of a float (`format_float`), counted
/// without writing it: its characters are all ASCII, one byte each.
pub fn float_length(value: f64) -> usize {
    /// Writer that only counts the bytes written to it.
    struct Counter(usize);

    impl fmt::Write for Counter {
        fn write_str(&mut self, text: &str) -> fmt::Result {
            self.0 += text.len();
            Ok(())
        }
    }

    let mut counter = Counter(0);
    write_float(&mut counter, value).expect("a counter takes any text");
    counter.0
}

/// Writes a float as `format_float` gives it.
fn write_float(out: &mut impl fmt::Write, value: f64) -> fmt::Result {
    if value.is_nan() {
        return out.write_str("NaN");
    }
    if value.is_infinite() {
        return out.write_str(if value > 0.0 { "Infinity" } else { "-Infinity" });
    }
    let magnitude = value.abs();
    if magnitude != 0.0 && !(1e-6..1e21).contains(&magnitude) {
        write!(out, "{value:e}")
    } else {
        write!(out, "{value}")
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
        assert_eq!(
            numbers.values,
            ExactValues::Narrow(vec![1250, 0, -325, 700, 50])
        );
    }

    #[test]
    fn exact_numbers_hold_unsigned_64_bit_integers() {
        let numbers = Exact::of(&column(&[Some("1"), Some("18446744073709551615")])).unwrap();
        assert_eq!(
            numbers.values,
            ExactValues::Wide(vec![1, 18_446_744_073_709_551_615])
        );
    }

    #[test]
    fn exact_numbers_move_to_128_bits_when_a_scale_does_not_hold_on_64() {
        let mut cells = column(&[Some("9223372036854775807"), Some("-1")]);
        let mut numbers = Exact::of(&cells).unwrap();
        assert_eq!(numbers.values, ExactValues::Narrow(vec![i64::MAX, -1]));
        cells.push(Some("0.5"));
        numbers.extend(&cells, cells.len()).unwrap();
        assert_eq!(numbers.scale, 1);
        assert_eq!(
            numbers.values,
            ExactValues::Wide(vec![i128::from(i64::MAX) * 10, -10, 5])
        );
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
    fn numbers_appended_later_read_as_the_whole_column() {
        let mut cells = column(&[Some("12.5"), None, Some("7")]);
        let mut exact = Exact::of(&cells).unwrap();
        let mut approx = Approx::of(&cells).unwrap();
        assert_eq!((exact.values.to_vec(), exact.scale), (vec![125, 0, 70], 1));
        // A finer scale scales the numbers already read again.
        for cell in [Some("-0.125"), Some("3")] {
            cells.push(cell);
        }
        exact.extend(&cells, cells.len()).unwrap();
        approx.extend(&cells, cells.len()).unwrap();
        let whole = Exact::of(&cells).unwrap();
        assert_eq!((exact.values, exact.scale), (whole.values, whole.scale));
        assert_eq!(whole.scale, 3);
        assert_eq!(approx.values, Approx::of(&cells).unwrap().values);
    }

    #[test]
    fn numbers_appended_later_give_the_error_of_the_whole_column() {
        // The finer scale of the last row takes the first number beyond 38
        // digits: the error names its row, as a whole reading does.
        let whole = "9".repeat(37);
        let mut cells = column(&[Some(&whole), Some("1")]);
        let mut exact = Exact::of(&cells).unwrap();
        cells.push(Some("0.25"));
        assert_eq!(
            exact.extend(&cells, cells.len()).unwrap_err(),
            Exact::of(&cells).unwrap_err()
        );
        assert_eq!(
            Exact::of(&cells).unwrap_err(),
            NumberError::TooLarge { row: 0 }
        );
        let mut cells = column(&[Some("1")]);
        let mut approx = Approx::of(&cells).unwrap();
        cells.push(Some("x"));
        assert_eq!(
            approx.extend(&cells, cells.len()).unwrap_err(),
            NumberError::Invalid { row: 1 }
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

    #[test]
    fn the_length_of_a_float_is_that_of_its_text() {
        for value in [
            0.1 + 0.2,
            1.0,
            0.0,
            -0.0,
            1e300,
            -1.5e-7,
            123_456.5,
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::MIN_POSITIVE,
        ] {
            assert_eq!(
                super::float_length(value),
                format_float(value).len(),
                "{value}"
            );
        }
    }
}
