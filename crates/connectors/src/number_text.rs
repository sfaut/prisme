//! Writing of the numbers that Prisme shows: plain decimal, whatever the
//! engine (`docs/TODO-SQLITE.md`, decision 14). An engine may send a
//! floating-point number in scientific notation: MySQL and MariaDB write a
//! `DOUBLE` of 10²⁰ as `1e20` (`docs/SQL-COMPARISON.md`, section 3.10).
//! Every connector gives its floating-point cells to this module, which
//! moves the decimal point instead of the exponent, digit for digit: the
//! value stays the one the engine wrote, only its writing changes.

use std::borrow::Cow;

/// Writes in plain decimal a number that an engine wrote in scientific
/// notation: `1e20` gives `100000000000000000000`, `1.2345678901234568e17`
/// gives `123456789012345680`, `-2.5E-3` gives `-0.0025`. The digits stay
/// those of the engine; trailing zeros after the point go, then the point if
/// nothing follows it. A text without exponent, or that is not a number
/// (`inf`, `NaN`), is returned as it is.
pub fn plain_decimal(text: &str) -> Cow<'_, str> {
    match scientific(text) {
        Some(parts) => Cow::Owned(parts.plain()),
        None => Cow::Borrowed(text),
    }
}

/// A number in scientific notation, split into its parts.
struct Scientific<'a> {
    /// Minus sign, or empty.
    sign: &'a str,
    /// Digits before the point of the mantissa.
    integer: &'a str,
    /// Digits after the point of the mantissa, possibly empty.
    fraction: &'a str,
    /// Power of ten of the exponent.
    exponent: i64,
}

/// Largest power of ten written out: beyond the range of a `DOUBLE`
/// (10³⁰⁸, 10⁻³²⁴ for the smallest), a text is left as it is rather than
/// written with thousands of zeros.
const MAX_EXPONENT: i64 = 400;

/// Splits `text` when it is a number in scientific notation:
/// `[+-]?digits[.digits](e|E)[+-]?digits`, at least one digit in the
/// mantissa, the exponent within `MAX_EXPONENT`.
fn scientific(text: &str) -> Option<Scientific<'_>> {
    let (mantissa, exponent) = text.split_once(['e', 'E'])?;
    let exponent: i64 = exponent.parse().ok()?;
    if exponent.abs() > MAX_EXPONENT {
        return None;
    }
    let (sign, unsigned) = match mantissa.as_bytes().first()? {
        b'-' => ("-", &mantissa[1..]),
        b'+' => ("", &mantissa[1..]),
        _ => ("", mantissa),
    };
    let (integer, fraction) = unsigned.split_once('.').unwrap_or((unsigned, ""));
    let digits_only = |part: &str| part.bytes().all(|byte| byte.is_ascii_digit());
    if integer.len() + fraction.len() == 0 || !digits_only(integer) || !digits_only(fraction) {
        return None;
    }
    Some(Scientific {
        sign,
        integer,
        fraction,
        exponent,
    })
}

impl Scientific<'_> {
    /// The number in plain decimal.
    fn plain(&self) -> String {
        let digits = format!("{}{}", self.integer, self.fraction);
        // Position of the point in `digits`, once the exponent applied.
        let point = i64::try_from(self.integer.len()).unwrap_or(i64::MAX) + self.exponent;
        let length = i64::try_from(digits.len()).unwrap_or(i64::MAX);
        let (integer, fraction) = if point <= 0 {
            let zeros = usize::try_from(-point).unwrap_or(0);
            (String::new(), format!("{}{digits}", "0".repeat(zeros)))
        } else if point >= length {
            let zeros = usize::try_from(point - length).unwrap_or(0);
            (format!("{digits}{}", "0".repeat(zeros)), String::new())
        } else {
            let split = usize::try_from(point).unwrap_or(0);
            (digits[..split].to_string(), digits[split..].to_string())
        };
        let integer = integer.trim_start_matches('0');
        let integer = if integer.is_empty() { "0" } else { integer };
        let fraction = fraction.trim_end_matches('0');
        let sign = if integer == "0" && fraction.is_empty() {
            ""
        } else {
            self.sign
        };
        if fraction.is_empty() {
            format!("{sign}{integer}")
        } else {
            format!("{sign}{integer}.{fraction}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moves_the_point_instead_of_the_exponent() {
        for (text, plain) in [
            ("1e20", "100000000000000000000"),
            ("1.5e300", &format!("15{}", "0".repeat(299))),
            ("1.2345678901234568e17", "123456789012345680"),
            ("1.0e+20", "100000000000000000000"),
            ("1.0e-07", "0.0000001"),
            ("-2.5E-3", "-0.0025"),
            ("+4.2e1", "42"),
            ("1.25e1", "12.5"),
            ("9.2233720368547758e+18", "9223372036854775800"),
            ("12.5e-1", "1.25"),
            ("0.5e1", "5"),
            (".5e1", "5"),
            ("5.e1", "50"),
            ("0e0", "0"),
            ("-0e5", "0"),
        ] {
            assert_eq!(plain_decimal(text), plain, "{text}");
        }
    }

    #[test]
    fn leaves_the_other_texts_as_they_are() {
        for text in [
            "100",
            "0.0000001",
            "3.141592653589793",
            "-12.50",
            "inf",
            "NaN",
            "-inf",
            "",
            "e5",
            "1e",
            "1e2.5",
            "1e999",
            "abc",
        ] {
            assert!(matches!(plain_decimal(text), Cow::Borrowed(_)), "{text}");
            assert_eq!(plain_decimal(text), text);
        }
    }
}
