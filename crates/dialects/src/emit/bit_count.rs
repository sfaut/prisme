//! `BIT_COUNT` between MySQL and MariaDB. Both count the bits of an
//! integer, of a date or a time without a fraction of a second read as a
//! number, of the index of an `ENUM`; they read the other values
//! differently:
//!
//! - a byte string: MySQL counts the bits of its bytes, MariaDB reads it
//!   as a number (`BIT_COUNT(X'FF0F')` gives 12, 0 in MariaDB);
//! - a character string: MySQL reads an integer, its fraction dropped,
//!   MariaDB a decimal, rounded (`BIT_COUNT('3.7')` gives 2, 1 in MariaDB).
//!   From 2^63, MySQL bounds the integer to 2^63 - 1 or takes it as
//!   unsigned according to the expression: a literal is taken as unsigned,
//!   a column of a table is bounded, a column of a view, of a derived table
//!   or of a common table expression is read as the expression it stands
//!   for, a string function is bounded before 8.4 and taken as unsigned
//!   from 8.4, `COALESCE` and `IFNULL` take it as unsigned;
//! - a number from 2^63: MySQL counts the bits of 2^63 - 1, MariaDB those
//!   of the number taken as unsigned, those of 2^64 - 1 from 2^64;
//! - a floating point number: MySQL rounds half to even, MariaDB half away
//!   from zero (`BIT_COUNT(2.5e0)` gives 1, 2 in MariaDB);
//! - a time with a fraction of a second: MySQL rounds the time to the
//!   second, MariaDB the number it reads, which differ from 59.5 seconds
//!   (`TIME'10:00:59.5'` reads 100100, 100060 in MariaDB);
//! - a `JSON`: MySQL reads its value, MariaDB its text, or a decimal for a
//!   function of JSON (`BIT_COUNT(JSON_EXTRACT('{"a": "3.7"}', '$.a'))`
//!   gives 2, 1 in MariaDB).
//!
//! Measured on MySQL 8.0.11 and 8.4.11, MariaDB 10.6.28 and 11.8.9; the
//! readings of a string by expression also on MySQL 8.0.17, 8.0.22, 8.0.28
//! and 9.3.0.

use prisme_plan::{Expr, ExprKind, FunctionCall};
use prisme_types::{DataType, Value};

use super::Emitter;
use super::rules::number;
use crate::Dialect;
use crate::builtins::Rule;
use crate::report::StepKind;

/// Largest number of integer digits of a decimal below 2^63 whatever its
/// value.
const SAFE_DIGITS: u8 = 18;

/// Bound below which a number literal is read alike, under 2^63.
const SAFE_LITERAL: f64 = 9e18;

/// What MariaDB does to a value before counting its bits, written in
/// MySQL around a decimal: rounded, bounded to `[-2^63, 2^64 - 1]`, then
/// taken as the unsigned integer of the same bits.
const MARIADB_BITS: (&str, &str) = (
    "CAST((GREATEST(LEAST(ROUND(",
    "), 18446744073709551615), -9223372036854775808) + 18446744073709551616) % 18446744073709551616 AS UNSIGNED)",
);

/// How a value is read before its bits are counted.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Reading {
    /// Alike in both dialects.
    Same,
    /// A character string.
    Text,
    /// A byte string.
    Binary,
    /// A number that may reach 2^63.
    Large,
    /// A floating point number.
    Float,
    /// A time, a date and time or a timestamp with a fraction of a second.
    Fraction,
    /// A `JSON`.
    Json,
    /// A type whose reading was not measured, or a type unknown.
    Other,
}

impl Reading {
    /// Reading of `arg`.
    fn of(arg: &Expr) -> Self {
        match &arg.ty.data_type {
            DataType::Null
            | DataType::Boolean
            | DataType::Integer(_)
            | DataType::Bit { .. }
            | DataType::Year
            | DataType::Enum
            | DataType::Set
            | DataType::Date => Self::Same,
            DataType::Time { fraction }
            | DataType::DateTime { fraction }
            | DataType::Timestamp { fraction } => {
                if *fraction == 0 {
                    Self::Same
                } else {
                    Self::Fraction
                }
            }
            DataType::Decimal(decimal) => {
                let small = match &arg.kind {
                    ExprKind::Literal(value) => {
                        number(value).is_some_and(|value| value.abs() < SAFE_LITERAL)
                    }
                    _ => decimal.precision() - decimal.scale() <= SAFE_DIGITS,
                };
                if small { Self::Same } else { Self::Large }
            }
            DataType::Text(_) => Self::Text,
            DataType::Binary(_) => Self::Binary,
            DataType::Float(_) => Self::Float,
            DataType::Json => Self::Json,
            _ => Self::Other,
        }
    }
}

impl Emitter<'_> {
    /// `BIT_COUNT(x)`: natively when both dialects read `x` alike, otherwise
    /// with `x` read as the input does, or an error.
    pub(super) fn bit_count(&mut self, rule: Rule, expr: &Expr, call: &FunctionCall) {
        // The server reports the number of arguments.
        let [arg] = call.args.as_slice() else {
            self.input_call(expr, call);
            return;
        };
        let reading = Reading::of(arg);
        let towards_mariadb = self.output.dialect == Dialect::MariaDb;
        let message = match reading {
            Reading::Same => {
                self.input_call(expr, call);
                return;
            }
            Reading::Float => {
                "`BIT_COUNT` of a floating point number: MySQL rounds it half to even, MariaDB half away from zero (`BIT_COUNT(2.5e0)` gives 1, 2 in MariaDB), and they bound it differently from 2^63"
            }
            Reading::Fraction => {
                "`BIT_COUNT` of a time with a fraction of a second: MySQL rounds the time to the second, MariaDB the number it reads (`BIT_COUNT(TIME'10:00:59.5')` gives 6, 9 in MariaDB)"
            }
            Reading::Json => {
                "`BIT_COUNT` of a `JSON`: MySQL reads its value, MariaDB its text, or a decimal for a function of JSON (`BIT_COUNT(JSON_EXTRACT('{\"a\": \"3.7\"}', '$.a'))` gives 2, 1 in MariaDB)"
            }
            Reading::Other if arg.ty.data_type == DataType::Unknown => {
                "`BIT_COUNT` of an argument whose type is unknown: MySQL and MariaDB read a string and a number from 2^63 differently"
            }
            Reading::Other => {
                "`BIT_COUNT` of a value of this type: its reading in MySQL and MariaDB is not established"
            }
            Reading::Text if towards_mariadb => match &arg.kind {
                ExprKind::Literal(Value::Text(text)) if !vertical_space_first(text) => "",
                ExprKind::Literal(_) => {
                    "`BIT_COUNT` of a string that starts with a line break or a vertical space: MySQL reads it as 0, MariaDB skips them"
                }
                _ => {
                    "`BIT_COUNT` of a string other than a literal: MySQL bounds the integer it reads to 2^63 - 1 or takes it as unsigned from 2^63 according to the expression (a column of a table, of a view or of a derived table, a function) and to the version"
                }
            },
            Reading::Text | Reading::Binary | Reading::Large => "",
        };
        if !message.is_empty() {
            self.error(rule.id(), message, expr.span);
            self.input_call(expr, call);
            return;
        }
        let context = match reading {
            Reading::Text => {
                "`BIT_COUNT` reads a string as an integer in MySQL, its fraction dropped, as a decimal in MariaDB, rounded (`BIT_COUNT('3.7')` gives 2, 1 in MariaDB)"
            }
            Reading::Binary => {
                "`BIT_COUNT` counts the bits of the bytes of a byte string in MySQL, reads it as a number in MariaDB (`BIT_COUNT(X'FF0F')` gives 12, 0 in MariaDB)"
            }
            _ => {
                "`BIT_COUNT` of a number from 2^63 counts the bits of 2^63 - 1 in MySQL, those of the number taken as unsigned in MariaDB"
            }
        };
        let how = match (reading, towards_mariadb) {
            (Reading::Text, true) => {
                "`CAST(x AS SIGNED)` reads a literal as MySQL does, taken as unsigned from 2^63"
            }
            (Reading::Binary, true) => "its bits are counted by `HEX` and `REPLACE`",
            (_, true) => "`LEAST(x, 9223372036854775807)` bounds it as MySQL does",
            (Reading::Large, false) => {
                "rounded, bounded and taken as an unsigned integer as MariaDB does"
            }
            (_, false) => {
                "read as a decimal, rounded, bounded and taken as an unsigned integer as MariaDB does"
            }
        };
        self.step(
            rule.id(),
            StepKind::Emulation,
            format!("{context}: {how}"),
            expr.span,
        );
        if towards_mariadb && reading == Reading::Binary {
            self.bit_count_of_bytes(arg);
            return;
        }
        let form = self.form(&call.name, expr.span);
        self.call_name(&call.name.value, form);
        match (reading, towards_mariadb) {
            (Reading::Text, true) => {
                self.write("CAST(");
                self.expr(arg, 0);
                self.write(" AS SIGNED)");
            }
            (_, true) => {
                self.write("LEAST(");
                self.expr(arg, 0);
                self.write(", 9223372036854775807)");
            }
            (reading, false) => {
                self.write(MARIADB_BITS.0);
                if reading == Reading::Large {
                    self.expr(arg, 0);
                } else {
                    self.write("CAST(");
                    self.expr(arg, 0);
                    self.write(" AS DECIMAL(65, 30))");
                }
                self.write(MARIADB_BITS.1);
            }
        }
        self.write(")");
    }

    /// Number of bits set in the bytes of `arg`, in MariaDB: each digit of
    /// `HEX(arg)` replaced by as many `x` as its bits, then counted.
    fn bit_count_of_bytes(&mut self, arg: &Expr) {
        self.write("LENGTH(");
        self.write(&"REPLACE(".repeat(16));
        self.write("HEX(");
        self.expr(arg, 0);
        self.write(")");
        for digit in 0..16_u32 {
            let bits = "x".repeat(digit.count_ones() as usize);
            self.write(&format!(", '{digit:X}', '{bits}')"));
        }
        self.write(")");
    }
}

/// Tells whether `text` starts with a line break or a vertical space after
/// its spaces and tabs: MySQL reads a literal as an integer after spaces
/// and tabs only, `CAST(x AS SIGNED)` of MariaDB after all of them.
fn vertical_space_first(text: &str) -> bool {
    text.trim_start_matches([' ', '\t'])
        .starts_with(['\n', '\u{b}', '\u{c}', '\r'])
}
