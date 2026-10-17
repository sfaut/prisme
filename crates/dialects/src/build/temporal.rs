//! Typed literals, of the type the servers give them
//! (`crates/probe/cases/typed-literals*.txt`): a `DATE`, a `TIME` or a
//! `DATETIME` of the fractional digits written, 6 at most, nullable in
//! MariaDB for a zero month or day (`DATE '2024-05-00'` is `date NULL`
//! there, `date NOT NULL` in MySQL).

use prisme_plan::Expr;
use prisme_types::{DataType, Type, Value};

use crate::Dialect;
use crate::sql::ast::TemporalKind;
use crate::temporal::{self, Reading};

/// Typed literal of `kind` and string `value`, read in `dialect`.
pub(super) fn temporal_literal(kind: TemporalKind, value: &str, dialect: Dialect) -> Expr {
    let (digits, nullable) = match temporal::read(kind, value, dialect) {
        Reading::Valid(read) => (read.digits(), dialect == Dialect::MariaDb && read.zero()),
        Reading::Invalid | Reading::Unknown => (written_digits(value), false),
    };
    let (value, data_type) = match kind {
        TemporalKind::Date => (Value::Date(value.to_string()), DataType::Date),
        TemporalKind::Time => (
            Value::Time(value.to_string()),
            DataType::Time { fraction: digits },
        ),
        TemporalKind::Timestamp => (
            Value::DateTime(value.to_string()),
            DataType::DateTime { fraction: digits },
        ),
    };
    Expr::literal_typed(
        value,
        Type {
            data_type,
            nullable,
        },
    )
}

/// Fractional digits after the last `.` of a string the dialect does not
/// read as a value, 6 at most.
pub(super) fn written_digits(value: &str) -> u8 {
    let digits = value.rsplit_once('.').map_or(0, |(_, fraction)| {
        fraction.bytes().take_while(u8::is_ascii_digit).count()
    });
    u8::try_from(digits.min(6)).expect("6 digits at most")
}
