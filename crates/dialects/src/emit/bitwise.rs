//! Bit operators between MySQL and MariaDB: `|`, `&`, `^`, `<<`, `>>` and
//! `~` (`crates/probe/cases/bitwise-readings.sql`, `bitwise-binary.sql`,
//! `bitwise-numbers.txt` and `bitwise-shifts.txt`, measured on MySQL 8.0.11
//! and 8.4.11, MariaDB 10.6.28 and 11.8.9).
//!
//! An operator reads each operand as an unsigned integer as `BIT_COUNT`
//! reads its argument, on each server and for each expression (a literal, a
//! column of a table, of a derived table, of a view, a function): an operand
//! that the dialects read otherwise, a string, a number from 2^63, a
//! floating point number, a time with a fraction of a second, a `JSON`, is
//! written read as the input does by the emulations of `BIT_COUNT`
//! (`bit_count` module), or is an error (rule `bitwise.operand`).
//!
//! The count of a shift is read alike in both dialects, a string as its
//! integer prefix, a decimal and a floating point number rounded, then
//! MySQL shifts out every bit from 64 and MariaDB takes the count modulo
//! 2^32 (`1 << 4294967297` gives 0, 2 in MariaDB). A count that stays in
//! `[-2^31, 2^32)` (an integer of at most 4 bytes, a small decimal, a
//! literal) shifts alike; a `BIGINT` is written bounded towards MariaDB,
//! `GREATEST(LEAST(n, 64), -1)`, taken modulo 2^32 towards MySQL,
//! `n & 4294967295`; another value is an error (rule `bitwise.shift`).
//!
//! MySQL computes byte by byte when every operand is a byte string, the left
//! one of a shift (`CAST('1' AS BINARY) | CAST('2' AS BINARY)` gives the
//! byte string `0x33`, 3 in MariaDB): towards MariaDB, an error (rule
//! `bitwise.binary`); towards MySQL, each byte string is read as a number as
//! MariaDB does, which MySQL then computes as numbers.

use prisme_plan::{Bitwise, Expr, ExprKind};
use prisme_types::{DataType, IntegerSize, IntegerType, Value};

use super::Emitter;
use super::bit_count::{Reading, Refusal, vertical_space_first};
use super::rules::number;
use crate::Dialect;
use crate::report::StepKind;

/// Range of the counts of a shift that MySQL and MariaDB shift alike: from
/// it, MariaDB takes the count modulo 2^32, which gives a count below 64.
const ALIKE_COUNTS: std::ops::Range<f64> = -2_147_483_648.0..4_294_967_296.0;

impl Emitter<'_> {
    /// `a op b` of the operation `expr`, its operands at levels `left` and
    /// `right` at least, each read as the input does.
    pub(super) fn bitwise(
        &mut self,
        expr: &Expr,
        op: Bitwise,
        operands: (&Expr, &Expr),
        levels: (u8, u8),
    ) {
        let (a, b) = operands;
        let symbol = match op {
            Bitwise::And => "&",
            Bitwise::Or => "|",
            Bitwise::Xor => "^",
            Bitwise::ShiftLeft => "<<",
            Bitwise::ShiftRight => ">>",
        };
        let shift = matches!(op, Bitwise::ShiftLeft | Bitwise::ShiftRight);
        let bytes = byte_string(a) && (shift || byte_string(b));
        if self.byte_by_byte(expr, symbol, bytes) {
            self.number_expr(a, levels.0);
            self.write(&format!(" {symbol} "));
            self.number_expr(b, levels.1);
            return;
        }
        self.bit_operand(a, levels.0, symbol);
        self.write(&format!(" {symbol} "));
        if shift {
            self.shift_count(b, levels.1, symbol);
        } else {
            self.bit_operand(b, levels.1, symbol);
        }
    }

    /// `~operand` of the inversion `expr`, its operand at level `min` at
    /// least, read as the input does.
    pub(super) fn bit_not(&mut self, expr: &Expr, operand: &Expr, min: u8) {
        let byte_by_byte = self.byte_by_byte(expr, "~", byte_string(operand));
        self.write("~");
        if byte_by_byte {
            self.number_expr(operand, min);
        } else {
            self.bit_operand(operand, min, "~");
        }
    }

    /// Records the error of the operation `expr` of the operator `symbol`
    /// that MySQL computes byte by byte, its operands all byte strings
    /// (`bytes`), towards MariaDB; tells whether it is recorded.
    fn byte_by_byte(&mut self, expr: &Expr, symbol: &str, bytes: bool) -> bool {
        let towards_mariadb =
            self.source.dialect == Dialect::MySql && self.output.dialect == Dialect::MariaDb;
        if !(bytes && towards_mariadb) {
            return false;
        }
        let message = format!(
            "`{symbol}` of byte strings: MySQL computes them byte by byte, MariaDB reads them as numbers (`CAST('1' AS BINARY) | CAST('2' AS BINARY)` gives the byte string `0x33`, 3 in MariaDB)"
        );
        self.error("bitwise.binary", &message, expr.span);
        true
    }

    /// Writes `operand` of the bit operator `symbol`, read as a number at
    /// level `min` at least, so that the output reads it as an unsigned
    /// integer as the input does, or records the error.
    fn bit_operand(&mut self, operand: &Expr, min: u8, symbol: &str) {
        let reading = Reading::of(operand);
        if reading == Reading::Same || self.source.dialect == self.output.dialect {
            self.number_expr(operand, min);
            return;
        }
        let towards_mariadb = self.output.dialect == Dialect::MariaDb;
        if let Some(refusal) = reading.refusal(operand, towards_mariadb) {
            let message = match refusal {
                Refusal::Float => format!(
                    "`{symbol}` of a floating point number: MySQL rounds it half to even, MariaDB half away from zero (`2.5e0 | 0` gives 2, 3 in MariaDB), and they bound it differently from 2^63"
                ),
                Refusal::Fraction => format!(
                    "`{symbol}` of a time with a fraction of a second: MySQL rounds the time to the second, MariaDB the number it reads (`TIME'10:00:59.5' | 0` gives 100100, 100060 in MariaDB)"
                ),
                Refusal::Json => format!(
                    "`{symbol}` of a `JSON`: MySQL reads its value, MariaDB its text (`j | 0` of a column holding the JSON string `\"3.7\"` gives 3, 0 in MariaDB)"
                ),
                Refusal::Unknown => format!(
                    "`{symbol}` of an operand whose type is unknown: MySQL and MariaDB read a string and a number from 2^63 differently, and MySQL computes byte strings byte by byte"
                ),
                Refusal::Unmeasured => format!(
                    "`{symbol}` of a value of this type: its reading in MySQL and MariaDB is not established"
                ),
                Refusal::VerticalSpace => format!(
                    "`{symbol}` of a string that starts with a line break or a vertical space: MySQL reads it as 0, MariaDB skips them"
                ),
                Refusal::NotLiteral => format!(
                    "`{symbol}` of a string other than a literal: MySQL bounds the integer it reads to 2^63 - 1 or takes it as unsigned from 2^63 according to the expression (a column of a table, of a view or of a derived table, a function) and to the version"
                ),
            };
            self.error("bitwise.operand", &message, operand.span);
            self.number_expr(operand, min);
            return;
        }
        let context = match reading {
            Reading::Text => format!(
                "`{symbol}` reads a string as an integer in MySQL, its fraction dropped, as a decimal in MariaDB, rounded (`'3.7' | 0` gives 3, 4 in MariaDB)"
            ),
            Reading::Binary => format!(
                "`{symbol}` reads a byte string as a decimal in MariaDB, rounded, as an integer in MySQL, its fraction dropped, or byte by byte with byte strings only"
            ),
            _ => format!(
                "`{symbol}` of a number from 2^63: MySQL bounds it to 2^63 - 1, MariaDB takes it as unsigned, 2^64 - 1 from 2^64"
            ),
        };
        let message = format!("{context}: {}", reading.emulation(towards_mariadb));
        self.step(
            "bitwise.operand",
            StepKind::Emulation,
            message,
            operand.span,
        );
        self.input_bits(reading, operand);
    }

    /// Writes `count`, the count of the shift `symbol`, read as a number at
    /// level `min` at least, so that the output shifts as the input does,
    /// or records the error.
    fn shift_count(&mut self, count: &Expr, min: u8, symbol: &str) {
        let reading = Count::of(count);
        if reading == Count::Same || self.source.dialect == self.output.dialect {
            self.number_expr(count, min);
            return;
        }
        let context = format!(
            "the count of `{symbol}`: MySQL shifts out every bit from 64, MariaDB takes the count modulo 2^32 (`1 {symbol} 4294967297` gives 0, {} in MariaDB)",
            if symbol == "<<" { 2 } else { 0 }
        );
        match reading {
            Count::Big if self.output.dialect == Dialect::MariaDb => {
                let message =
                    format!("{context}: `GREATEST(LEAST(n, 64), -1)` bounds it as MySQL reads it");
                self.step("bitwise.shift", StepKind::Emulation, message, count.span);
                self.write("GREATEST(LEAST(");
                self.number_expr(count, 0);
                self.write(", 64), -1)");
            }
            Count::Big => {
                let message =
                    format!("{context}: `n & 4294967295` takes it modulo 2^32 as MariaDB does");
                self.step("bitwise.shift", StepKind::Emulation, message, count.span);
                self.write("(");
                self.number_expr(count, 7);
                self.write(" & 4294967295)");
            }
            Count::VerticalSpace => {
                let message = format!(
                    "the count of `{symbol}` of a string that starts with a line break or a vertical space: MySQL reads it as 0, MariaDB skips them"
                );
                self.error("bitwise.shift", &message, count.span);
                self.number_expr(count, min);
            }
            _ => {
                let message = format!("{context}, for a value that may reach 2^32");
                self.error("bitwise.shift", &message, count.span);
                self.number_expr(count, min);
            }
        }
    }
}

/// How the count of a shift is read, against the counts that both dialects
/// shift alike.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Count {
    /// In `[-2^31, 2^32)`, shifted alike.
    Same,
    /// A `BIGINT`, written bounded or taken modulo 2^32.
    Big,
    /// A string literal that starts with a line break or a vertical space.
    VerticalSpace,
    /// Another value, which may reach 2^32.
    Other,
}

impl Count {
    /// Reading of `count`.
    fn of(count: &Expr) -> Self {
        match &count.kind {
            ExprKind::Literal(Value::Text(text)) if vertical_space_first(text) => {
                return Self::VerticalSpace;
            }
            ExprKind::Literal(Value::Text(text)) => {
                return if ALIKE_COUNTS.contains(&integer_prefix(text)) {
                    Self::Same
                } else {
                    Self::Other
                };
            }
            // Beyond, a literal is read as a value of its type.
            ExprKind::Literal(value)
                if number(value).is_some_and(|value| ALIKE_COUNTS.contains(&value)) =>
            {
                return Self::Same;
            }
            ExprKind::Negate(operand) => {
                if let ExprKind::Literal(value) = &operand.kind
                    && number(value).is_some_and(|value| ALIKE_COUNTS.contains(&-value))
                {
                    return Self::Same;
                }
            }
            _ => {}
        }
        match &count.ty.data_type {
            DataType::Null
            | DataType::Boolean
            | DataType::Year
            | DataType::Enum
            | DataType::Date
            | DataType::Time { fraction: 0 } => Self::Same,
            DataType::Integer(IntegerType {
                size: IntegerSize::Big,
                ..
            }) => Self::Big,
            DataType::Integer(_) => Self::Same,
            DataType::Bit { width } if *width <= 32 => Self::Same,
            DataType::Decimal(decimal) if decimal.precision() - decimal.scale() <= 9 => Self::Same,
            _ => Self::Other,
        }
    }
}

/// Integer prefix of `text`, as both dialects read the count of a shift:
/// after spaces and tabs, a sign and digits (`'1e1'` reads 1, `'3abc'` 3).
fn integer_prefix(text: &str) -> f64 {
    let text = text.trim_start_matches([' ', '\t']);
    let (negative, digits) = match text.as_bytes().first() {
        Some(b'-') => (true, &text[1..]),
        Some(b'+') => (false, &text[1..]),
        _ => (false, text),
    };
    let value = digits
        .bytes()
        .take_while(u8::is_ascii_digit)
        .fold(0.0, |value, digit| value * 10.0 + f64::from(digit - b'0'));
    if negative { -value } else { value }
}

/// Tells whether `operand` is a byte string, which MySQL computes byte by
/// byte with other byte strings.
fn byte_string(operand: &Expr) -> bool {
    matches!(operand.ty.data_type, DataType::Binary(_))
}
