//! Type of the columns of a MySQL or MariaDB result, as the interface shows it
//! in the grid's header (`describe`) and as the logical plan types it
//! (`plan_type`, internal types of `prisme_types`).
//!
//! The type comes from the metadata the server sends with the result: type
//! code, flags (`UNSIGNED`, `ENUM`, `SET`), length, number of decimals and
//! character set. It is the type of the *result*, which does not always tell
//! everything about the type declared in the table:
//!
//! - `BOOLEAN` arrives as `TINYINT(1)`;
//! - MariaDB's JSON arrives as `LONGTEXT`;
//! - `ENUM` and `SET` are recognised by a flag, without their values;
//! - a computed expression (`COUNT(*)`, `CONCAT(...)`) has the type chosen by
//!   the server.
//!
//! The length of a text arrives in bytes, in the character set of the
//! connection's results: it is brought back to characters for the known sets
//! (`charset_max_bytes`). For another set, the length is left out rather than
//! guessed.

use mysql_async::consts::{ColumnFlags, ColumnType};
use prisme_types::{
    BinaryType, DataType, DecimalType, FloatType, IntegerSize, IntegerType, MAX_FRACTION_DIGITS,
    TextType, Type,
};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Family of a column type, for the icon of the grid's header.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum TypeFamily {
    /// `TINYINT` to `BIGINT`.
    Integer,
    /// `DECIMAL`.
    Decimal,
    /// `FLOAT`, `DOUBLE`.
    Float,
    /// `CHAR`, `VARCHAR`, `TEXT`.
    Text,
    /// `BINARY`, `VARBINARY`, `BLOB`.
    Binary,
    /// `DATE`.
    Date,
    /// `TIME`.
    Time,
    /// `DATETIME`, `TIMESTAMP`.
    Datetime,
    /// `YEAR`.
    Year,
    /// `JSON` (MySQL).
    Json,
    /// `BIT`.
    Bit,
    /// `ENUM`.
    Enum,
    /// `SET`.
    Set,
    /// `GEOMETRY` and its derivatives.
    Geometry,
    /// Other type, or `NULL` (column of a `NULL` expression).
    Other,
}

/// Type of a result column: its family and its native name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ColumnTypeInfo {
    /// Family of the type.
    pub family: TypeFamily,
    /// Name of the type in the server's dialect: `INT UNSIGNED`,
    /// `DECIMAL(10,2)`, `VARCHAR(20)`, `DATETIME(6)`...
    pub name: String,
}

/// Collation of the `binary` character set: a byte string.
const BINARY_COLLATION: u16 = 63;

/// "Not fixed" number of decimals sent by the server (expression whose
/// precision is unknown).
const NOT_FIXED_DEC: u8 = 31;

/// Describes the type of a column from its result metadata.
///
/// `length` is the announced length (in bytes for a text), `charset` the
/// collation id of the results.
pub fn describe(
    column_type: ColumnType,
    flags: ColumnFlags,
    length: u32,
    decimals: u8,
    charset: u16,
) -> ColumnTypeInfo {
    let unsigned = flags.contains(ColumnFlags::UNSIGNED_FLAG);
    let binary = charset == BINARY_COLLATION;
    let (family, name) = if flags.contains(ColumnFlags::ENUM_FLAG) {
        (TypeFamily::Enum, "ENUM".to_string())
    } else if flags.contains(ColumnFlags::SET_FLAG) {
        (TypeFamily::Set, "SET".to_string())
    } else {
        match column_type {
            ColumnType::MYSQL_TYPE_TINY if length == 1 => {
                (TypeFamily::Integer, numeric("TINYINT(1)", unsigned))
            }
            ColumnType::MYSQL_TYPE_TINY => (TypeFamily::Integer, numeric("TINYINT", unsigned)),
            ColumnType::MYSQL_TYPE_SHORT => (TypeFamily::Integer, numeric("SMALLINT", unsigned)),
            ColumnType::MYSQL_TYPE_INT24 => (TypeFamily::Integer, numeric("MEDIUMINT", unsigned)),
            ColumnType::MYSQL_TYPE_LONG => (TypeFamily::Integer, numeric("INT", unsigned)),
            ColumnType::MYSQL_TYPE_LONGLONG => (TypeFamily::Integer, numeric("BIGINT", unsigned)),
            ColumnType::MYSQL_TYPE_DECIMAL | ColumnType::MYSQL_TYPE_NEWDECIMAL => {
                (TypeFamily::Decimal, decimal(length, decimals, unsigned))
            }
            ColumnType::MYSQL_TYPE_FLOAT => (TypeFamily::Float, numeric("FLOAT", unsigned)),
            ColumnType::MYSQL_TYPE_DOUBLE => (TypeFamily::Float, numeric("DOUBLE", unsigned)),
            ColumnType::MYSQL_TYPE_YEAR => (TypeFamily::Year, "YEAR".to_string()),
            ColumnType::MYSQL_TYPE_DATE | ColumnType::MYSQL_TYPE_NEWDATE => {
                (TypeFamily::Date, "DATE".to_string())
            }
            ColumnType::MYSQL_TYPE_TIME | ColumnType::MYSQL_TYPE_TIME2 => {
                (TypeFamily::Time, with_fraction("TIME", decimals))
            }
            ColumnType::MYSQL_TYPE_DATETIME | ColumnType::MYSQL_TYPE_DATETIME2 => {
                (TypeFamily::Datetime, with_fraction("DATETIME", decimals))
            }
            ColumnType::MYSQL_TYPE_TIMESTAMP | ColumnType::MYSQL_TYPE_TIMESTAMP2 => {
                (TypeFamily::Datetime, with_fraction("TIMESTAMP", decimals))
            }
            ColumnType::MYSQL_TYPE_BIT => (TypeFamily::Bit, format!("BIT({length})")),
            ColumnType::MYSQL_TYPE_JSON => (TypeFamily::Json, "JSON".to_string()),
            ColumnType::MYSQL_TYPE_ENUM => (TypeFamily::Enum, "ENUM".to_string()),
            ColumnType::MYSQL_TYPE_SET => (TypeFamily::Set, "SET".to_string()),
            ColumnType::MYSQL_TYPE_GEOMETRY => (TypeFamily::Geometry, "GEOMETRY".to_string()),
            ColumnType::MYSQL_TYPE_STRING if binary => {
                (TypeFamily::Binary, sized("BINARY", Some(length)))
            }
            ColumnType::MYSQL_TYPE_STRING => {
                (TypeFamily::Text, sized("CHAR", characters(length, charset)))
            }
            ColumnType::MYSQL_TYPE_VAR_STRING | ColumnType::MYSQL_TYPE_VARCHAR if binary => {
                (TypeFamily::Binary, sized("VARBINARY", Some(length)))
            }
            ColumnType::MYSQL_TYPE_VAR_STRING | ColumnType::MYSQL_TYPE_VARCHAR => (
                TypeFamily::Text,
                sized("VARCHAR", characters(length, charset)),
            ),
            ColumnType::MYSQL_TYPE_TINY_BLOB
            | ColumnType::MYSQL_TYPE_MEDIUM_BLOB
            | ColumnType::MYSQL_TYPE_LONG_BLOB
            | ColumnType::MYSQL_TYPE_BLOB => large_object(length, charset),
            ColumnType::MYSQL_TYPE_VECTOR => (TypeFamily::Other, "VECTOR".to_string()),
            ColumnType::MYSQL_TYPE_NULL => (TypeFamily::Other, "NULL".to_string()),
            _ => (TypeFamily::Other, "?".to_string()),
        }
    };
    ColumnTypeInfo { family, name }
}

/// Internal type of a column (`prisme_types`), from the same metadata as
/// `describe`, with its nullability (`NOT_NULL` flag).
///
/// The character set of the metadata is the one of the connection's
/// results, not that of the column: the charset and the collation of a text
/// are left unknown.
pub fn plan_type(
    column_type: ColumnType,
    flags: ColumnFlags,
    length: u32,
    decimals: u8,
    charset: u16,
) -> Type {
    let nullable = !flags.contains(ColumnFlags::NOT_NULL_FLAG);
    let data_type = if flags.contains(ColumnFlags::ENUM_FLAG) {
        DataType::Enum
    } else if flags.contains(ColumnFlags::SET_FLAG) {
        DataType::Set
    } else {
        plan_data_type(column_type, flags, length, decimals, charset)
    };
    Type {
        data_type,
        nullable,
    }
}

/// Data type of a column, `ENUM` and `SET` aside.
fn plan_data_type(
    column_type: ColumnType,
    flags: ColumnFlags,
    length: u32,
    decimals: u8,
    charset: u16,
) -> DataType {
    let unsigned = flags.contains(ColumnFlags::UNSIGNED_FLAG);
    let integer = |size| DataType::Integer(IntegerType { size, unsigned });
    match column_type {
        ColumnType::MYSQL_TYPE_TINY => integer(IntegerSize::Tiny),
        ColumnType::MYSQL_TYPE_SHORT => integer(IntegerSize::Small),
        ColumnType::MYSQL_TYPE_INT24 => integer(IntegerSize::Medium),
        ColumnType::MYSQL_TYPE_LONG => integer(IntegerSize::Int),
        ColumnType::MYSQL_TYPE_LONGLONG => integer(IntegerSize::Big),
        ColumnType::MYSQL_TYPE_DECIMAL | ColumnType::MYSQL_TYPE_NEWDECIMAL => {
            plan_decimal(length, decimals, unsigned)
        }
        ColumnType::MYSQL_TYPE_FLOAT => DataType::Float(FloatType::Single),
        ColumnType::MYSQL_TYPE_DOUBLE => DataType::Float(FloatType::Double),
        ColumnType::MYSQL_TYPE_YEAR => DataType::Year,
        ColumnType::MYSQL_TYPE_DATE | ColumnType::MYSQL_TYPE_NEWDATE => DataType::Date,
        ColumnType::MYSQL_TYPE_TIME | ColumnType::MYSQL_TYPE_TIME2 => DataType::Time {
            fraction: fraction(decimals),
        },
        ColumnType::MYSQL_TYPE_DATETIME | ColumnType::MYSQL_TYPE_DATETIME2 => DataType::DateTime {
            fraction: fraction(decimals),
        },
        ColumnType::MYSQL_TYPE_TIMESTAMP | ColumnType::MYSQL_TYPE_TIMESTAMP2 => {
            DataType::Timestamp {
                fraction: fraction(decimals),
            }
        }
        // A `BIT` column announces its number of bits, 1 to 64.
        ColumnType::MYSQL_TYPE_BIT => DataType::Bit {
            width: u8::try_from(length).unwrap_or(u8::MAX),
        },
        ColumnType::MYSQL_TYPE_JSON => DataType::Json,
        ColumnType::MYSQL_TYPE_ENUM => DataType::Enum,
        ColumnType::MYSQL_TYPE_SET => DataType::Set,
        ColumnType::MYSQL_TYPE_GEOMETRY => DataType::Geometry,
        ColumnType::MYSQL_TYPE_STRING => string(true, length, charset),
        ColumnType::MYSQL_TYPE_VAR_STRING | ColumnType::MYSQL_TYPE_VARCHAR => {
            string(false, length, charset)
        }
        ColumnType::MYSQL_TYPE_TINY_BLOB
        | ColumnType::MYSQL_TYPE_MEDIUM_BLOB
        | ColumnType::MYSQL_TYPE_LONG_BLOB
        | ColumnType::MYSQL_TYPE_BLOB => string(false, length, charset),
        ColumnType::MYSQL_TYPE_VECTOR => DataType::Other("VECTOR".to_string()),
        ColumnType::MYSQL_TYPE_NULL => DataType::Null,
        _ => DataType::Unknown,
    }
}

/// Decimal type from its announced length (see `decimal`); a length outside
/// the model keeps the type by its name.
fn plan_decimal(length: u32, scale: u8, unsigned: bool) -> DataType {
    let precision = length
        .saturating_sub(u32::from(scale > 0))
        .saturating_sub(u32::from(!unsigned));
    u8::try_from(precision)
        .ok()
        .and_then(|precision| DecimalType::new(precision, scale).ok())
        .map_or_else(
            || DataType::Other(decimal(length, scale, unsigned)),
            DataType::Decimal,
        )
}

/// Digits of the fractional seconds; the "not fixed" number of an
/// expression counts as the largest one.
fn fraction(decimals: u8) -> u8 {
    decimals.min(MAX_FRACTION_DIGITS)
}

/// Text or byte string, fixed or variable, depending on the character set.
fn string(fixed: bool, length: u32, charset: u16) -> DataType {
    if charset == BINARY_COLLATION {
        DataType::Binary(BinaryType {
            length: Some(length),
            fixed,
        })
    } else {
        DataType::Text(TextType {
            length: characters(length, charset),
            fixed,
            charset: None,
            collation: None,
        })
    }
}

/// Name of a numeric type, followed by `UNSIGNED` when relevant.
fn numeric(name: &str, unsigned: bool) -> String {
    if unsigned {
        format!("{name} UNSIGNED")
    } else {
        name.to_string()
    }
}

/// `DECIMAL(precision,scale)`. The announced length counts the decimal point
/// (if there are decimals) and the sign (except `UNSIGNED`).
fn decimal(length: u32, scale: u8, unsigned: bool) -> String {
    let precision = length
        .saturating_sub(u32::from(scale > 0))
        .saturating_sub(u32::from(!unsigned));
    numeric(&format!("DECIMAL({precision},{scale})"), unsigned)
}

/// Name of a temporal type, with its fractions of a second if it has any.
fn with_fraction(name: &str, decimals: u8) -> String {
    if decimals == 0 || decimals >= NOT_FIXED_DEC {
        name.to_string()
    } else {
        format!("{name}({decimals})")
    }
}

/// Name of a type with its length between parentheses, if it is known.
fn sized(name: &str, length: Option<u32>) -> String {
    match length {
        Some(length) => format!("{name}({length})"),
        None => name.to_string(),
    }
}

/// `TINYTEXT` to `LONGTEXT`, or `TINYBLOB` to `LONGBLOB` for the `binary`
/// set, from the maximum length brought back to characters.
fn large_object(length: u32, charset: u16) -> (TypeFamily, String) {
    let (family, suffix) = if charset == BINARY_COLLATION {
        (TypeFamily::Binary, "BLOB")
    } else {
        (TypeFamily::Text, "TEXT")
    };
    let Some(characters) = characters(length, charset) else {
        return (family, suffix.to_string());
    };
    let prefix = match characters {
        0..=255 => "TINY",
        256..=65_535 => "",
        65_536..=16_777_215 => "MEDIUM",
        _ => "LONG",
    };
    (family, format!("{prefix}{suffix}"))
}

/// Length of a text in characters, from its length in bytes; `None` for an
/// unknown character set.
fn characters(bytes: u32, charset: u16) -> Option<u32> {
    charset_max_bytes(charset).map(|max| bytes / max)
}

/// Maximum number of bytes per character of the common character sets, from
/// the collation id; `None` for the others.
fn charset_max_bytes(collation: u16) -> Option<u32> {
    match collation {
        // utf8mb4: utf8mb4_general_ci, _bin, Unicode and 0900 collations of MySQL 8.
        45 | 46 | 224..=247 | 255..=323 => Some(4),
        // utf8mb3 (utf8 before MySQL 8).
        33 | 76 | 83 | 192..=215 | 223 => Some(3),
        // binary, latin1, ascii.
        63 | 5 | 8 | 15 | 31 | 47..=49 | 94 | 11 | 65 => Some(1),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// utf8mb4_general_ci collation, that of Prisme's results.
    const UTF8MB4: u16 = 45;

    fn name(
        column_type: ColumnType,
        flags: ColumnFlags,
        length: u32,
        decimals: u8,
        charset: u16,
    ) -> String {
        describe(column_type, flags, length, decimals, charset).name
    }

    #[test]
    fn names_numbers() {
        let none = ColumnFlags::empty();
        assert_eq!(name(ColumnType::MYSQL_TYPE_LONG, none, 11, 0, 63), "INT");
        assert_eq!(
            name(
                ColumnType::MYSQL_TYPE_LONGLONG,
                ColumnFlags::UNSIGNED_FLAG,
                20,
                0,
                63
            ),
            "BIGINT UNSIGNED"
        );
        assert_eq!(
            name(ColumnType::MYSQL_TYPE_TINY, none, 1, 0, 63),
            "TINYINT(1)"
        );
        // Signed DECIMAL(10,2): 10 digits, the decimal point and the sign.
        assert_eq!(
            name(ColumnType::MYSQL_TYPE_NEWDECIMAL, none, 12, 2, 63),
            "DECIMAL(10,2)"
        );
        assert_eq!(
            name(
                ColumnType::MYSQL_TYPE_NEWDECIMAL,
                ColumnFlags::UNSIGNED_FLAG,
                5,
                0,
                63
            ),
            "DECIMAL(5,0) UNSIGNED"
        );
    }

    #[test]
    fn names_texts_in_characters() {
        let none = ColumnFlags::empty();
        assert_eq!(
            name(ColumnType::MYSQL_TYPE_VAR_STRING, none, 80, 0, UTF8MB4),
            "VARCHAR(20)"
        );
        assert_eq!(
            name(ColumnType::MYSQL_TYPE_STRING, none, 12, 0, 33),
            "CHAR(4)"
        );
        assert_eq!(
            name(ColumnType::MYSQL_TYPE_VAR_STRING, none, 16, 0, 63),
            "VARBINARY(16)"
        );
        assert_eq!(
            name(ColumnType::MYSQL_TYPE_BLOB, none, 262_140, 0, UTF8MB4),
            "TEXT"
        );
        assert_eq!(
            name(ColumnType::MYSQL_TYPE_BLOB, none, u32::MAX, 0, UTF8MB4),
            "LONGTEXT"
        );
        assert_eq!(
            name(ColumnType::MYSQL_TYPE_BLOB, none, 255, 0, 63),
            "TINYBLOB"
        );
        // Unknown character set: the length is left out.
        assert_eq!(
            name(ColumnType::MYSQL_TYPE_VAR_STRING, none, 80, 0, 2000),
            "VARCHAR"
        );
    }

    #[test]
    fn names_temporal_types_and_flags() {
        let none = ColumnFlags::empty();
        assert_eq!(
            name(ColumnType::MYSQL_TYPE_DATETIME, none, 26, 6, 63),
            "DATETIME(6)"
        );
        assert_eq!(
            name(ColumnType::MYSQL_TYPE_TIMESTAMP, none, 19, 0, 63),
            "TIMESTAMP"
        );
        assert_eq!(
            name(ColumnType::MYSQL_TYPE_TIME, none, 10, NOT_FIXED_DEC, 63),
            "TIME"
        );
        let enumeration = describe(
            ColumnType::MYSQL_TYPE_STRING,
            ColumnFlags::ENUM_FLAG,
            4,
            0,
            UTF8MB4,
        );
        assert_eq!(
            (enumeration.family, enumeration.name.as_str()),
            (TypeFamily::Enum, "ENUM")
        );
        assert_eq!(
            describe(ColumnType::MYSQL_TYPE_DATE, none, 10, 0, 63).family,
            TypeFamily::Date
        );
    }

    #[test]
    fn plan_type_keeps_size_sign_and_nullability() {
        // Measured on MySQL 8.4: `CAST(18446744073709551615 AS UNSIGNED)`.
        let unsigned_bigint = plan_type(
            ColumnType::MYSQL_TYPE_LONGLONG,
            ColumnFlags::NOT_NULL_FLAG | ColumnFlags::UNSIGNED_FLAG,
            21,
            0,
            63,
        );
        assert_eq!(
            unsigned_bigint,
            Type::not_null(DataType::Integer(IntegerType::unsigned(IntegerSize::Big)))
        );
        let tiny = plan_type(ColumnType::MYSQL_TYPE_TINY, ColumnFlags::empty(), 4, 0, 63);
        assert_eq!(
            tiny,
            Type::nullable(DataType::Integer(IntegerType::signed(IntegerSize::Tiny)))
        );
    }

    #[test]
    fn plan_type_reads_decimals_and_fractions() {
        // Measured on MySQL 8.4 and MariaDB: `1 / 3` is a DECIMAL with 4 decimals, length 7.
        assert_eq!(
            plan_type(
                ColumnType::MYSQL_TYPE_NEWDECIMAL,
                ColumnFlags::empty(),
                7,
                4,
                63
            )
            .data_type,
            DataType::Decimal(DecimalType::new(5, 4).unwrap())
        );
        assert_eq!(
            plan_type(
                ColumnType::MYSQL_TYPE_DATETIME,
                ColumnFlags::empty(),
                26,
                6,
                63
            )
            .data_type,
            DataType::DateTime { fraction: 6 }
        );
        assert_eq!(
            plan_type(
                ColumnType::MYSQL_TYPE_TIME,
                ColumnFlags::empty(),
                10,
                NOT_FIXED_DEC,
                63
            )
            .data_type,
            DataType::Time { fraction: 6 }
        );
    }

    #[test]
    fn plan_type_tells_texts_from_bytes() {
        let none = ColumnFlags::empty();
        assert_eq!(
            plan_type(ColumnType::MYSQL_TYPE_VAR_STRING, none, 80, 0, UTF8MB4).data_type,
            DataType::Text(TextType {
                length: Some(20),
                fixed: false,
                charset: None,
                collation: None,
            })
        );
        assert_eq!(
            plan_type(ColumnType::MYSQL_TYPE_STRING, none, 16, 0, 63).data_type,
            DataType::Binary(BinaryType {
                length: Some(16),
                fixed: true,
            })
        );
        assert_eq!(
            plan_type(
                ColumnType::MYSQL_TYPE_STRING,
                ColumnFlags::SET_FLAG,
                8,
                0,
                UTF8MB4
            )
            .data_type,
            DataType::Set
        );
        assert_eq!(
            plan_type(ColumnType::MYSQL_TYPE_NULL, none, 0, 0, 63).data_type,
            DataType::Null
        );
    }
}
