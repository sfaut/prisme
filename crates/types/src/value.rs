//! Values held by the plan: literals of the queries and constants computed by
//! Prisme.
//!
//! Integers are held in an `i128`, which holds every integer of the MySQL
//! family, signed and unsigned. Decimals and temporal values keep the exact
//! text of their literal: the representation for computing is chosen with the
//! first computation done by Prisme, and a date keeps its text, zero dates
//! included (`docs/FEDERATED.md`, section 6).

use crate::TypeError;

/// Value of the plan.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// NULL.
    Null,
    /// Boolean, `TRUE` or `FALSE`.
    Boolean(bool),
    /// Integer.
    Integer(i128),
    /// Exact decimal.
    Decimal(Decimal),
    /// Floating point number (`1e3` in MySQL).
    Float(f64),
    /// Character string.
    Text(String),
    /// Byte string (`X'4D'`).
    Binary(Vec<u8>),
    /// Date, as written (`DATE '2026-10-17'`, `'0000-00-00'`).
    Date(String),
    /// Time, as written.
    Time(String),
    /// Date and time, as written.
    DateTime(String),
}

/// Exact decimal number, kept as the text of its literal: an optional sign,
/// digits, and an optional decimal point followed by digits (`-12.50`, `.5`,
/// `5.`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Decimal {
    /// Text of the literal.
    text: String,
}

impl Decimal {
    /// Reads an exact decimal number.
    ///
    /// # Errors
    ///
    /// `TypeError::DecimalText` when `text` is not an optional sign followed
    /// by digits with at most one decimal point (an exponent makes a floating
    /// point number).
    pub fn parse(text: &str) -> Result<Self, TypeError> {
        let unsigned = text.strip_prefix(['+', '-']).unwrap_or(text);
        let (integer, fraction) = unsigned.split_once('.').unwrap_or((unsigned, ""));
        let digits = |part: &str| part.bytes().all(|byte| byte.is_ascii_digit());
        if integer.len() + fraction.len() == 0 || !digits(integer) || !digits(fraction) {
            return Err(TypeError::DecimalText(text.to_string()));
        }
        Ok(Self {
            text: text.to_string(),
        })
    }

    /// Text of the literal, as written.
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// Number of digits after the decimal point.
    pub fn scale(&self) -> usize {
        self.text
            .split_once('.')
            .map_or(0, |(_, fraction)| fraction.len())
    }

    /// Number of significant digits: those of the integer part without its
    /// leading zeros, plus the scale; 1 at least.
    pub fn precision(&self) -> usize {
        let unsigned = self.text.trim_start_matches(['+', '-']);
        let integer = unsigned
            .split_once('.')
            .map_or(unsigned, |(integer, _)| integer);
        (integer.trim_start_matches('0').len() + self.scale()).max(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimal_keeps_its_text() {
        for text in ["12.50", "-0.001", "+7", ".5", "5.", "0"] {
            assert_eq!(Decimal::parse(text).unwrap().as_str(), text);
        }
    }

    #[test]
    fn decimal_refuses_what_is_not_exact() {
        for text in ["", ".", "-", "1e3", "1.2.3", "12a", " 1"] {
            assert_eq!(
                Decimal::parse(text),
                Err(TypeError::DecimalText(text.to_string()))
            );
        }
    }

    #[test]
    fn decimal_counts_its_digits() {
        let measure = |text: &str| {
            let decimal = Decimal::parse(text).unwrap();
            (decimal.precision(), decimal.scale())
        };
        assert_eq!(measure("12.50"), (4, 2));
        assert_eq!(measure("-0.001"), (3, 3));
        assert_eq!(measure("007"), (1, 0));
        assert_eq!(measure("0"), (1, 0));
        assert_eq!(measure(".5"), (1, 1));
    }

    #[test]
    fn integer_value_holds_every_integer_of_the_mysql_family() {
        assert_eq!(
            Value::Integer(i128::from(u64::MAX)),
            Value::Integer(18_446_744_073_709_551_615)
        );
        assert_eq!(
            Value::Integer(i128::from(i64::MIN)),
            Value::Integer(-9_223_372_036_854_775_808)
        );
    }
}
