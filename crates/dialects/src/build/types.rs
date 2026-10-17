//! Types the MySQL family gives to literals and operations, for the plan
//! builder.
//!
//! The rules follow the reference manuals of MySQL and MariaDB (sections
//! "Type Conversion in Expression Evaluation", "Precision Math", "Arithmetic
//! Operators"): integers give `BIGINT`, unsigned when an operand is;
//! decimals keep their digits; a floating point number or a string makes the
//! operation a `DOUBLE`; a division gives a `DECIMAL` with 4 more digits of
//! scale (`div_precision_increment` at its default value, a setting of the
//! server the transpiler leaves to the target). A division or a remainder by
//! zero gives NULL.

use prisme_plan::Arithmetic;
use prisme_types::{
    DataType, DecimalType, FloatType, IntegerSize, IntegerType, MAX_DECIMAL_PRECISION, TextType,
    Type,
};

use crate::Dialect;

/// Largest scale of a `DECIMAL` in the MySQL family.
const MAX_SCALE: u8 = 30;

/// Digits added to the scale of a division (`div_precision_increment`).
const DIVISION_INCREMENT: u8 = 4;

/// `BIGINT`, signed.
pub(crate) const fn bigint() -> DataType {
    DataType::Integer(IntegerType::signed(IntegerSize::Big))
}

/// `DOUBLE`.
pub(crate) const fn double() -> DataType {
    DataType::Float(FloatType::Double)
}

/// Integer type of a boolean used as a number: `BIGINT` in MySQL, `INT` in
/// MariaDB.
pub(crate) const fn boolean_integer(dialect: Dialect) -> IntegerType {
    match dialect {
        Dialect::MySql => IntegerType::signed(IntegerSize::Big),
        Dialect::MariaDb => IntegerType::signed(IntegerSize::Int),
    }
}

/// Type of an integer literal: `BIGINT`, `BIGINT UNSIGNED` beyond, or
/// `DECIMAL` beyond `BIGINT UNSIGNED`. MariaDB gives `INT` to the literals
/// an `INT` holds.
pub(crate) fn integer_literal(value: i128, digits: usize, dialect: Dialect) -> DataType {
    let int = IntegerType::signed(IntegerSize::Int);
    if dialect == Dialect::MariaDb && int.contains(value) {
        return DataType::Integer(int);
    }
    if i64::try_from(value).is_ok() {
        bigint()
    } else if u64::try_from(value).is_ok() {
        DataType::Integer(IntegerType::unsigned(IntegerSize::Big))
    } else {
        decimal(digits, 0)
    }
}

/// `DECIMAL(precision, scale)`, bounded by the limits of the MySQL family.
pub(crate) fn decimal(precision: usize, scale: usize) -> DataType {
    let scale = u8::try_from(scale).unwrap_or(MAX_SCALE).min(MAX_SCALE);
    let precision = u8::try_from(precision)
        .unwrap_or(MAX_DECIMAL_PRECISION)
        .clamp(scale.max(1), MAX_DECIMAL_PRECISION);
    DecimalType::new(precision, scale).map_or_else(|_| double(), DataType::Decimal)
}

/// Type of a string literal: a `VARCHAR` of its length, in the charset of
/// the connection.
pub(crate) fn string_literal(value: &str) -> DataType {
    DataType::Text(TextType {
        length: u32::try_from(value.chars().count()).ok(),
        fixed: false,
        charset: None,
        collation: None,
    })
}

/// Numeric class of an operand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Numeric {
    /// NULL, of no type.
    Null,
    /// Integer.
    Integer(IntegerType),
    /// Decimal: digits before the point, and scale.
    Decimal(usize, usize),
    /// Floating point number, or a value converted to one (string, date).
    Float,
}

/// Numeric class of a data type.
fn numeric(data_type: &DataType) -> Numeric {
    match data_type {
        DataType::Null => Numeric::Null,
        DataType::Boolean => Numeric::Integer(IntegerType::signed(IntegerSize::Big)),
        DataType::Integer(integer) => Numeric::Integer(*integer),
        DataType::Year => Numeric::Integer(IntegerType::unsigned(IntegerSize::Small)),
        DataType::Bit { .. } => Numeric::Integer(IntegerType::unsigned(IntegerSize::Big)),
        DataType::Decimal(decimal) => Numeric::Decimal(
            usize::from(decimal.precision() - decimal.scale()),
            usize::from(decimal.scale()),
        ),
        _ => Numeric::Float,
    }
}

/// Digits of the largest value of an integer type.
fn integer_digits(integer: IntegerType) -> usize {
    integer.max().max(-integer.min()).to_string().len()
}

/// Digits before the point and scale of a numeric class, as a decimal.
fn as_decimal(numeric: Numeric) -> (usize, usize) {
    match numeric {
        Numeric::Integer(integer) => (integer_digits(integer), 0),
        Numeric::Decimal(integer, scale) => (integer, scale),
        Numeric::Null | Numeric::Float => (0, 0),
    }
}

/// Type of an arithmetic operation in the MySQL family.
pub(crate) fn arithmetic(op: Arithmetic, left: &Type, right: &Type) -> Type {
    let by_zero = matches!(
        op,
        Arithmetic::Divide | Arithmetic::IntegerDivide | Arithmetic::Modulo
    );
    let nullable = left.nullable || right.nullable || by_zero;
    let (left, right) = (numeric(&left.data_type), numeric(&right.data_type));
    let data_type = match (left, right) {
        (Numeric::Null, Numeric::Null) => DataType::Null,
        _ if op == Arithmetic::IntegerDivide => DataType::Integer(integer_result(left, right)),
        (Numeric::Float, _) | (_, Numeric::Float) => double(),
        (Numeric::Integer(_) | Numeric::Null, Numeric::Integer(_) | Numeric::Null)
            if op != Arithmetic::Divide =>
        {
            DataType::Integer(integer_result(left, right))
        }
        _ => decimal_result(op, as_decimal(left), as_decimal(right)),
    };
    Type {
        data_type,
        nullable,
    }
}

/// `BIGINT`, unsigned when an operand is.
fn integer_result(left: Numeric, right: Numeric) -> IntegerType {
    let unsigned = |numeric| matches!(numeric, Numeric::Integer(integer) if integer.unsigned);
    if unsigned(left) || unsigned(right) {
        IntegerType::unsigned(IntegerSize::Big)
    } else {
        IntegerType::signed(IntegerSize::Big)
    }
}

/// `DECIMAL` of an operation on decimals, from the digits before the point
/// and the scale of its operands.
fn decimal_result(op: Arithmetic, left: (usize, usize), right: (usize, usize)) -> DataType {
    let ((left_integer, left_scale), (right_integer, right_scale)) = (left, right);
    let (integer, scale) = match op {
        Arithmetic::Multiply => (left_integer + right_integer, left_scale + right_scale),
        Arithmetic::Divide => (
            left_integer + right_scale,
            left_scale + usize::from(DIVISION_INCREMENT),
        ),
        Arithmetic::Add | Arithmetic::Subtract => (
            left_integer.max(right_integer) + 1,
            left_scale.max(right_scale),
        ),
        Arithmetic::Modulo | Arithmetic::IntegerDivide => {
            (left_integer.max(right_integer), left_scale.max(right_scale))
        }
    };
    decimal(integer + scale, scale)
}

/// Type of the opposite of a number: the type of the number, signed.
pub(crate) fn negate(operand: &Type) -> Type {
    let data_type = match &operand.data_type {
        DataType::Integer(_) | DataType::Boolean => bigint(),
        DataType::Decimal(decimal) => DataType::Decimal(*decimal),
        DataType::Null => DataType::Null,
        _ => double(),
    };
    Type {
        data_type,
        nullable: operand.nullable,
    }
}

/// Type that holds the values of several types, for the columns of a
/// `VALUES` (and later of a `UNION` or a `CASE`).
pub(crate) fn aggregate<'a>(types: impl IntoIterator<Item = &'a DataType>) -> DataType {
    types
        .into_iter()
        .fold(DataType::Null, |merged, next| merge(&merged, next))
}

/// Type that holds the values of two types.
fn merge(left: &DataType, right: &DataType) -> DataType {
    match (left, right) {
        (DataType::Null, other) | (other, DataType::Null) => other.clone(),
        _ if left == right => left.clone(),
        (DataType::Text(left), DataType::Text(right)) => DataType::Text(TextType {
            length: left.length.zip(right.length).map(|(a, b)| a.max(b)),
            fixed: false,
            charset: left.charset.clone().or_else(|| right.charset.clone()),
            collation: left.collation.clone().or_else(|| right.collation.clone()),
        }),
        (DataType::Text(_), _) | (_, DataType::Text(_)) => DataType::Text(TextType {
            length: None,
            fixed: false,
            charset: None,
            collation: None,
        }),
        _ => match (numeric(left), numeric(right)) {
            (Numeric::Integer(a), Numeric::Integer(b)) if a.unsigned == b.unsigned => {
                DataType::Integer(if a.size.bytes() >= b.size.bytes() {
                    a
                } else {
                    b
                })
            }
            (Numeric::Float, _) | (_, Numeric::Float) => double(),
            (a, b) => {
                let ((a_integer, a_scale), (b_integer, b_scale)) = (as_decimal(a), as_decimal(b));
                let scale = a_scale.max(b_scale);
                decimal(a_integer.max(b_integer) + scale, scale)
            }
        },
    }
}

/// Digits `SUM` adds to those of an exact number (measured on MySQL and
/// MariaDB: `SUM` of an `INT` is a `DECIMAL(32, 0)`).
const SUM_INCREMENT: usize = 22;

/// Digits `AVG` adds to the precision and to the scale of an exact number
/// (`AVG` of an `INT` is a `DECIMAL(14, 4)`).
const AVG_INCREMENT: usize = 4;

/// Type of `SUM`: a `DECIMAL` of more digits for an exact number, a
/// `DOUBLE` otherwise.
pub(crate) fn sum(argument: &DataType) -> DataType {
    match numeric(argument) {
        exact @ (Numeric::Integer(_) | Numeric::Decimal(..)) => {
            let (integer, scale) = as_decimal(exact);
            decimal(integer + scale + SUM_INCREMENT, scale)
        }
        Numeric::Null | Numeric::Float => double(),
    }
}

/// Type of `AVG`: a `DECIMAL` of more digits and scale for an exact number,
/// a `DOUBLE` otherwise.
pub(crate) fn avg(argument: &DataType) -> DataType {
    match numeric(argument) {
        exact @ (Numeric::Integer(_) | Numeric::Decimal(..)) => {
            let (integer, scale) = as_decimal(exact);
            decimal(integer + scale + AVG_INCREMENT, scale + AVG_INCREMENT)
        }
        Numeric::Null | Numeric::Float => double(),
    }
}

/// Type of a rank (`ROW_NUMBER`, `RANK`, `NTILE`...): `BIGINT UNSIGNED` in
/// MySQL, `BIGINT` in MariaDB.
pub(crate) const fn rank(dialect: Dialect) -> DataType {
    match dialect {
        Dialect::MySql => DataType::Integer(IntegerType::unsigned(IntegerSize::Big)),
        Dialect::MariaDb => bigint(),
    }
}

/// Type of a value a window function reads from another row (`LAG`,
/// `FIRST_VALUE`...): MySQL widens integers to `BIGINT`, MariaDB keeps the
/// type.
pub(crate) fn window_value(data_type: &DataType, dialect: Dialect) -> DataType {
    match (data_type, dialect) {
        (DataType::Integer(integer), Dialect::MySql) => DataType::Integer(IntegerType {
            size: IntegerSize::Big,
            ..*integer
        }),
        _ => data_type.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn not_null(data_type: DataType) -> Type {
        Type::not_null(data_type)
    }

    #[test]
    fn integer_literals_grow_to_decimal() {
        assert_eq!(integer_literal(123, 3, Dialect::MySql), bigint());
        assert_eq!(
            integer_literal(123, 3, Dialect::MariaDb),
            DataType::Integer(IntegerType::signed(IntegerSize::Int))
        );
        assert_eq!(
            integer_literal(i128::from(u64::MAX), 20, Dialect::MySql),
            DataType::Integer(IntegerType::unsigned(IntegerSize::Big))
        );
        assert_eq!(
            integer_literal(i128::from(u64::MAX) + 1, 20, Dialect::MySql),
            decimal(20, 0)
        );
    }

    #[test]
    fn arithmetic_follows_the_operands() {
        let int = not_null(bigint());
        let dec = not_null(decimal(5, 2));
        let add = arithmetic(Arithmetic::Add, &int, &int);
        assert_eq!(add, not_null(bigint()));
        // BIGINT has 19 digits; with DECIMAL(5, 2): 20 before the point, 2 after.
        assert_eq!(
            arithmetic(Arithmetic::Add, &int, &dec),
            not_null(decimal(22, 2))
        );
        assert_eq!(
            arithmetic(Arithmetic::Multiply, &dec, &dec),
            not_null(decimal(10, 4))
        );
        // Division: a nullable decimal, 4 more digits of scale.
        assert_eq!(
            arithmetic(Arithmetic::Divide, &int, &int),
            Type::nullable(decimal(23, 4))
        );
        assert_eq!(
            arithmetic(Arithmetic::Add, &int, &not_null(double())),
            not_null(double())
        );
    }

    #[test]
    fn aggregate_takes_the_wider_type() {
        let int = DataType::Integer(IntegerType::signed(IntegerSize::Int));
        assert_eq!(aggregate([&DataType::Null, &int]), int);
        assert_eq!(aggregate([&int, &bigint()]), bigint());
        assert_eq!(aggregate([&int, &decimal(4, 2)]), decimal(12, 2));
        assert_eq!(aggregate([&int, &double()]), double());
        assert_eq!(
            aggregate([&string_literal("ab"), &string_literal("abcd")]),
            string_literal("abcd")
        );
    }
}
