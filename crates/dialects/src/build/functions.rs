//! Minimal catalog of the aggregate and window functions of the MySQL
//! family: their role, the type of their result, and whether MySQL and
//! MariaDB write and compute them alike. The scalar functions wait for the
//! function catalog of step 7 (`docs/DEV-DIALECTS.md`): until then, they are
//! written as is towards their own dialect and refused towards another.
//!
//! The types were measured on MySQL 8.0.11 and 8.4, MariaDB 10.6 and 11.8
//! (`CREATE TABLE ... AS SELECT`, then `DESCRIBE`).

use prisme_plan::{Expr, OnEmpty};
use prisme_types::{DataType, IntegerSize, IntegerType, TextType, Type};

use super::types;
use crate::Dialect;

/// Window functions that are not aggregate functions.
const WINDOW_FUNCTIONS: [&str; 11] = [
    "CUME_DIST",
    "DENSE_RANK",
    "FIRST_VALUE",
    "LAG",
    "LAST_VALUE",
    "LEAD",
    "NTH_VALUE",
    "NTILE",
    "PERCENT_RANK",
    "RANK",
    "ROW_NUMBER",
];

/// Window functions MariaDB computes over an ordered window only.
pub(crate) const ORDERED_IN_MARIADB: [&str; 6] = [
    "CUME_DIST",
    "DENSE_RANK",
    "LAG",
    "LEAD",
    "PERCENT_RANK",
    "RANK",
];

/// Aggregate functions MySQL and MariaDB write and compute alike.
const SHARED_AGGREGATES: [&str; 16] = [
    "AVG",
    "BIT_AND",
    "BIT_OR",
    "BIT_XOR",
    "COUNT",
    "GROUP_CONCAT",
    "MAX",
    "MIN",
    "STD",
    "STDDEV",
    "STDDEV_POP",
    "STDDEV_SAMP",
    "SUM",
    "VARIANCE",
    "VAR_POP",
    "VAR_SAMP",
];

/// Tells whether `name`, in capitals, is a window function that is not an
/// aggregate function.
pub(crate) fn is_window_function(name: &str) -> bool {
    WINDOW_FUNCTIONS.contains(&name)
}

/// Tells whether MySQL and MariaDB write and compute the aggregate or
/// window function `name`, in capitals, alike.
pub(crate) fn is_shared(name: &str) -> bool {
    SHARED_AGGREGATES.contains(&name) || WINDOW_FUNCTIONS.contains(&name)
}

/// Text without length: `TEXT`, `MEDIUMTEXT`.
fn text() -> DataType {
    DataType::Text(TextType {
        length: None,
        fixed: false,
        charset: None,
        collation: None,
    })
}

/// Data type of the aggregate function `name`, in capitals, over `args` in
/// `dialect`, and its result over an empty set.
pub(super) fn aggregate(name: &str, args: &[Expr], dialect: Dialect) -> (DataType, OnEmpty) {
    let first = args
        .first()
        .map_or(DataType::Null, |arg| arg.ty.data_type.clone());
    match name {
        "COUNT" => (types::bigint(), OnEmpty::Value),
        "SUM" => (types::sum(&first), OnEmpty::Null),
        "AVG" => (types::avg(&first), OnEmpty::Null),
        "MIN" | "MAX" => (first, OnEmpty::Null),
        // Over an empty set, `BIT_AND` gives all bits set, the others 0.
        "BIT_AND" | "BIT_OR" | "BIT_XOR" => (
            DataType::Integer(IntegerType::unsigned(IntegerSize::Big)),
            OnEmpty::Value,
        ),
        "STD" | "STDDEV" | "STDDEV_POP" | "STDDEV_SAMP" | "VARIANCE" | "VAR_POP" | "VAR_SAMP" => {
            (types::double(), OnEmpty::Null)
        }
        "GROUP_CONCAT" => (text(), OnEmpty::Null),
        "JSON_ARRAYAGG" | "JSON_OBJECTAGG" => match dialect {
            Dialect::MySql => (DataType::Json, OnEmpty::Null),
            Dialect::MariaDb => (text(), OnEmpty::Null),
        },
        _ => (DataType::Unknown, OnEmpty::Null),
    }
}

/// Type of the window function `name`, in capitals, over `args` in
/// `dialect`, for a function that is not an aggregate.
pub(super) fn window(name: &str, args: &[Expr], dialect: Dialect) -> Type {
    let value = |index: usize| {
        args.get(index)
            .map_or(DataType::Null, |arg| arg.ty.data_type.clone())
    };
    match name {
        "ROW_NUMBER" | "RANK" | "DENSE_RANK" | "NTILE" => Type::not_null(types::rank(dialect)),
        "PERCENT_RANK" | "CUME_DIST" => Type {
            data_type: types::double(),
            nullable: dialect == Dialect::MariaDb,
        },
        "LAG" | "LEAD" => {
            // The default value, third argument, joins the type of the value.
            let merged = types::aggregate([&value(0), &value(2)]);
            Type::nullable(types::window_value(&merged, dialect))
        }
        "FIRST_VALUE" | "LAST_VALUE" | "NTH_VALUE" => {
            Type::nullable(types::window_value(&value(0), dialect))
        }
        _ => Type::nullable(DataType::Unknown),
    }
}
