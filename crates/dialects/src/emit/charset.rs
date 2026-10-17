//! Functions of a string whose other arguments may be binary strings, from
//! the character set they read them in (`crates/probe/cases/string-binary.txt`,
//! measured on MySQL 8.0.18, 8.0.19, 8.0.25, 8.0.26 and 8.4.11, MariaDB
//! 10.6.28 and 11.8.9):
//!
//! - `REPLACE`, `SUBSTRING_INDEX` and `TRIM` take the character set of their
//!   string in MySQL from 8.0.19 (construct `string_charset`): a binary
//!   argument with a character string gives a character string; before, as
//!   in MariaDB, a binary string of the same bytes;
//! - `LOCATE`, `INSTR` and `POSITION` search in the collation of their
//!   string in MySQL from 8.0.26 (construct `locate_charset`): a binary
//!   substring is searched after the collation of a character string, its
//!   position counted in characters; before, as in MariaDB, byte by byte,
//!   its position counted in bytes (`POSITION(CAST('B' AS BINARY) IN 'abc')`
//!   gives 0, 2 from 8.0.26).
//!
//! Towards the reading by the character set of the string, the string read
//! as binary (`CAST(s AS BINARY)`) gives the reading of the aggregated
//! character sets: a binary result, a search byte by byte. `LOCATE` from a
//! start, third argument, is an error: the aggregated reading counts the
//! start in characters of the string, then the bytes up to the substring
//! (`LOCATE(CAST('b' AS BINARY), 'ébéb', 3)` gives 5, 4 in MySQL 8.4, 3 of
//! the string read as binary). Towards the
//! aggregated character sets, the character set of the string is unknown in
//! the plan: an error, as for an argument of unknown type, which may be a
//! binary string (rule `function.binary-argument`).

use prisme_plan::{Expr, FunctionCall};
use prisme_types::DataType;

use super::Emitter;
use crate::builtins::Rule;
use crate::report::StepKind;
use crate::versions::Construct;

/// Kind of string of an argument.
#[derive(Clone, Copy, PartialEq, Eq)]
enum StringKind {
    /// A binary string: `BINARY`, `VARBINARY`, `BLOB`, `BIT`, a geometry.
    Binary,
    /// A character string, or a value read as one (a number, a date...).
    Character,
    /// A value of unknown type, which may be a binary string.
    Unknown,
}

impl StringKind {
    /// Kind of string of `arg`.
    fn of(arg: &Expr) -> Self {
        match &arg.ty.data_type {
            DataType::Binary(_) | DataType::Bit { .. } | DataType::Geometry => Self::Binary,
            DataType::Unknown | DataType::Other(_) => Self::Unknown,
            _ => Self::Character,
        }
    }
}

impl Emitter<'_> {
    /// `REPLACE`, `SUBSTRING_INDEX`, `LOCATE` and `INSTR`: natively, or with
    /// their string read as binary when the output reads it in its own
    /// character set and an argument is a binary string, or an error. `TRIM`
    /// and `POSITION` are nodes of their own (`string` module).
    pub(super) fn binary_argument(&mut self, rule: Rule, expr: &Expr, call: &FunctionCall) {
        let upper = call.name.value.to_ascii_uppercase();
        let (construct, subject, others): (Construct, usize, &[usize]) = match upper.as_str() {
            "REPLACE" => (Construct::StringCharset, 0, &[1, 2]),
            "SUBSTRING_INDEX" => (Construct::StringCharset, 0, &[1]),
            "LOCATE" => (Construct::LocateCharset, 1, &[0]),
            "INSTR" => (Construct::LocateCharset, 0, &[1]),
            _ => {
                self.rule_not_written(rule, &call.name, expr.span);
                self.input_call(expr, call);
                return;
            }
        };
        // The server reports the number of arguments.
        let Some(string) = call.args.get(subject) else {
            self.input_call(expr, call);
            return;
        };
        let others: Vec<&Expr> = others
            .iter()
            .filter_map(|index| call.args.get(*index))
            .collect();
        let start = upper == "LOCATE" && call.args.len() == 3;
        if !self.string_as_binary(&upper, construct, string, &others, start, expr) {
            self.input_call(expr, call);
            return;
        }
        let form = self.form(&call.name, expr.span);
        self.call_name(&call.name.value, form);
        for (index, arg) in call.args.iter().enumerate() {
            if index > 0 {
                self.write(", ");
            }
            if index == subject {
                self.binary_string(arg);
            } else {
                self.expr(arg, 0);
            }
        }
        self.write(")");
    }

    /// Tells whether `function`, of `string` and of the arguments `others`,
    /// read after `construct`, from a start when `start`, writes its string
    /// as binary; records the emulation, or the error of an argument read
    /// otherwise.
    pub(super) fn string_as_binary(
        &mut self,
        function: &str,
        construct: Construct,
        string: &Expr,
        others: &[&Expr],
        start: bool,
        expr: &Expr,
    ) -> bool {
        let by_string = self.has(construct);
        if self.input_has(construct) == by_string || StringKind::of(string) == StringKind::Binary {
            return false;
        }
        let kinds: Vec<StringKind> = others.iter().map(|arg| StringKind::of(arg)).collect();
        let binary = kinds.contains(&StringKind::Binary);
        let unknown = kinds.contains(&StringKind::Unknown);
        if !binary && !unknown {
            return false;
        }
        let input = format!("{} {}", self.source.dialect.label(), self.source.version);
        let target = self.target();
        let (by_string_reading, aggregated_reading) = match construct {
            Construct::LocateCharset => (
                "searches a binary substring after the collation of a character string, its position counted in characters",
                "byte by byte, its position counted in bytes",
            ),
            _ => (
                "gives a character string when an argument is binary",
                "a binary string",
            ),
        };
        let (reads_by_string, reads_aggregated) = if by_string {
            (&target, &input)
        } else {
            (&input, &target)
        };
        let readings = format!(
            "{reads_by_string} {by_string_reading}, {reads_aggregated} {aggregated_reading}"
        );
        if unknown {
            let message = format!(
                "`{function}` with an argument of unknown type, which may be a binary string: {readings}"
            );
            self.error(Rule::BinaryArgument.id(), &message, expr.span);
            return false;
        }
        if !by_string {
            let message = format!(
                "`{function}` with a binary argument: {readings}; the character set of its string is unknown"
            );
            self.error(Rule::BinaryArgument.id(), &message, expr.span);
            return false;
        }
        if start {
            let message = format!(
                "`{function}` of a binary substring from a start: {readings}; {input} counts the start in characters of the string, then the bytes up to the substring"
            );
            self.error(Rule::BinaryArgument.id(), &message, expr.span);
            return false;
        }
        let message = format!(
            "`{function}` with a binary argument: {readings}; its string is read as binary"
        );
        self.step(
            Rule::BinaryArgument.id(),
            StepKind::Emulation,
            message,
            expr.span,
        );
        true
    }

    /// `CAST(arg AS BINARY)`.
    pub(super) fn binary_string(&mut self, arg: &Expr) {
        self.write("CAST(");
        self.expr(arg, 0);
        self.write(" AS BINARY)");
    }
}
