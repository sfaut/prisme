//! National strings (`N'abc'`) and strings after a character set introducer
//! (`_latin1'abc'`, `_binary'abc'`) between versions and dialects (rule
//! `literal.introducer`).
//!
//! Both dialects read them alike, of the same character sets, `gb18030`
//! (MySQL) and `filename` (MariaDB) aside: an introducer of a character set
//! the output lacks is an error. An introduced string takes the default
//! collation of its character set, which differs for `utf8mb4` between the
//! dialects (`utf8mb4_0900_ai_ci` in MySQL, `utf8mb4_general_ci` in MariaDB,
//! `utf8mb4_uca1400_ai_ci` from MariaDB 11.4) and for the other Unicode
//! character sets from MariaDB 11.5 (`utf8mb3_uca1400_ai_ci`): two strings
//! of these collations compare otherwise (`_utf8mb4'a' = _utf8mb4'a '` gives
//! 0 in MySQL, 1 in MariaDB). An emulation by `COLLATE` would change the
//! coercibility of the string, and MariaDB 10.6 lacks `utf8mb4_0900_ai_ci`:
//! such a string, when the input and the output give it different
//! collations, is an error (**[decided autonomously]**). A national string
//! is of `utf8mb3_general_ci` everywhere, even where `_utf8mb3'abc'` is not.
//! Measured on MySQL 8.0.11, 8.4.11 and 9.3.0, MariaDB 10.5.29 to 11.8.9
//! (`crates/probe/cases/literals-collations.txt`).

use prisme_plan::Expr;
use prisme_types::DataType;

use super::Emitter;
use crate::charsets::{default_collation, is_charset};

impl Emitter<'_> {
    /// Records the error of the introduced string `expr` that the output
    /// lacks or gives another collation (rule `literal.introducer`).
    pub(super) fn introducer(&mut self, expr: &Expr) {
        let DataType::Text(text) = &expr.ty.data_type else {
            return;
        };
        let (Some(charset), None) = (&text.charset, &text.collation) else {
            return;
        };
        if !is_charset(self.output.dialect, charset) {
            let message = format!(
                "the character set `{charset}` of the introducer `_{charset}`: absent from {}",
                self.target()
            );
            self.error("literal.introducer", &message, expr.span);
            return;
        }
        let input = default_collation(charset, self.source.dialect, |construct| {
            self.input_has(construct)
        });
        let output = default_collation(charset, self.output.dialect, |construct| {
            self.has(construct)
        });
        if let (Some(input), Some(output)) = (input, output)
            && input != output
        {
            let message = format!(
                "{} gives a string after `_{charset}` the collation `{input}`, {} `{output}`, which compares otherwise",
                self.source_name(),
                self.target()
            );
            self.error("literal.introducer", &message, expr.span);
        }
    }

    /// Text of the national or introduced string `expr`, of `value`, for the
    /// SQL mode of the output: its introducer as written (`N`, `_latin1`),
    /// then the string.
    pub(super) fn introduced_string(&self, expr: &Expr, value: &str, charset: &str) -> String {
        let prefix = expr
            .span
            .and_then(|span| self.source.text.get(span.start..span.end))
            .and_then(|text| {
                text.find(['\'', '"'])
                    .map(|quote| text[..quote].to_string())
            })
            .unwrap_or_else(|| format!("_{charset}"));
        format!("{prefix}{}", self.string(value))
    }
}

/// Name the MySQL family gives to the byte string `text` written, of
/// `bytes`, without alias: its value for a string (`_binary'c'` is named
/// `c`), its text for a hexadecimal or bit literal (`_binary X'63'`).
pub(super) fn bytes_name(text: &str, bytes: &[u8]) -> String {
    let string = text.find(['\'', '"']).is_some_and(|quote| {
        let before = text[..quote].trim_end();
        let last = before
            .rsplit(|c: char| c.is_whitespace() || c == '/')
            .next();
        !matches!(last, Some("X" | "x" | "B" | "b"))
    });
    if string {
        String::from_utf8_lossy(bytes).into_owned()
    } else {
        text.to_string()
    }
}
