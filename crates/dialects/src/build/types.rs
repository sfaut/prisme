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
    BinaryType, DataType, DecimalType, FloatType, IntegerSize, IntegerType, MAX_DECIMAL_PRECISION,
    TextType, Type,
};

use crate::Dialect;
use crate::sql::ast::BytesForm;

/// Largest scale of a `DECIMAL` in the MySQL family.
const MAX_SCALE: u8 = 30;

/// Digits added to the scale of a division (`div_precision_increment`).
const DIVISION_INCREMENT: u8 = 4;

/// Most digits of an integer literal of type `INT` in MariaDB.
const MARIADB_INT_DIGITS: usize = 9;

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

/// Type of an integer literal of `digits` digits: `BIGINT`, `BIGINT
/// UNSIGNED` beyond, or `DECIMAL` beyond `BIGINT UNSIGNED`. MariaDB gives
/// `INT` to the literals of at most 9 digits, leading zeros included,
/// whatever their value: `999999999` and `000000001` are `INT`s,
/// `1000000000` and `0999999999` `BIGINT`s
/// (`crates/probe/cases/integer-literals.txt`).
pub(crate) fn integer_literal(value: i128, digits: usize, dialect: Dialect) -> DataType {
    if dialect == Dialect::MariaDb && digits <= MARIADB_INT_DIGITS {
        return DataType::Integer(IntegerType::signed(IntegerSize::Int));
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

/// Type of a national string, `N'abc'`: a `VARCHAR` of `utf8mb3`, of the
/// collation `utf8mb3_general_ci` in every version of both dialects.
pub(crate) fn national_string(value: &str) -> DataType {
    DataType::Text(TextType {
        length: u32::try_from(value.chars().count()).ok(),
        fixed: false,
        charset: Some("utf8mb3".to_string()),
        collation: Some("utf8mb3_general_ci".to_string()),
    })
}

/// Type of a string after the introducer `charset` (`_latin1'abc'`): a
/// `VARCHAR` of that character set, of its default collation (`None`).
/// Its length counts the characters of a Unicode character set, unknown in
/// another one, which reads the bytes of the text otherwise (`_latin1'é'`
/// holds two characters).
pub(crate) fn introduced_string(charset: &str, value: &str) -> DataType {
    let unicode = matches!(charset, "utf8" | "utf8mb3" | "utf8mb4");
    DataType::Text(TextType {
        length: if unicode {
            u32::try_from(value.chars().count()).ok()
        } else {
            None
        },
        fixed: false,
        charset: Some(charset.to_string()),
        collation: None,
    })
}

/// Type of bytes after the introducer `charset`, but `binary`
/// (`_latin1 X'E9'`): a `VARCHAR` of that character set, of its default
/// collation, of a length unknown.
pub(crate) fn introduced_bytes(charset: &str) -> DataType {
    DataType::Text(TextType {
        length: None,
        fixed: false,
        charset: Some(charset.to_string()),
        collation: None,
    })
}

/// Type of a hexadecimal or bit literal of `length` bytes, of the form
/// `form`, in `dialect`: a hybrid, signed for a bit literal of MySQL
/// (`b'101' + 0` is an `INT`, an `INT UNSIGNED` in MariaDB), but `X'41'` in
/// MariaDB, a byte string (`X'41' + 0` gives 65 in MySQL, 0 in MariaDB).
pub(crate) fn bytes_literal(length: usize, form: BytesForm, dialect: Dialect) -> DataType {
    let bits = matches!(form, BytesForm::BitString | BytesForm::BitNumber);
    match (form, dialect) {
        (BytesForm::HexString, Dialect::MariaDb) => byte_string(length),
        _ => DataType::Hybrid {
            length: u32::try_from(length).unwrap_or(u32::MAX),
            signed: bits && dialect == Dialect::MySql,
        },
    }
}

/// Type of a byte string of `length` bytes, a literal (`_binary'abc'`, and
/// `X'41'` in MariaDB): a `VARBINARY`.
pub(crate) fn byte_string(length: usize) -> DataType {
    DataType::Binary(BinaryType {
        length: u32::try_from(length).ok(),
        fixed: false,
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
        // `0x41 + 0` is an `INT UNSIGNED`, `0x0102030405 + 0` a `BIGINT
        // UNSIGNED`.
        DataType::Hybrid { length, signed } => {
            let size = if *length <= 4 {
                IntegerSize::Int
            } else {
                IntegerSize::Big
            };
            Numeric::Integer(IntegerType {
                size,
                unsigned: !signed,
            })
        }
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

/// Digits MariaDB gives an integer in a `DECIMAL` that holds it: its display
/// width, sign aside, 8 for a signed `MEDIUMINT` (`MEDIUMINT(9)`).
fn display_digits(integer: IntegerType) -> usize {
    match (integer.size, integer.unsigned) {
        (IntegerSize::Medium, false) => 8,
        _ => integer_digits(integer),
    }
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
/// `VALUES` and of a set operation, and the default value of `LAG`.
pub(crate) fn aggregate<'a>(types: impl IntoIterator<Item = &'a DataType>) -> DataType {
    types
        .into_iter()
        .fold(DataType::Null, |merged, next| merge(&merged, next))
}

/// Type of a `CASE` whose results are of the types `types` (measured on
/// MySQL 8.0.11 and 8.4.11, MariaDB 10.6.28 and 11.8.9:
/// `crates/probe/cases/case.txt`, `case-integers.sql`). The families merge
/// alike in both dialects, the integers otherwise:
///
/// - MySQL gives an `INT` or a `BIGINT`, unsigned when all are, a
///   `DECIMAL(20, 0)` for a `BIGINT UNSIGNED` with a signed integer; a
///   `NULL` changes nothing;
/// - MariaDB keeps the largest size, and gives a `DECIMAL` of the digits of
///   the largest integer to a signed integer with an unsigned one, a `NULL`
///   literal counting as a signed integer.
pub(crate) fn case<'a>(
    dialect: Dialect,
    types: impl IntoIterator<Item = &'a DataType>,
) -> DataType {
    let types: Vec<&DataType> = types.into_iter().collect();
    let integers: Vec<IntegerType> = types
        .iter()
        .filter_map(|data_type| match data_type {
            DataType::Integer(integer) => Some(*integer),
            _ => None,
        })
        .collect();
    let only_integers = types
        .iter()
        .all(|data_type| matches!(data_type, DataType::Integer(_) | DataType::Null));
    if integers.is_empty() || !only_integers {
        return aggregate(types);
    }
    let unsigned = integers.iter().all(|integer| integer.unsigned);
    let largest = |integers: &[IntegerType]| {
        integers
            .iter()
            .map(|integer| integer.size)
            .max_by_key(|size| size.bytes())
            .unwrap_or(IntegerSize::Int)
    };
    match dialect {
        Dialect::MySql => {
            let big_unsigned = integers
                .iter()
                .any(|integer| integer.unsigned && integer.size == IntegerSize::Big);
            if big_unsigned && !unsigned {
                return decimal(20, 0);
            }
            let size = match largest(&integers) {
                IntegerSize::Big => IntegerSize::Big,
                _ => IntegerSize::Int,
            };
            DataType::Integer(IntegerType { size, unsigned })
        }
        Dialect::MariaDb => {
            let any_unsigned = integers.iter().any(|integer| integer.unsigned);
            let any_signed = !unsigned || types.contains(&&DataType::Null);
            if any_unsigned && any_signed {
                let digits = integers
                    .iter()
                    .map(|integer| display_digits(*integer))
                    .max();
                return decimal(digits.unwrap_or(1), 0);
            }
            DataType::Integer(IntegerType {
                size: largest(&integers),
                unsigned,
            })
        }
    }
}

/// Type that holds the values of two types, as a `CASE` merges them in
/// both dialects, the integers aside; a hybrid merges as a byte string.
fn merge(left: &DataType, right: &DataType) -> DataType {
    let (left, right) = (&left.stored(), &right.stored());
    let text = || {
        DataType::Text(TextType {
            length: None,
            fixed: false,
            charset: None,
            collation: None,
        })
    };
    match (left, right) {
        (DataType::Null, other) | (other, DataType::Null) => other.clone(),
        (DataType::Unknown, _) | (_, DataType::Unknown) => DataType::Unknown,
        _ if left == right => left.clone(),
        (DataType::Text(left), DataType::Text(right)) => DataType::Text(TextType {
            length: left.length.zip(right.length).map(|(a, b)| a.max(b)),
            fixed: false,
            charset: left.charset.clone().or_else(|| right.charset.clone()),
            collation: left.collation.clone().or_else(|| right.collation.clone()),
        }),
        // A `JSON` with another type is a `LONGTEXT`.
        (DataType::Json, _) | (_, DataType::Json) => text(),
        // A byte string with a character string: bytes, unless the
        // characters have the collation of a column and the bytes are a
        // literal (coercibility), which the plan does not tell apart.
        (DataType::Binary(_), _) | (_, DataType::Binary(_)) => DataType::Binary(BinaryType {
            length: None,
            fixed: false,
        }),
        (DataType::Text(_), _) | (_, DataType::Text(_)) => text(),
        _ if temporal(left) && temporal(right) => merge_temporal(left, right),
        // A date or a time with a number: a string.
        _ if temporal(left) || temporal(right) => text(),
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

/// Tells whether `data_type` is a date, a time or both.
const fn temporal(data_type: &DataType) -> bool {
    matches!(
        data_type,
        DataType::Date
            | DataType::Time { .. }
            | DataType::DateTime { .. }
            | DataType::Timestamp { .. }
    )
}

/// Type that holds the values of two different temporal types: a
/// `DATETIME` of the larger fraction, a `TIMESTAMP` only with another one.
fn merge_temporal(left: &DataType, right: &DataType) -> DataType {
    let fraction = |data_type: &DataType| match data_type {
        DataType::Time { fraction }
        | DataType::DateTime { fraction }
        | DataType::Timestamp { fraction } => *fraction,
        _ => 0,
    };
    let fraction = fraction(left).max(fraction(right));
    match (left, right) {
        (DataType::Timestamp { .. }, DataType::Timestamp { .. }) => {
            DataType::Timestamp { fraction }
        }
        (DataType::Time { .. }, DataType::Time { .. }) => DataType::Time { fraction },
        _ => DataType::DateTime { fraction },
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
        let int = DataType::Integer(IntegerType::signed(IntegerSize::Int));
        assert_eq!(integer_literal(123, 3, Dialect::MariaDb), int);
        assert_eq!(integer_literal(999_999_999, 9, Dialect::MariaDb), int);
        assert_eq!(integer_literal(1, 9, Dialect::MariaDb), int);
        assert_eq!(
            integer_literal(1_000_000_000, 10, Dialect::MariaDb),
            bigint()
        );
        assert_eq!(integer_literal(999_999_999, 10, Dialect::MariaDb), bigint());
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
