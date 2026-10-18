//! Hexadecimal and bit literals (`X'41'`, `0x41`, `b'1000001'`,
//! `0b1000001`) between versions and dialects (rule `literal.hexadecimal`),
//! measured on MySQL 8.0.11 to 8.0.28 and 8.4.11, MariaDB 10.5.29 to 11.8.9
//! (`crates/probe/cases/literals-hex.txt`, `literals-hybrid.txt`,
//! `literals-hex-long.txt`, `literals-hex-functions.txt`,
//! `literals-hex-contexts.txt`, `literals-hex-results.txt`).
//!
//! Both dialects read `0x41`, `b'1000001'` and `0b1000001` as hybrids: a
//! byte string read as a string, an integer read as a number (`0x41 = 'A'`
//! and `0x41 = 65` are both true). `X'41'` is a hybrid in MySQL, a byte
//! string in MariaDB (`X'41' + 0` gives 65 in MySQL, 0 in MariaDB): it is
//! written `0x41` towards MariaDB (`X''` is `b''`, an integer read as a
//! number, a `DOUBLE` written `X''` in MariaDB), `_binary X'41'` towards
//! MySQL. Read as a number, a hybrid differs:
//!
//! - beyond 8 bytes, when a byte before the last 8 is not zero: 2^64 - 1 in
//!   MySQL until 8.0.26, 0 from 8.0.27 (construct `hex_long_zero`), the last
//!   8 bytes in MariaDB;
//! - from 2^63 on 8 bytes, a bit literal: MySQL reads it as a signed integer
//!   (`b'1...1' + 0` gives -1), MariaDB as an unsigned one;
//! - after a unary minus and in `ABS`, `CEIL`, `FLOOR`, `ROUND` and
//!   `TRUNCATE`: a `DOUBLE` in MySQL, an integer in MariaDB
//!   (`ROUND(0x41, -1)` gives 60 in MySQL, 70 in MariaDB);
//! - in `CONV`, `BIN` and `OCT`: a number in MySQL, a string in MariaDB
//!   (`BIN(0x41)` gives `1000001`, `0` in MariaDB);
//! - as a result of `CASE`, `IF`, `IFNULL`, `COALESCE` and `NULLIF`: MySQL
//!   reads it as a number in `CASE`, `IF` and `NULLIF`, as a string in
//!   `IFNULL` and `COALESCE`; MariaDB as a string, as a number when all the
//!   results are hybrids from 10.11.19, 11.4.13 and 11.8.9 (construct
//!   `hybrid_result_number`; `IF(1, 0x41, 0x42) + 0` gives 65 in MySQL and
//!   MariaDB 11.8.9, 0 in MariaDB 11.8.8);
//! - in `GREATEST` and `LEAST`, compared with a number: as a string in
//!   MySQL, as a number in MariaDB (`greatest` module).
//!
//! Such a hybrid is an error, between dialects, between versions on either
//! side of a change; written `0x41 + 0`, it reads as an integer in both
//! dialects, written `_binary 0x41` as a byte string (**[decided
//! autonomously]**). A hybrid read as bytes stays: a column of the result,
//! an operand of a comparison with a string, of `LIKE`, an argument of
//! `CAST` to a string or of a string function (`reads_bytes`). A hybrid in
//! a derived table, a common table expression, `MIN` or `MAX` is a byte
//! string in both dialects (`DataType::stored`).

use prisme_plan::{Expr, ExprKind, FunctionCall};
use prisme_types::{DataType, TextType, Value};

use super::Emitter;
use crate::Dialect;
use crate::versions::Construct;

/// Functions that read a hybrid as a `DOUBLE` in MySQL, as an integer in
/// MariaDB.
const DOUBLE_IN_MYSQL: [&str; 6] = ["ABS", "CEIL", "CEILING", "FLOOR", "ROUND", "TRUNCATE"];

/// Functions that read a hybrid as a number in MySQL, as a string in
/// MariaDB.
const NUMBER_IN_MYSQL: [&str; 3] = ["BIN", "CONV", "OCT"];

impl Emitter<'_> {
    /// Records the errors of the hybrids of `expr` that the output reads
    /// otherwise, `expr` being read as bytes when `as_bytes`.
    pub(super) fn hybrids(&mut self, expr: &Expr, as_bytes: bool) {
        match &expr.kind {
            ExprKind::Literal(Value::Binary(bytes)) if is_hybrid(expr) => {
                if !as_bytes {
                    self.hybrid_number(expr, bytes);
                }
            }
            ExprKind::Negate(operand) => self.hybrid_negate(operand),
            ExprKind::Function(call) if !call.stored => self.hybrid_call(call, as_bytes),
            ExprKind::Case(case) if !as_bytes => {
                let results: Vec<&Expr> = case
                    .branches
                    .iter()
                    .map(|branch| &branch.then)
                    .chain(case.otherwise.as_deref())
                    .collect();
                self.hybrid_results("CASE", &results);
            }
            _ => {}
        }
    }

    /// Text of the hexadecimal or bit literal `expr`, of `bytes`, when the
    /// output reads its text otherwise: `X'41'` of MySQL, a hybrid, is
    /// `0x41` in MariaDB, `X''` is `b''`; `X'41'` of MariaDB, a byte
    /// string, is `_binary X'41'` in MySQL. `None` to write it as in the
    /// input.
    pub(super) fn bytes_text(&self, expr: &Expr, bytes: &[u8]) -> Option<String> {
        let text = expr
            .span
            .and_then(|span| self.source.text.get(span.start..span.end))?;
        let hex_string = text.starts_with(['X', 'x']) && text[1..].starts_with('\'');
        if !hex_string || self.source.dialect == self.output.dialect {
            return None;
        }
        match (&expr.ty.data_type, self.output.dialect) {
            (DataType::Hybrid { .. }, Dialect::MariaDb) => Some(bytes_canonical(expr, bytes)),
            (DataType::Binary(_), Dialect::MySql) => Some(format!("_binary X'{}'", digits(bytes))),
            _ => None,
        }
    }

    /// Records the error of the hybrid literal `expr`, of `bytes`, read as a
    /// number, when the output reads that number otherwise.
    fn hybrid_number(&mut self, expr: &Expr, bytes: &[u8]) {
        let across = self.source.dialect != self.output.dialect;
        let (input_zero, output_zero) = (
            self.input_has(Construct::HexLongZero),
            self.has(Construct::HexLongZero),
        );
        let long = bytes.len() > 8 && bytes[..bytes.len() - 8].iter().any(|byte| *byte != 0);
        let text = expr
            .span
            .and_then(|span| self.source.text.get(span.start..span.end))
            .unwrap_or_default();
        let bits = text.starts_with(['b', 'B']) || text.starts_with("0b");
        let message = if long && (across || input_zero != output_zero) {
            format!(
                "a hexadecimal or bit literal of more than 8 bytes read as a number: {} gives {}, {} {}; compared with a string or written `CAST(... AS CHAR)`, it is read as bytes alike",
                self.source_name(),
                long_reading(self.source.dialect, input_zero),
                self.target(),
                long_reading(self.output.dialect, output_zero),
            )
        } else if across && bits && bytes.len() == 8 && bytes[0] & 0x80 != 0 {
            "a bit literal from 2^63 read as a number: a signed integer in MySQL (`b'1...1' + 0`, of 64 bits, gives -1), an unsigned one in MariaDB".to_string()
        } else {
            return;
        };
        self.error("literal.hexadecimal", &message, expr.span);
    }

    /// Records the error of the hybrid `operand` of a unary minus.
    fn hybrid_negate(&mut self, operand: &Expr) {
        if self.source.dialect != self.output.dialect && is_hybrid(operand) {
            let message = "the opposite of a hexadecimal or bit literal: a `DOUBLE` in MySQL, an integer in MariaDB (`-0x20000000000001` gives -9007199254740992 in MySQL, -9007199254740993 in MariaDB); written `-(0x41 + 0)`, an integer in both";
            self.error("literal.hexadecimal", message, operand.span);
        }
    }

    /// Records the errors of the hybrid arguments of the built-in call
    /// `call` that the output reads otherwise, the call being read as bytes
    /// when `as_bytes`.
    fn hybrid_call(&mut self, call: &FunctionCall, as_bytes: bool) {
        let name = call.name.value.to_ascii_uppercase();
        let first = call.args.first().filter(|arg| is_hybrid(arg));
        if self.source.dialect != self.output.dialect
            && let Some(arg) = first
        {
            let message = if DOUBLE_IN_MYSQL.contains(&name.as_str()) {
                format!(
                    "`{name}` of a hexadecimal or bit literal: a `DOUBLE` in MySQL, an integer in MariaDB (`ROUND(0x41, -1)` gives 60 in MySQL, 70 in MariaDB); written `{name}(0x41 + 0)`, an integer in both"
                )
            } else if NUMBER_IN_MYSQL.contains(&name.as_str()) {
                format!(
                    "`{name}` of a hexadecimal or bit literal: read as a number in MySQL, as a string in MariaDB (`BIN(0x41)` gives `1000001`, `0` in MariaDB); written `{name}(0x41 + 0)` or `{name}(_binary 0x41)`, read alike"
                )
            } else {
                String::new()
            };
            if !message.is_empty() {
                self.error("literal.hexadecimal", &message, arg.span);
            }
        }
        if as_bytes {
            return;
        }
        let results: Vec<&Expr> = match name.as_str() {
            "IF" => call.args.iter().skip(1).collect(),
            "IFNULL" | "COALESCE" => call.args.iter().collect(),
            "NULLIF" => call.args.iter().take(1).collect(),
            _ => return,
        };
        self.hybrid_results(&name, &results);
    }

    /// Records the error of a hybrid among `results`, the results of `CASE`
    /// or of the conditional function `name`, read other than as bytes.
    fn hybrid_results(&mut self, name: &str, results: &[&Expr]) {
        let changed = self.source.dialect != self.output.dialect
            || self.input_has(Construct::HybridResultNumber)
                != self.has(Construct::HybridResultNumber);
        if !changed {
            return;
        }
        if let Some(result) = results.iter().find(|result| is_hybrid(result)) {
            let message = format!(
                "a hexadecimal or bit literal as a result of `{name}`, read as a number: MySQL reads it as a number in `CASE`, `IF` and `NULLIF`, as a string in `IFNULL` and `COALESCE`; MariaDB as a string, as a number when all the results are such literals from 10.11.19, 11.4.13 and 11.8.9 (`IF(1, 0x41, 0x42) + 0` gives 65 in MySQL and MariaDB 11.8.9, 0 in MariaDB 11.8.8); written `0x41 + 0`, a number, or `_binary 0x41`, a byte string, read alike"
            );
            self.error("literal.hexadecimal", &message, result.span);
        }
    }
}

/// Canonical text of the literal `expr` of `bytes`: a hybrid `0x41`
/// (`b''` without byte), bytes of a character set `_latin1 X'E9'`, a byte
/// string `_binary X'41'`, read alike in both dialects.
pub(super) fn bytes_canonical(expr: &Expr, bytes: &[u8]) -> String {
    match &expr.ty.data_type {
        DataType::Hybrid { .. } if bytes.is_empty() => "b''".to_string(),
        DataType::Hybrid { .. } => format!("0x{}", digits(bytes)),
        DataType::Text(TextType {
            charset: Some(charset),
            ..
        }) => format!("_{charset} X'{}'", digits(bytes)),
        _ => format!("_binary X'{}'", digits(bytes)),
    }
}

/// Tells whether `expr` is a hybrid: a hexadecimal or bit literal.
pub(super) fn is_hybrid(expr: &Expr) -> bool {
    matches!(expr.ty.data_type, DataType::Hybrid { .. })
}

/// Tells whether `expr` is read as a string, a hybrid included: a hybrid
/// compared with it is compared as bytes.
pub(super) fn reads_as_string(expr: &Expr) -> bool {
    matches!(
        expr.ty.data_type,
        DataType::Text(_) | DataType::Binary(_) | DataType::Hybrid { .. }
    )
}

/// Tells whether the built-in function `name`, in capitals, reads its
/// argument at `index` as a string, a hybrid as its bytes (measured with a
/// literal of 11 bytes, `crates/probe/cases/literals-hex-functions.txt`).
pub(super) fn reads_bytes(name: &str, index: usize) -> bool {
    match name {
        "ASCII" | "BIT_LENGTH" | "CHARSET" | "CHAR_LENGTH" | "CHARACTER_LENGTH" | "COLLATION"
        | "CONCAT" | "CONCAT_WS" | "CRC32" | "FIND_IN_SET" | "HEX" | "INSTR" | "LCASE"
        | "LENGTH" | "LOWER" | "MD5" | "OCTET_LENGTH" | "ORD" | "QUOTE" | "REPLACE" | "REVERSE"
        | "SHA" | "SHA1" | "SOUNDEX" | "STRCMP" | "TO_BASE64" | "UCASE" | "UNHEX" | "UPPER" => true,
        "LEFT" | "MID" | "REPEAT" | "RIGHT" | "SHA2" | "SUBSTR" | "SUBSTRING" => index == 0,
        "LPAD" | "RPAD" => index != 1,
        "INSERT" => index == 0 || index == 3,
        "LOCATE" | "SUBSTRING_INDEX" => index < 2,
        _ => false,
    }
}

/// Number a hexadecimal literal of more than 8 bytes reads as in `dialect`,
/// from 8.0.27 in MySQL when `zero`.
fn long_reading(dialect: Dialect, zero: bool) -> &'static str {
    match (dialect, zero) {
        (Dialect::MySql, false) => "2^64 - 1",
        (Dialect::MySql, true) => "0",
        (Dialect::MariaDb, _) => "its last 8 bytes",
    }
}

/// Hexadecimal digits of `bytes`, in capitals.
fn digits(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02X}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn string_functions_read_some_arguments_as_bytes() {
        assert!(reads_bytes("HEX", 0));
        assert!(reads_bytes("SUBSTRING", 0));
        assert!(!reads_bytes("SUBSTRING", 1));
        assert!(reads_bytes("LPAD", 2));
        assert!(!reads_bytes("LPAD", 1));
        assert!(!reads_bytes("ABS", 0));
    }
}
