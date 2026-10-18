//! `CAST` and `CONVERT` towards MySQL and MariaDB, measured on MySQL 8.0.11
//! to 9.3.0, MariaDB 10.6.28 and 11.8.9 (`crates/probe/cases/cast*`).
//!
//! Within a dialect, the conversion keeps its form, its target checked
//! against the version of the server. Between the dialects:
//!
//! - a target the output lacks is written as its equivalent (`REAL` as
//!   `DOUBLE`, `INT` as `SIGNED`, `VARCHAR(n)` as `CHAR(n)`...), or is an
//!   error when it has none (`JSON`, `YEAR`, `INET6`...) (rule `cast.target`);
//! - the values that differ between the servers are errors, by the type of
//!   the operand: the fractional seconds, rounded by MySQL, truncated by
//!   MariaDB (`cast.fraction`); a negative or large number to `UNSIGNED`
//!   (`cast.unsigned`); a string whose integer follows a line break
//!   (`cast.spaces`); a string read as a date, of a time zone offset that
//!   MySQL reads and MariaDB ignores (`cast.temporal`); a number or bytes
//!   read as a date, NULL in MySQL, a zero date in MariaDB (`cast.invalid`);
//!   bytes read as characters, of invalid sequences (`cast.bytes`); a
//!   `FLOAT` out of its range, an error in MySQL, bounded in MariaDB
//!   (`cast.float`); an operand of unknown type for a target where a type
//!   differs (`cast.unknown`).
//!
//! A literal whose value has none of these differences is written as is.

use prisme_plan::{
    Cast, CastForm, CastTarget, CharTarget, CharWord, Charset, Expr, ExprKind, IntegerWord,
};
use prisme_types::{DataType, FloatType, Value};

use super::Emitter;
use crate::Dialect;
use crate::report::StepKind;
use crate::versions::Construct;

/// Largest number of digits of the integer part of a decimal that a `FLOAT`
/// holds (3.4e38).
const FLOAT_DIGITS: u8 = 38;

/// Smallest float that `UNSIGNED` reads differently in MySQL and MariaDB,
/// 2^63.
const UNSIGNED_FLOAT_BOUND: f64 = 9_223_372_036_854_775_808.0;

impl Emitter<'_> {
    /// Writes `CAST(...)` or `CONVERT(...)`.
    pub(super) fn cast(&mut self, expr: &Expr, cast: &Cast) {
        if self.source.dialect != self.output.dialect {
            self.cast_difference(expr, cast);
        }
        if let CastForm::AtTimeZone { .. } = cast.form
            && !self.has(Construct::CastAtTimeZone)
        {
            let message = format!(
                "{} has no `CAST(x AT TIME ZONE ...)`, the date and time in UTC of a `TIMESTAMP`",
                self.target()
            );
            self.error("cast.target", &message, expr.span);
        }
        let target = self.target_text(&cast.target, expr);
        let collation = self.moved_collation(&cast.target, expr);
        self.write(match cast.form {
            CastForm::Convert | CastForm::ConvertUsing => "CONVERT(",
            _ => "CAST(",
        });
        // The numeric targets read their operand as a number.
        if numeric(&cast.target) {
            self.number_expr(&cast.operand, 0);
        } else {
            // A string takes the text of a `JSON` in both dialects (`json`
            // module), the bytes of a hexadecimal literal.
            let string = matches!(cast.target, CastTarget::Char(_) | CastTarget::Binary(_));
            self.json_as_is = string;
            self.as_bytes = string;
            self.expr(&cast.operand, 0);
        }
        match &cast.form {
            CastForm::Cast => self.write(&format!(" AS {target})")),
            CastForm::AtTimeZone { zone, interval } => {
                let interval = if *interval { "INTERVAL " } else { "" };
                let zone = self.string(zone);
                self.write(&format!(" AT TIME ZONE {interval}{zone} AS {target})"));
            }
            CastForm::Convert => self.write(&format!(", {target})")),
            CastForm::ConvertUsing => {
                let charset = match &cast.target {
                    CastTarget::Char(CharTarget {
                        charset: Some(charset),
                        ..
                    }) => self.charset(charset),
                    _ => unreachable!("`CONVERT ... USING` converts to a character set"),
                };
                self.write(&format!(" USING {charset})"));
            }
        }
        if let Some(collation) = collation {
            self.write(&format!(" COLLATE {collation}"));
        }
    }

    /// Text of `target` in the output: as written, or as its equivalent when
    /// the output lacks it.
    fn target_text(&mut self, target: &CastTarget, expr: &Expr) -> String {
        let across = self.source.dialect != self.output.dialect;
        match target {
            CastTarget::Signed(word) => format!("SIGNED{}", integer_word(*word)),
            CastTarget::Unsigned(word) => format!("UNSIGNED{}", integer_word(*word)),
            CastTarget::Integer(word) => {
                if self.has(Construct::CastInteger) {
                    return integer_word(Some(*word)).trim_start().to_string();
                }
                self.target_rewrite("`INT`", "`SIGNED`", expr);
                format!("SIGNED{}", integer_word(Some(*word)))
            }
            CastTarget::Decimal {
                dec,
                precision,
                scale,
            } => {
                let name = if *dec { "DEC" } else { "DECIMAL" };
                if self.output.dialect == Dialect::MySql && scale.is_some_and(|scale| scale > 30) {
                    let message = format!(
                        "{} gives a `DECIMAL` 30 decimals at most, {} 38",
                        self.target(),
                        self.source_name()
                    );
                    self.error("cast.target", &message, expr.span);
                }
                match (precision, scale) {
                    (Some(precision), Some(scale)) => format!("{name}({precision}, {scale})"),
                    (Some(precision), None) => format!("{name}({precision})"),
                    _ => name.to_string(),
                }
            }
            CastTarget::Double { precision, digits } => {
                self.target_version(Construct::CastDouble, expr);
                if let Some((digits, decimals)) = digits {
                    if self.has(Construct::CastDoubleDigits) {
                        return format!("DOUBLE({digits}, {decimals})");
                    }
                    let message = format!(
                        "{} has no `CAST(x AS DOUBLE(m, d))`, which rounds the value to `d` decimals and bounds it to `m` digits",
                        self.target()
                    );
                    self.error("cast.target", &message, expr.span);
                    return "DOUBLE".to_string();
                }
                if *precision {
                    if self.has(Construct::CastDoubleSynonyms) {
                        return "DOUBLE PRECISION".to_string();
                    }
                    if across {
                        self.target_rewrite("`DOUBLE PRECISION`", "`DOUBLE`", expr);
                    }
                }
                "DOUBLE".to_string()
            }
            CastTarget::Float(bits) => {
                self.target_version(Construct::CastDouble, expr);
                let Some(bits) = bits else {
                    return "FLOAT".to_string();
                };
                if self.has(Construct::CastDoubleSynonyms) || !across {
                    self.target_version(Construct::CastDoubleSynonyms, expr);
                    return format!("FLOAT({bits})");
                }
                let name = if *bits > 24 { "DOUBLE" } else { "FLOAT" };
                self.target_rewrite("`FLOAT(p)`", &format!("`{name}`"), expr);
                name.to_string()
            }
            CastTarget::Real { float } => {
                if self.has(Construct::CastDoubleSynonyms) || !across {
                    self.target_version(Construct::CastDoubleSynonyms, expr);
                    return "REAL".to_string();
                }
                let name = if *float { "FLOAT" } else { "DOUBLE" };
                self.target_rewrite("`REAL`", &format!("`{name}`"), expr);
                name.to_string()
            }
            CastTarget::Char(char_target) => self.char_text(char_target, expr),
            CastTarget::Binary(length) => format!("BINARY{}", length_text(*length)),
            CastTarget::Date => "DATE".to_string(),
            CastTarget::DateTime(fraction) => {
                format!("DATETIME{}", length_text(fraction.map(u32::from)))
            }
            CastTarget::Time(fraction) => format!("TIME{}", length_text(fraction.map(u32::from))),
            CastTarget::Json => {
                self.target_absent(Construct::CastJson, "`JSON`, a type of its own", expr);
                "JSON".to_string()
            }
            CastTarget::Year => {
                self.target_absent(Construct::CastYear, "`YEAR`", expr);
                "YEAR".to_string()
            }
            CastTarget::Spatial(name) => {
                self.target_absent(Construct::CastSpatial, "the spatial types", expr);
                name.clone()
            }
            CastTarget::Interval(fraction) => {
                self.target_absent(Construct::CastInterval, "`INTERVAL DAY_SECOND(n)`", expr);
                format!("INTERVAL DAY_SECOND({fraction})")
            }
            CastTarget::Named(name) => {
                let construct = match name.as_str() {
                    "INET6" => Construct::Inet6Type,
                    "INET4" => Construct::Inet4Type,
                    _ => Construct::UuidType,
                };
                self.target_absent(construct, &format!("the type `{name}`"), expr);
                name.clone()
            }
        }
    }

    /// Text of a character string target: `NATIONAL CHAR` as `NCHAR`,
    /// `VARCHAR(n)` as `CHAR(n)` where the output lacks them; its
    /// `COLLATE`, when the output lacks it, written after the conversion
    /// (`moved_collation`).
    fn char_text(&mut self, target: &CharTarget, expr: &Expr) -> String {
        let word = match target.word {
            CharWord::Char => "CHAR",
            CharWord::Character => "CHARACTER",
            CharWord::NChar => "NCHAR",
            CharWord::NationalChar => {
                if self.has(Construct::CastNationalChar) {
                    "NATIONAL CHAR"
                } else {
                    self.target_rewrite("`NATIONAL CHAR`", "`NCHAR`", expr);
                    "NCHAR"
                }
            }
            CharWord::VarChar => {
                if self.has(Construct::CastVarchar) {
                    "VARCHAR"
                } else {
                    self.target_rewrite("`VARCHAR(n)`", "`CHAR(n)`", expr);
                    "CHAR"
                }
            }
        };
        let mut text = format!("{word}{}", length_text(target.length));
        if let Some(charset) = &target.charset {
            text.push(' ');
            text.push_str(&match charset {
                Charset::Named { short, .. } => {
                    let keyword = if *short { "CHARSET" } else { "CHARACTER SET" };
                    format!("{keyword} {}", self.charset(charset))
                }
                Charset::Ascii => "ASCII".to_string(),
                Charset::Unicode => "UNICODE".to_string(),
                Charset::Byte => "BYTE".to_string(),
            });
        }
        if target.binary {
            text.push_str(" BINARY");
        }
        if let Some(collation) = &target.collation
            && self.has(Construct::CastCollate)
        {
            text.push_str(&format!(" COLLATE {collation}"));
        }
        text
    }

    /// Collation of the target that the output writes after the
    /// conversion, `CAST(x AS CHAR) COLLATE c`.
    fn moved_collation(&mut self, target: &CastTarget, expr: &Expr) -> Option<String> {
        let CastTarget::Char(CharTarget {
            collation: Some(collation),
            ..
        }) = target
        else {
            return None;
        };
        if self.has(Construct::CastCollate) {
            return None;
        }
        let message = format!(
            "{} has no `COLLATE` in the target of `CAST`: written after it",
            self.target()
        );
        self.step("cast.target", StepKind::Rewrite, message, expr.span);
        Some(collation.clone())
    }

    /// Name of a character set, between quotes when written so.
    fn charset(&self, charset: &Charset) -> String {
        match charset {
            Charset::Named {
                name, quoted: true, ..
            } => self.string(name),
            Charset::Named { name, .. } => name.clone(),
            other => other.name(),
        }
    }

    /// Records the rewrite of the target `from`, which the output lacks,
    /// into `written`.
    fn target_rewrite(&mut self, from: &str, written: &str, expr: &Expr) {
        let message = format!(
            "{} has no `CAST` to {from}: written {written}, of the same values",
            self.target()
        );
        self.step("cast.target", StepKind::Rewrite, message, expr.span);
    }

    /// Error when the output lacks `construct` in its version.
    fn target_version(&mut self, construct: Construct, expr: &Expr) {
        if !self.has(construct) {
            let message = format!("{}: absent from {}", construct.label(), self.target());
            self.error("cast.target", &message, expr.span);
        }
    }

    /// Error when the output lacks `construct`, a target without equivalent,
    /// `what`.
    fn target_absent(&mut self, construct: Construct, what: &str, expr: &Expr) {
        if !self.has(construct) {
            let message = format!(
                "{} has no `CAST` to {what}, nor an equivalent",
                self.target()
            );
            self.error("cast.target", &message, expr.span);
        }
    }

    /// Errors of the values the servers convert differently, by the type of
    /// the operand and the target.
    fn cast_difference(&mut self, expr: &Expr, cast: &Cast) {
        let operand = &cast.operand;
        let data_type = &operand.ty.data_type;
        let reason = match &cast.target {
            CastTarget::Signed(_) | CastTarget::Integer(_) => integer_difference(operand, false),
            CastTarget::Unsigned(_) => {
                let decimal_differs = self.input_has(Construct::CastUnsignedWrap)
                    != self.has(Construct::CastUnsignedWrap);
                integer_difference(operand, true)
                    .or_else(|| unsigned_difference(operand, decimal_differs))
            }
            CastTarget::Float(bits) if bits.is_none_or(|bits| bits <= 24) => {
                float_difference(data_type)
            }
            CastTarget::Real { float: true } => float_difference(data_type),
            CastTarget::Char(char_target) => bytes_difference(data_type, char_target),
            CastTarget::Date => temporal_difference(operand, None),
            CastTarget::DateTime(fraction) | CastTarget::Time(fraction) => {
                temporal_difference(operand, Some(fraction.unwrap_or(0)))
            }
            _ => None,
        };
        if let Some((rule, message)) = reason {
            self.error(rule, &message, expr.span);
        }
    }
}

/// Difference of a conversion to an integer: the fractional seconds of a
/// date or a time, the line breaks before the integer of a string, an
/// unknown type.
fn integer_difference(operand: &Expr, unsigned: bool) -> Option<(&'static str, String)> {
    match &operand.ty.data_type {
        DataType::DateTime { fraction }
        | DataType::Timestamp { fraction }
        | DataType::Time { fraction }
            if *fraction > 0 =>
        {
            Some(fraction_reason(*fraction, 0))
        }
        DataType::Text(_) => match &operand.kind {
            ExprKind::Literal(Value::Text(text)) if !starts_with_line_break(text) => None,
            _ => Some((
                "cast.spaces",
                "`CAST` of a string to an integer: MySQL stops at a line break, a vertical tab, a form feed or a carriage return before the number (`CAST('\\n12' AS SIGNED)` gives 0), MariaDB skips them (12)".to_string(),
            )),
        },
        DataType::Unknown => Some(unknown_reason(if unsigned {
            "UNSIGNED"
        } else {
            "SIGNED"
        })),
        _ => None,
    }
}

/// Difference of a conversion to `UNSIGNED` of a negative or large number:
/// MySQL takes a float, a negative time and, from 8.0.19, a negative decimal
/// as the unsigned integer of the same bits, MariaDB gives 0; a float from
/// 2^63, 2^63 - 1 in MySQL, the unsigned integer in MariaDB.
/// `decimal_differs`: one dialect wraps the negative decimals, the other
/// gives 0.
fn unsigned_difference(operand: &Expr, decimal_differs: bool) -> Option<(&'static str, String)> {
    let negative_literal = |value: &Value| match value {
        Value::Decimal(decimal) => decimal.as_str().starts_with('-'),
        Value::Float(float) => *float < 0.0 || *float >= UNSIGNED_FLOAT_BOUND,
        _ => false,
    };
    let differs = match (&operand.ty.data_type, &operand.kind) {
        (DataType::Decimal(_) | DataType::Float(_), ExprKind::Literal(value)) => {
            negative_literal(value)
                && (decimal_differs || matches!(operand.ty.data_type, DataType::Float(_)))
        }
        (DataType::Decimal(_), _) => decimal_differs,
        (DataType::Float(_) | DataType::Time { .. }, _) => true,
        _ => false,
    };
    differs.then(|| {
        (
            "cast.unsigned",
            "`CAST` of a negative number to `UNSIGNED`: MySQL gives the unsigned integer of the same bits to a float, a time and, from 8.0.19, a decimal (`CAST(-3.7e0 AS UNSIGNED)` gives 18446744073709551612), MariaDB 0; a float from 2^63 gives 9223372036854775807 in MySQL, the integer in MariaDB".to_string(),
        )
    })
}

/// Difference of a conversion to a `FLOAT` of a value beyond its range: an
/// error in MySQL, the largest `FLOAT` in MariaDB.
fn float_difference(data_type: &DataType) -> Option<(&'static str, String)> {
    let within = match data_type {
        DataType::Decimal(decimal) => decimal.precision() - decimal.scale() <= FLOAT_DIGITS,
        DataType::Float(FloatType::Double) | DataType::Text(_) | DataType::Unknown => false,
        _ => true,
    };
    (!within).then(|| {
        (
            "cast.float",
            "`CAST` to `FLOAT` of a value beyond 3.4e38: an error in MySQL (`CAST(1e300 AS FLOAT)`, error 1690), the largest `FLOAT` in MariaDB (3.40282e38)".to_string(),
        )
    })
}

/// Difference of a conversion of bytes to characters, a byte string or a
/// value of unknown type, to a character set other than `latin1`: an
/// invalid sequence gives its bytes in MySQL until 8.0.27, NULL from 8.0.28,
/// a `?` in MariaDB.
fn bytes_difference(data_type: &DataType, target: &CharTarget) -> Option<(&'static str, String)> {
    let charset = target.charset.as_ref().map(Charset::name);
    let any_byte = matches!(charset.as_deref(), Some("latin1" | "binary"))
        || target.charset == Some(Charset::Ascii);
    let bytes = matches!(
        data_type,
        DataType::Binary(_)
            | DataType::Hybrid { .. }
            | DataType::Unknown
            | DataType::Other(_)
            | DataType::Geometry
    );
    (bytes && !any_byte).then(|| {
        (
            "cast.bytes",
            "`CAST` of bytes to characters: an invalid sequence gives its bytes in MySQL until 8.0.27, NULL from 8.0.28, `?` in MariaDB (`CAST(X'FF41' AS CHAR)`); the type of the operand is a byte string or is unknown (a function, a system variable)".to_string(),
        )
    })
}

/// Difference of a conversion to a date (`fraction` `None`), a date and
/// time or a time of `fraction` digits.
fn temporal_difference(operand: &Expr, fraction: Option<u8>) -> Option<(&'static str, String)> {
    let data_type = &operand.ty.data_type;
    match data_type {
        DataType::Null | DataType::Date => None,
        DataType::DateTime { fraction: digits }
        | DataType::Timestamp { fraction: digits }
        | DataType::Time { fraction: digits } => match fraction {
            Some(fraction) if *digits > fraction => Some(fraction_reason(*digits, fraction)),
            _ => None,
        },
        DataType::Text(_) => match &operand.kind {
            ExprKind::Literal(Value::Text(text)) if plain_temporal(text, fraction) => None,
            _ => Some((
                "cast.temporal",
                "`CAST` of a string to a date or a time: MySQL from 8.0.19 reads a time zone offset (`'2024-05-06 23:00:00+02:00'` gives `21:00:00` in UTC), MariaDB ignores it; MySQL rounds the fractional seconds, MariaDB truncates them; MySQL gives NULL to a zero date, and from 8.0.24 to a zero day or month, MariaDB keeps them (under the default SQL modes)".to_string(),
            )),
        },
        DataType::Unknown => Some(unknown_reason(match fraction {
            None => "DATE",
            Some(_) => "DATETIME` or `TIME",
        })),
        _ => Some((
            "cast.invalid",
            "`CAST` to a date or a time of a value that is not one: MySQL gives NULL to an invalid date (`CAST(-3.7 AS DATE)`), MariaDB a zero date (`0000-00-00`); MySQL gives NULL to a time out of range (`CAST(9002030 AS TIME)`), MariaDB its bound (`838:59:59`); MySQL rounds the fractional seconds, MariaDB truncates them".to_string(),
        )),
    }
}

/// Reason of the rounding of `digits` fractional digits to `fraction`.
fn fraction_reason(digits: u8, fraction: u8) -> (&'static str, String) {
    (
        "cast.fraction",
        format!(
            "`CAST` of a value of {digits} fractional digits to {fraction}: MySQL rounds the fractional seconds (`CAST(TIME '10:20:30.5' AS TIME)` gives `10:20:31`), MariaDB truncates them (`10:20:30`), under the default SQL modes"
        ),
    )
}

/// Reason of a conversion of an operand of unknown type to `target`.
fn unknown_reason(target: &str) -> (&'static str, String) {
    (
        "cast.unknown",
        format!(
            "`CAST` to `{target}` of a value whose type is unknown (a function, until the types of the function catalog, or a system variable): MySQL and MariaDB convert some types differently (a string, a fractional time, a negative number)"
        ),
    )
}

/// Tells whether `text` holds, after spaces and tabs, a line break, a
/// vertical tab, a form feed or a carriage return, where MySQL stops
/// reading an integer and MariaDB skips them.
fn starts_with_line_break(text: &str) -> bool {
    text.trim_start_matches([' ', '\t'])
        .starts_with(['\n', '\u{b}', '\u{c}', '\r'])
}

/// Tells whether `text` is a date (`YYYY-MM-DD`) or a date and time
/// (`YYYY-MM-DD HH:MM:SS[.f]`) that both servers read alike: a valid date
/// from the year 1000, without offset, of `fraction` fractional digits at
/// most (any for a date, `fraction` `None`).
pub(super) fn plain_temporal(text: &str, fraction: Option<u8>) -> bool {
    let (date, time) = match text.split_once(' ') {
        Some((date, time)) => (date, Some(time)),
        None => (text, None),
    };
    let numbers = |part: &str, widths: &[usize]| -> Option<Vec<u32>> {
        let fields: Vec<&str> = part.split([':', '-']).collect();
        let well_formed = fields.len() == widths.len()
            && fields.iter().zip(widths).all(|(field, width)| {
                field.len() == *width && field.bytes().all(|b| b.is_ascii_digit())
            });
        well_formed.then(|| {
            fields
                .iter()
                .map(|field| field.parse().unwrap_or(0))
                .collect()
        })
    };
    let Some(ymd) = numbers(date, &[4, 2, 2]) else {
        return false;
    };
    let (year, month, day) = (ymd[0], ymd[1], ymd[2]);
    if year < 1000 || !(1..=12).contains(&month) || day == 0 || day > days_in_month(year, month) {
        return false;
    }
    let Some(time) = time else {
        return true;
    };
    let (clock, digits) = match time.split_once('.') {
        Some((clock, digits)) => (clock, digits),
        None => (time, ""),
    };
    let fraction_ok = digits.bytes().all(|b| b.is_ascii_digit())
        && fraction.is_none_or(|fraction| digits.len() <= usize::from(fraction))
        && digits.len() <= 6
        && (digits.is_empty() == !time.contains('.'));
    let Some(hms) = numbers(clock, &[2, 2, 2]) else {
        return false;
    };
    fraction_ok && hms[0] < 24 && hms[1] < 60 && hms[2] < 60
}

/// Days of `month` in `year`, of the Gregorian calendar.
const fn days_in_month(year: u32, month: u32) -> u32 {
    match month {
        2 if year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400)) => {
            29
        }
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Tells whether `target` reads its operand as a number.
const fn numeric(target: &CastTarget) -> bool {
    matches!(
        target,
        CastTarget::Signed(_)
            | CastTarget::Unsigned(_)
            | CastTarget::Integer(_)
            | CastTarget::Decimal { .. }
            | CastTarget::Double { .. }
            | CastTarget::Float(_)
            | CastTarget::Real { .. }
    )
}

/// ` INTEGER` or ` INT` after `SIGNED` or `UNSIGNED`, when written.
const fn integer_word(word: Option<IntegerWord>) -> &'static str {
    match word {
        Some(IntegerWord::Integer) => " INTEGER",
        Some(IntegerWord::Int) => " INT",
        None => "",
    }
}

/// `(n)` of a target, when written.
fn length_text(length: Option<u32>) -> String {
    length
        .map(|length| format!("({length})"))
        .unwrap_or_default()
}
