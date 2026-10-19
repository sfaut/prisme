//! `NULLIF` between MySQL and MariaDB (rule `function.nullif`), from their
//! sources (`docs/sources/mysql/nullif.md`, `docs/sources/mariadb/nullif.md`)
//! and measured on each local image (`crates/probe/cases/nullif.txt`,
//! `flow-types.txt`, `flow-direct.txt`). MariaDB gives the first argument,
//! of its type, as `CASE WHEN a = b THEN NULL ELSE a END` does. MySQL
//! differs twice:
//!
//! - its type (`build::flow::mysql_nullif`): a date or a time becomes a
//!   `VARCHAR` of the same text, which the expression around it reads as a
//!   string (`NULLIF(t, 1) + INTERVAL 1 DAY`, `YEAR(NULLIF(d, 1))`,
//!   `ROUND(NULLIF(dt, 1))`... differ, and so does the order of a time,
//!   `-10:00:00` after `-01:00:00`); a `YEAR` a byte string of its text
//!   read as an integer from 8.0.24, a `VARCHAR` from 8.1.0; before 8.0.24,
//!   a string, a date or a time compared with a byte string becomes a byte
//!   string;
//! - a constant compared with a column of `BIGINT`, `YEAR`, a date or a
//!   time is replaced by its value in the type of the column, which
//!   `NULLIF` gives: `NULLIF(1, t)` gives 16777216, the packed value of
//!   `00:00:01`, `NULLIF(24, y)` 2024; but where the column refuses it
//!   (`NULLIF(-1, y)`, `NULLIF(160, t)`, `NULLIF(24, d)`) and against a
//!   `BIGINT` an integer or a fraction (`NULLIF(1 + 1, n)`,
//!   `NULLIF(2.5, n)`), of the same value (`crates/probe/cases/nullif-convert.txt`).
//!
//! A date, a time or a `YEAR` is written `CAST(CAST(NULLIF(a, b) AS CHAR)
//! AS type)` towards MySQL, the type of MariaDB; towards MariaDB,
//! `CAST(NULLIF(a, b) AS CHAR)`, or `AS BINARY` for a byte string, the type
//! of MySQL, and a `YEAR` read as an integer `CAST(NULLIF(a, b) AS SIGNED)`
//! but as a value of its type, a byte string of its text there
//! (`AS BINARY`, MySQL 8.0 from 8.0.24). A string that MySQL gives as a byte
//! string is written `CAST(NULLIF(a, b) AS BINARY)` towards MariaDB, an
//! error towards MySQL, whose character set the transpiler does not know. A
//! first argument of a type unknown to Prisme may be a date or a time:
//! written as is in a column of the result, of the same text, an error
//! elsewhere and against a byte string before MySQL 8.0.24. A constant
//! compared with such a column is written `CASE WHEN a = b THEN NULL ELSE a
//! END` towards MySQL, an error towards MariaDB.

use prisme_plan::{Expr, ExprKind, FunctionCall};
use prisme_types::{DataType, IntegerSize, IntegerType, Value};

use super::Emitter;
use super::case::Reading;
use super::expr::PREDICATE;
use super::rules::repeatable;
use crate::Dialect;
use crate::build::flow;
use crate::builtins::Rule;
use crate::report::StepKind;
use crate::versions::{Construct, registry};

/// How the output gives `NULLIF` the type of the input.
enum Form {
    /// As written: both dialects give the same type.
    AsIs,
    /// A date, a time or a `YEAR` that MySQL gives as a string, towards
    /// MySQL: `CAST(CAST(NULLIF(a, b) AS CHAR) AS type)`.
    Typed(String),
    /// Towards MariaDB, the string or the byte string MySQL gives:
    /// `CAST(NULLIF(a, b) AS CHAR)` or `AS BINARY`.
    String(&'static str),
    /// Towards MariaDB, the integer MySQL reads a `YEAR` as:
    /// `CAST(NULLIF(a, b) AS SIGNED)`.
    Integer,
    /// An error, for this reason.
    Error(String),
}

impl Emitter<'_> {
    /// Writes the call `expr` of `NULLIF`, `call`, read by its parent as
    /// `reading` says, of the value and the type the input gives.
    pub(super) fn nullif(
        &mut self,
        rule: Rule,
        expr: &Expr,
        call: &FunctionCall,
        reading: Reading,
    ) {
        // The server reports the number of arguments.
        let [first, second] = call.args.as_slice() else {
            self.input_call(expr, call);
            return;
        };
        let target = self.target();
        let span = first.span.or(expr.span);
        if converted_constant(first, second) {
            match self.output.dialect {
                Dialect::MySql => {
                    let message = format!(
                        "{target} gives a constant compared with a column of `BIGINT`, `YEAR`, a date or a time by `NULLIF` in the type of the column (`NULLIF(1, t)` gives 16777216): `CASE` gives the constant"
                    );
                    self.step(rule.id(), StepKind::Emulation, message, span);
                    self.write("CASE WHEN ");
                    self.compared(first, " = ", second, PREDICATE, PREDICATE + 1);
                    self.write(" THEN NULL ELSE ");
                    self.expr(first, 0);
                    self.write(" END");
                }
                Dialect::MariaDb => {
                    let message = "MySQL gives a constant compared with a column of `BIGINT`, `YEAR`, a date or a time by `NULLIF` in the type of the column (`NULLIF(1, t)` gives 16777216, the packed value of `00:00:01`): MariaDB gives the constant";
                    self.error(rule.id(), message, span);
                    self.input_call(expr, call);
                }
            }
            return;
        }
        let (before, after, message) = match self.nullif_form(first, second, reading) {
            Form::AsIs => {
                self.input_call(expr, call);
                return;
            }
            Form::Error(message) => {
                self.error(rule.id(), &message, span);
                self.input_call(expr, call);
                return;
            }
            Form::Typed(target_type) => (
                "CAST(CAST(",
                format!(" AS CHAR) AS {target_type})"),
                format!(
                    "{target} gives a date, a time or a `YEAR` by `NULLIF` as a string: a `CAST` gives it the type of MariaDB"
                ),
            ),
            Form::String(string) => (
                "CAST(",
                format!(" AS {string})"),
                format!(
                    "MySQL gives a date, a time or a `YEAR` by `NULLIF` as a string, a string compared with a byte string as a byte string before 8.0.24: a `CAST` gives that string to {target}"
                ),
            ),
            Form::Integer => (
                "CAST(",
                " AS SIGNED)".to_string(),
                format!(
                    "MySQL gives a `YEAR` by `NULLIF` as a string read as an integer: a `CAST` gives that integer to {target}"
                ),
            ),
        };
        self.step(rule.id(), StepKind::Emulation, message, span);
        self.write(before);
        self.input_call(expr, call);
        self.write(&after);
    }

    /// How the output writes `NULLIF(first, second)`, read by its parent as
    /// `reading` says, a constant compared with a converted column aside.
    fn nullif_form(&self, first: &Expr, second: &Expr, reading: Reading) -> Form {
        let (towards_mysql, version) = match self.output.dialect {
            Dialect::MySql => (true, self.output.version),
            Dialect::MariaDb => (false, self.source.version),
        };
        let has = |construct| {
            registry(Dialect::MySql)
                .check(Dialect::MySql, construct, version)
                .is_ok()
        };
        let data_type = &first.ty.data_type;
        if *data_type == DataType::Unknown {
            let bytes = !has(Construct::NullifOwnCollation)
                && !matches!(second.kind, ExprKind::Literal(_))
                && matches!(
                    second.ty.data_type,
                    DataType::Binary(_) | DataType::Bit { .. }
                );
            if reading.result && !bytes {
                return Form::AsIs;
            }
            return Form::Error(
                "`NULLIF` of a first argument of a type unknown to Prisme: a date or a time gives a string in MySQL, a value of its type in MariaDB, a string compared with a byte string a byte string in MySQL before 8.0.24".to_string(),
            );
        }
        let mysql = flow::mysql_nullif(first, second, has);
        if mysql == *data_type {
            return Form::AsIs;
        }
        let temporal = matches!(
            data_type,
            DataType::Date
                | DataType::DateTime { .. }
                | DataType::Timestamp { .. }
                | DataType::Time { .. }
                | DataType::Year
        );
        match (towards_mysql, temporal, &mysql) {
            (true, true, _) => Form::Typed(temporal_target(data_type)),
            (true, false, _) => Form::Error(format!(
                "{} gives a string compared with a byte string by `NULLIF` as a byte string, of a character set unknown to Prisme",
                self.target()
            )),
            (false, _, DataType::Binary(_)) => Form::String("BINARY"),
            // A `YEAR` read as an integer, a byte string of its text as a
            // value of its type (the metadata of the results).
            (false, _, DataType::Integer(_)) if !reading.column => Form::Integer,
            (false, _, DataType::Integer(_)) => Form::String("BINARY"),
            (false, _, _) => Form::String("CHAR"),
        }
    }
}

/// Tells whether MySQL may replace `first`, a constant, by its value in the
/// type of `second`, a column of `BIGINT`, `YEAR`, a date or a time, in
/// `NULLIF(first, second)` (`convert_constant_item`): when the column
/// stores it without an error, and for a `BIGINT` keeps its value. A string
/// compared with a date stays a string; an integer stays as written against
/// a `BIGINT`, of the same value, and where the column refuses it (measured
/// on each local image, `crates/probe/cases/nullif-convert.txt`).
fn converted_constant(first: &Expr, second: &Expr) -> bool {
    let ExprKind::Column(_) = &second.kind else {
        return false;
    };
    if !constant(first) || !repeatable(first) || first.ty.data_type == DataType::Null {
        return false;
    }
    let integer = integer_constant(first);
    match &second.ty.data_type {
        DataType::Integer(IntegerType {
            size: IntegerSize::Big,
            ..
        }) => {
            !matches!(first.ty.data_type, DataType::Integer(_))
                && integer.is_none()
                && !fraction(first)
        }
        DataType::Year => {
            integer.is_none_or(|n| (0..=99).contains(&n) || (1901..=2155).contains(&n))
        }
        DataType::Time { .. } => integer.is_none_or(stored_as_time),
        DataType::Date | DataType::DateTime { .. } | DataType::Timestamp { .. } => {
            !matches!(first.ty.data_type, DataType::Text(_))
                && integer.is_none_or(|n| n == 0 || n >= 101)
        }
        _ => false,
    }
}

/// Value of `expr`, an integer, a boolean or their opposite as written.
fn integer_constant(expr: &Expr) -> Option<i128> {
    match &expr.kind {
        ExprKind::Literal(Value::Integer(value)) => Some(*value),
        ExprKind::Literal(Value::Boolean(value)) => Some(i128::from(*value)),
        ExprKind::Negate(operand) => integer_constant(operand).map(|value| -value),
        _ => None,
    }
}

/// Tells whether `expr` is a decimal or a floating point literal of a
/// fraction, which a `BIGINT` stores of another value: MySQL keeps it.
fn fraction(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Literal(Value::Decimal(decimal)) => (decimal.as_str())
            .split_once('.')
            .is_some_and(|(_, digits)| digits.bytes().any(|digit| digit != b'0')),
        ExprKind::Literal(Value::Float(value)) => value.fract() != 0.0,
        ExprKind::Negate(operand) => fraction(operand),
        _ => false,
    }
}

/// Tells whether a `TIME` column may store the integer `n` without an error
/// (`number_to_time`): of minutes and seconds below 60 up to 838:59:59, a
/// date and time from 10^10.
fn stored_as_time(n: i128) -> bool {
    const TIME_MAX_VALUE: i128 = 8_385_959;
    match n.abs() {
        _ if n >= 10_000_000_000 => true,
        absolute if absolute > TIME_MAX_VALUE => false,
        absolute => absolute % 100 < 60 && absolute / 100 % 100 < 60,
    }
}

/// Tells whether `expr` is a literal or an expression of literals, which
/// MySQL evaluates once.
fn constant(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Literal(_) => true,
        ExprKind::Negate(operand) | ExprKind::Not(operand) | ExprKind::BitNot(operand) => {
            constant(operand)
        }
        ExprKind::Cast(cast) => constant(&cast.operand),
        ExprKind::Arithmetic { left, right, .. } | ExprKind::Bitwise { left, right, .. } => {
            constant(left) && constant(right)
        }
        ExprKind::Function(call) => !call.stored && call.args.iter().all(constant),
        _ => false,
    }
}

/// Target of the `CAST` that gives `data_type`, a date, a time or a `YEAR`,
/// in MySQL: a timestamp is a date and time there.
fn temporal_target(data_type: &DataType) -> String {
    let (name, fraction) = match data_type {
        DataType::Date => return "DATE".to_string(),
        DataType::Year => return "YEAR".to_string(),
        DataType::Time { fraction } => ("TIME", *fraction),
        DataType::DateTime { fraction } | DataType::Timestamp { fraction } => {
            ("DATETIME", *fraction)
        }
        _ => unreachable!("a date, a time or a year"),
    };
    match fraction {
        0 => name.to_string(),
        _ => format!("{name}({fraction})"),
    }
}
