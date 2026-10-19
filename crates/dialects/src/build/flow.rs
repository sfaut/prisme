//! Functions of flow control: the type of their result, which holds the
//! values of the arguments it may give, as a `CASE` merges them
//! (`types::case`). Measured on each local image
//! (`crates/probe/cases/flow-types.txt`, each call in a subquery, whose
//! column MySQL widens to an `INT` or a `BIGINT`; `flow-direct.txt`, read
//! from the table of the measures, for the integers and the `YEAR` of
//! MySQL) and read in the sources (`docs/sources/mysql/flow.md`,
//! `docs/sources/mysql/nullif.md`, `docs/sources/mariadb/flow.md`):
//!
//! - `COALESCE`: of all its arguments, `NULL` when all may be;
//! - `IFNULL`, `NVL` of MariaDB: the same, but for integers in MySQL, of
//!   the largest type of its arguments, an integer other than a column
//!   being a `BIGINT` (`IFNULL(i, 0)` of an `INT` is a `BIGINT`,
//!   `COALESCE(i, 0)` an `INT`);
//! - `IF`, `NVL2` of MariaDB: of their second and third arguments, the type
//!   of the other one when MariaDB reads the `NULL` literal (`IF(c, u,
//!   NULL)` of an `INT UNSIGNED` keeps it, `CASE` gives a `DECIMAL`);
//! - `NULLIF`: of its first argument, always nullable. MySQL gives a date or
//!   a time as a `VARCHAR` of its text; a `YEAR` as an `INT` from 8.0.24,
//!   which a column of the result gives as a byte string of its text
//!   (construct `nullif_year_integer`), as a `VARCHAR` from 8.1.0
//!   (`nullif_year_string`); before 8.0.24, a string, a date or a time
//!   compared with a byte string, other than a literal, as a byte string
//!   (`nullif_own_collation`).
//!
//! MySQL 8.0.11 to 8.0.17 give `IFNULL` of a decimal one digit more or less
//! than `CASE`, a string of another length, of the same values: the type of
//! `CASE` is kept. `NULLIF` of a constant compared with a column of
//! `BIGINT`, `YEAR`, a date or a time, which MySQL converts to the type of
//! the column, keeps here the type of the constant.

use prisme_plan::{Expr, ExprKind};
use prisme_types::{BinaryType, DataType, IntegerSize, IntegerType, TextType, Type, Value};

use super::types;
use crate::Dialect;
use crate::versions::{Construct, Version, registry};

/// Length of the text of a date (`2024-05-06`).
const DATE_LENGTH: u32 = 10;
/// Length of the text of a date and time without fraction
/// (`2024-05-06 07:08:09`).
const DATETIME_LENGTH: u32 = 19;
/// Length of the text of a time without fraction, of its sign and its 838
/// hours (`-838:59:59`).
const TIME_LENGTH: u32 = 10;
/// Length of the text of a `YEAR` (`2024`).
const YEAR_LENGTH: u32 = 4;

/// Type of the built-in function `name`, in capitals, called with `args` in
/// `dialect` and its `version`; `None` for another function, or for a
/// number of arguments the servers refuse.
pub(super) fn flow_type(
    name: &str,
    args: &[Expr],
    dialect: Dialect,
    version: Version,
) -> Option<Type> {
    let mariadb = dialect == Dialect::MariaDb;
    // The arguments the call may give, and whether it gives `NULL` only
    // when all of them do.
    let (results, all_null) = match (name, args) {
        ("COALESCE", [_, ..]) => (args, true),
        ("IFNULL", [_, _]) | ("NVL", [_, _]) if name == "IFNULL" || mariadb => {
            if !mariadb
                && mysql_year_merge(args).is_none()
                && let Some(integer) = mysql_ifnull_integer(args)
            {
                let nullable = args.iter().all(|arg| arg.ty.nullable);
                return Some(Type {
                    data_type: integer,
                    nullable,
                });
            }
            (args, true)
        }
        ("IF", [_, second, third]) | ("NVL2", [_, second, third]) if name == "IF" || mariadb => {
            if mariadb {
                match (is_null(second), is_null(third)) {
                    (true, false) => return Some(Type::nullable(third.ty.data_type.clone())),
                    (false, true) => return Some(Type::nullable(second.ty.data_type.clone())),
                    _ => {}
                }
            }
            (&args[1..], false)
        }
        ("NULLIF", [first, second]) => {
            let data_type = match dialect {
                Dialect::MySql => {
                    let has =
                        |construct| registry(dialect).check(dialect, construct, version).is_ok();
                    mysql_nullif(first, second, has)
                }
                Dialect::MariaDb => first.ty.data_type.clone(),
            };
            return Some(Type::nullable(data_type));
        }
        _ => return None,
    };
    let data_type = match dialect {
        Dialect::MySql => mysql_year_merge(results),
        Dialect::MariaDb => None,
    }
    .unwrap_or_else(|| types::case(dialect, results.iter().map(|arg| &arg.ty.data_type)));
    let nullable = if all_null {
        results.iter().all(|arg| arg.ty.nullable)
    } else {
        results.iter().any(|arg| arg.ty.nullable)
    };
    Some(Type {
        data_type,
        nullable,
    })
}

/// Tells whether `expr` is the `NULL` literal.
fn is_null(expr: &Expr) -> bool {
    matches!(expr.kind, ExprKind::Literal(Value::Null))
}

/// Type MySQL from 8.0.14 to 8.0.16 gives the merge of `results` by `CASE`,
/// `COALESCE`, `IFNULL` or `IF` when `YEAR`, an unsigned `INT` of 4 digits
/// in those versions, meets a signed integer: a `DECIMAL` of the digits of
/// the largest (`CASE WHEN c THEN YEAR(d) ELSE 1 END` gives a
/// `DECIMAL(4, 0)`, an `INT UNSIGNED` column a `BIGINT`); `None` otherwise
/// (`crates/probe/cases/year.txt`, `year-merges.txt`).
pub(super) fn mysql_year_merge<'a>(
    results: impl IntoIterator<Item = &'a Expr>,
) -> Option<DataType> {
    let results: Vec<&Expr> = results.into_iter().collect();
    let unsigned_year = |expr: &Expr| {
        matches!(&expr.kind, ExprKind::Function(call)
            if !call.stored && call.name.value.eq_ignore_ascii_case("YEAR"))
            && matches!(
                expr.ty.data_type,
                DataType::Integer(IntegerType { unsigned: true, .. })
            )
    };
    let signed = |expr: &Expr| {
        matches!(
            expr.ty.data_type,
            DataType::Integer(IntegerType {
                unsigned: false,
                ..
            })
        )
    };
    let integers = results
        .iter()
        .all(|expr| matches!(expr.ty.data_type, DataType::Integer(_) | DataType::Null));
    if !integers
        || !results.iter().any(|expr| unsigned_year(expr))
        || !results.iter().any(|expr| signed(expr))
    {
        return None;
    }
    let digits = results
        .iter()
        .filter_map(|expr| match (&expr.kind, &expr.ty.data_type) {
            _ if unsigned_year(expr) => Some(YEAR_LENGTH as usize),
            (ExprKind::Literal(Value::Integer(value)), _) => {
                Some(value.unsigned_abs().to_string().len())
            }
            (_, DataType::Integer(integer)) => Some(types::integer_digits(*integer)),
            _ => None,
        })
        .max()
        .unwrap_or(YEAR_LENGTH as usize);
    Some(types::decimal(digits, 0))
}

/// Type MySQL gives `NULLIF(first, second)`, its constructs read by `has`
/// (`Item_func_nullif::resolve_type`): the type of `first`, a date or a time
/// becoming a string of its text, a byte string before 8.0.24 when
/// `second` is one.
pub(crate) fn mysql_nullif(
    first: &Expr,
    second: &Expr,
    has: impl Fn(Construct) -> bool,
) -> DataType {
    // The collation of the result aggregated with that of `second`, but for
    // a literal, whose collation yields.
    let bytes = !has(Construct::NullifOwnCollation)
        && !matches!(second.kind, ExprKind::Literal(_))
        && matches!(
            second.ty.data_type,
            DataType::Binary(_) | DataType::Bit { .. }
        );
    let fractional = |fraction: u8| match fraction {
        0 => 0,
        digits => u32::from(digits) + 1,
    };
    let length = match &first.ty.data_type {
        DataType::Date => DATE_LENGTH,
        DataType::DateTime { fraction } | DataType::Timestamp { fraction } => {
            DATETIME_LENGTH + fractional(*fraction)
        }
        DataType::Time { fraction } => TIME_LENGTH + fractional(*fraction),
        DataType::Year if has(Construct::NullifYearString) => YEAR_LENGTH,
        DataType::Year if has(Construct::NullifYearInteger) => {
            return DataType::Integer(IntegerType::signed(IntegerSize::Int));
        }
        DataType::Text(text) if bytes => {
            return DataType::Binary(BinaryType {
                length: None,
                fixed: text.fixed,
            });
        }
        data_type => return data_type.clone(),
    };
    if bytes {
        return DataType::Binary(BinaryType {
            length: Some(length),
            fixed: false,
        });
    }
    DataType::Text(TextType {
        length: Some(length),
        fixed: false,
        charset: None,
        collation: None,
    })
}

/// Integer type of `IFNULL` of `args` in MySQL, when they are integers or
/// `NULL`: the type of their merge (`Item::aggregate_type`), the largest
/// size, an integer other than a column being a `BIGINT`; signed with a
/// signed integer, one size larger when an unsigned one has that size, a
/// `DECIMAL(20, 0)` beyond a `BIGINT`.
fn mysql_ifnull_integer(args: &[Expr]) -> Option<DataType> {
    let mut integers = Vec::new();
    for arg in args {
        match &arg.ty.data_type {
            DataType::Null => {}
            DataType::Integer(integer) => integers.push(match arg.kind {
                ExprKind::Column(_) => *integer,
                _ => IntegerType {
                    size: IntegerSize::Big,
                    unsigned: integer.unsigned,
                },
            }),
            _ => return None,
        }
    }
    let size = integers
        .iter()
        .map(|integer| integer.size)
        .max_by_key(|size| size.bytes())?;
    let unsigned = integers.iter().all(|integer| integer.unsigned);
    let signed = integers.iter().all(|integer| !integer.unsigned);
    if unsigned || signed {
        return Some(DataType::Integer(IntegerType { size, unsigned }));
    }
    let bumped = integers
        .iter()
        .any(|integer| integer.unsigned && integer.size == size);
    let size = match (bumped, size) {
        (false, size) => size,
        (true, IntegerSize::Tiny) => IntegerSize::Small,
        (true, IntegerSize::Small) => IntegerSize::Medium,
        (true, IntegerSize::Medium) => IntegerSize::Int,
        (true, IntegerSize::Int) => IntegerSize::Big,
        (true, IntegerSize::Big) => return Some(types::decimal(20, 0)),
    };
    Some(DataType::Integer(IntegerType {
        size,
        unsigned: false,
    }))
}
