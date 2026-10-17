//! Functions of the current date and time: the type of their result, as
//! MySQL and MariaDB give it (`crates/probe/cases/now-types.txt`, measured
//! on MySQL 8.0.11 and 8.4.11, MariaDB 10.6.28 and 11.8.9), `NOT NULL`, of
//! the fractional digits of their argument, an integer literal from 0 to 6:
//!
//! - `NOW`, `CURRENT_TIMESTAMP`, `SYSDATE`: a `DATETIME`, a `TIMESTAMP` in
//!   MariaDB from 11.7.1 (construct `timestamp_now`);
//! - `LOCALTIME`: a `DATETIME`, a `TIME` in MariaDB from 11.6.2 (construct
//!   `time_localtime`);
//! - `LOCALTIMESTAMP`, `UTC_TIMESTAMP`: a `DATETIME`;
//! - `CURDATE`, `CURRENT_DATE`, `UTC_DATE`: a `DATE`;
//! - `CURTIME`, `CURRENT_TIME`, `UTC_TIME`: a `TIME`.

use prisme_plan::{Expr, ExprKind};
use prisme_types::{DataType, MAX_FRACTION_DIGITS, Type, Value};

use crate::Dialect;
use crate::versions::{Construct, Version, registry};

/// Type of the built-in function `name`, in capitals, called with `args` in
/// `dialect` and its `version`; `None` for another function, or for an
/// argument the servers refuse.
pub(super) fn now_type(
    name: &str,
    args: &[Expr],
    dialect: Dialect,
    version: Version,
) -> Option<Type> {
    let fraction = match args {
        [] => 0,
        [arg] => match &arg.kind {
            ExprKind::Literal(Value::Integer(digits)) => u8::try_from(*digits)
                .ok()
                .filter(|digits| *digits <= MAX_FRACTION_DIGITS)?,
            _ => return None,
        },
        _ => return None,
    };
    let since = |construct| {
        dialect == Dialect::MariaDb && registry(dialect).check(dialect, construct, version).is_ok()
    };
    let data_type = match name {
        "NOW" | "CURRENT_TIMESTAMP" | "SYSDATE" if since(Construct::TimestampNow) => {
            DataType::Timestamp { fraction }
        }
        "LOCALTIME" if since(Construct::TimeLocaltime) => DataType::Time { fraction },
        "NOW" | "CURRENT_TIMESTAMP" | "SYSDATE" | "LOCALTIME" | "LOCALTIMESTAMP"
        | "UTC_TIMESTAMP" => DataType::DateTime { fraction },
        "CURDATE" | "CURRENT_DATE" | "UTC_DATE" if args.is_empty() => DataType::Date,
        "CURTIME" | "CURRENT_TIME" | "UTC_TIME" => DataType::Time { fraction },
        _ => return None,
    };
    Some(Type::not_null(data_type))
}
