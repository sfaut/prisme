//! Data types of the plan, and `Type`, a data type with its nullability.
//!
//! A data type keeps the declared form the emitter needs to reproduce the
//! result type of a query: `INT UNSIGNED` stays a 4-byte unsigned integer,
//! `DECIMAL(10,2)` keeps its precision and scale. The limits that depend on a
//! dialect (a scale of 30 at most in MySQL, 38 in MariaDB) are checked by the
//! dialects, not here.

use crate::TypeError;

/// Largest decimal precision, the same in MySQL and MariaDB.
pub const MAX_DECIMAL_PRECISION: u8 = 65;

/// Largest number of digits of the fractional seconds of a time.
pub const MAX_FRACTION_DIGITS: u8 = 6;

/// Type of a value, its nullability aside.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DataType {
    /// Type of the `NULL` literal, before a context gives it another type.
    Null,
    /// Three-valued boolean: true, false, unknown (NULL). An integer in the
    /// MySQL family, where the plan converts it explicitly
    /// (`docs/FEDERATED.md`, section 7, point 2).
    Boolean,
    /// Integer, 1 to 8 bytes, signed or not.
    Integer(IntegerType),
    /// Exact decimal.
    Decimal(DecimalType),
    /// Floating point number.
    Float(FloatType),
    /// Character string.
    Text(TextType),
    /// Byte string.
    Binary(BinaryType),
    /// Hexadecimal or bit literal (`0x41`, `b'1000001'`, and `X'41'` in
    /// MySQL): a byte string read as a string, an integer read as a number
    /// (`0x41 = 'A'` and `0x41 = 65` are both true). A value of this type
    /// stored or merged with another one (a column of a derived table,
    /// `MIN`, `CASE`) is a byte string.
    Hybrid {
        /// Number of bytes.
        length: u32,
        /// Tells whether it reads as a signed integer (a bit literal of
        /// MySQL: `b'101' + 0` is an `INT`), unsigned otherwise.
        signed: bool,
    },
    /// Calendar date.
    Date,
    /// Time of day or duration (`TIME` of MySQL ranges over ±838 hours).
    Time {
        /// Digits of the fractional seconds, 0 to `MAX_FRACTION_DIGITS`.
        fraction: u8,
    },
    /// Local date and time, without time zone.
    DateTime {
        /// Digits of the fractional seconds, 0 to `MAX_FRACTION_DIGITS`.
        fraction: u8,
    },
    /// Instant, shown in the time zone of the session.
    Timestamp {
        /// Digits of the fractional seconds, 0 to `MAX_FRACTION_DIGITS`.
        fraction: u8,
    },
    /// Year (`YEAR` of MySQL).
    Year,
    /// JSON document: a native type in MySQL, a checked `LONGTEXT` in MariaDB.
    Json,
    /// Bit field (`BIT(n)` of MySQL).
    Bit {
        /// Number of bits, 1 to 64.
        width: u8,
    },
    /// One value out of a declared list (`ENUM` of MySQL).
    Enum,
    /// Several values out of a declared list (`SET` of MySQL).
    Set,
    /// Geometry (`GEOMETRY` of MySQL and its derivatives).
    Geometry,
    /// Native type without a model in Prisme, kept by its name: `VECTOR`,
    /// `UUID` or `INET6` of MariaDB...
    Other(String),
    /// Type not known yet: the plan asks the server for it before
    /// transpiling (`docs/QUESTIONS.md`, point 44).
    Unknown,
}

/// Integer type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct IntegerType {
    /// Storage size.
    pub size: IntegerSize,
    /// `true` for an `UNSIGNED` integer.
    pub unsigned: bool,
}

/// Storage size of an integer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IntegerSize {
    /// 1 byte (`TINYINT`).
    Tiny,
    /// 2 bytes (`SMALLINT`).
    Small,
    /// 3 bytes (`MEDIUMINT`).
    Medium,
    /// 4 bytes (`INT`).
    Int,
    /// 8 bytes (`BIGINT`).
    Big,
}

/// Exact decimal type, `DECIMAL(precision, scale)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DecimalType {
    /// Total number of digits.
    precision: u8,
    /// Number of digits after the decimal point.
    scale: u8,
}

/// Floating point type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FloatType {
    /// Single precision (`FLOAT`).
    Single,
    /// Double precision (`DOUBLE`).
    Double,
}

/// Character string type.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TextType {
    /// Maximum length in characters; `None` when unknown.
    pub length: Option<u32>,
    /// `true` for a fixed length (`CHAR`), `false` for a variable one
    /// (`VARCHAR`, `TEXT`).
    pub fixed: bool,
    /// Character set (`utf8mb4`...); `None` when unknown.
    pub charset: Option<String>,
    /// Collation (`utf8mb4_0900_ai_ci`...), which the server applies to the
    /// comparisons; `None` when unknown.
    pub collation: Option<String>,
}

/// Byte string type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BinaryType {
    /// Maximum length in bytes; `None` when unknown.
    pub length: Option<u32>,
    /// `true` for a fixed length (`BINARY`), `false` for a variable one
    /// (`VARBINARY`, `BLOB`).
    pub fixed: bool,
}

/// Type of a value with its nullability: the type of a column of a schema or
/// of an expression of the plan.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Type {
    /// Data type.
    pub data_type: DataType,
    /// `true` when the value can be NULL.
    pub nullable: bool,
}

impl DataType {
    /// Type of a value of this type once stored, as a column of a derived
    /// table or of a common table expression: a hybrid is a byte string
    /// (`(SELECT v + 0 FROM (SELECT 0x41 AS v) d)` gives 0), any other type
    /// itself.
    pub fn stored(&self) -> DataType {
        match self {
            Self::Hybrid { length, .. } => Self::Binary(BinaryType {
                length: Some(*length),
                fixed: false,
            }),
            _ => self.clone(),
        }
    }
}

impl IntegerSize {
    /// Storage size in bytes.
    pub const fn bytes(self) -> u8 {
        match self {
            Self::Tiny => 1,
            Self::Small => 2,
            Self::Medium => 3,
            Self::Int => 4,
            Self::Big => 8,
        }
    }
}

impl IntegerType {
    /// Signed integer of the given size.
    pub const fn signed(size: IntegerSize) -> Self {
        Self {
            size,
            unsigned: false,
        }
    }

    /// Unsigned integer of the given size.
    pub const fn unsigned(size: IntegerSize) -> Self {
        Self {
            size,
            unsigned: true,
        }
    }

    /// Smallest value of the type.
    pub const fn min(self) -> i128 {
        if self.unsigned {
            0
        } else {
            -(1 << (self.bits() - 1))
        }
    }

    /// Largest value of the type.
    pub const fn max(self) -> i128 {
        if self.unsigned {
            (1 << self.bits()) - 1
        } else {
            (1 << (self.bits() - 1)) - 1
        }
    }

    /// `true` when `value` fits the type; otherwise the engine raises an
    /// "out of range" error (`docs/FEDERATED.md`, section 6).
    pub const fn contains(self, value: i128) -> bool {
        self.min() <= value && value <= self.max()
    }

    /// Number of bits of the type.
    const fn bits(self) -> u32 {
        self.size.bytes() as u32 * 8
    }
}

impl DecimalType {
    /// Decimal type of `precision` digits, `scale` of them after the decimal
    /// point.
    ///
    /// # Errors
    ///
    /// `TypeError::DecimalPrecision` outside 1 to `MAX_DECIMAL_PRECISION`,
    /// `TypeError::DecimalScale` when the scale exceeds the precision.
    pub const fn new(precision: u8, scale: u8) -> Result<Self, TypeError> {
        if precision == 0 || precision > MAX_DECIMAL_PRECISION {
            return Err(TypeError::DecimalPrecision(precision));
        }
        if scale > precision {
            return Err(TypeError::DecimalScale { precision, scale });
        }
        Ok(Self { precision, scale })
    }

    /// Total number of digits.
    pub const fn precision(self) -> u8 {
        self.precision
    }

    /// Number of digits after the decimal point.
    pub const fn scale(self) -> u8 {
        self.scale
    }
}

impl Type {
    /// Type whose values can be NULL.
    pub const fn nullable(data_type: DataType) -> Self {
        Self {
            data_type,
            nullable: true,
        }
    }

    /// Type whose values are never NULL.
    pub const fn not_null(data_type: DataType) -> Self {
        Self {
            data_type,
            nullable: false,
        }
    }

    /// Same data type, nullable or not.
    pub fn with_nullable(self, nullable: bool) -> Self {
        Self { nullable, ..self }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integer_bounds_follow_size_and_sign() {
        let tiny = IntegerType::signed(IntegerSize::Tiny);
        assert_eq!((tiny.min(), tiny.max()), (-128, 127));
        let medium = IntegerType::unsigned(IntegerSize::Medium);
        assert_eq!((medium.min(), medium.max()), (0, 16_777_215));
        let big = IntegerType::signed(IntegerSize::Big);
        assert_eq!(
            (big.min(), big.max()),
            (i128::from(i64::MIN), i128::from(i64::MAX))
        );
    }

    #[test]
    fn unsigned_bigint_fits_in_the_value_of_the_plan() {
        let big = IntegerType::unsigned(IntegerSize::Big);
        assert_eq!(big.max(), i128::from(u64::MAX));
        assert!(big.contains(18_446_744_073_709_551_615));
        // Measured: `18446744073709551615 + 1` is "out of range" on MySQL and MariaDB.
        assert!(!big.contains(18_446_744_073_709_551_616));
        // Measured: `CAST(1 AS UNSIGNED) - 2` is "out of range" as well.
        assert!(!big.contains(-1));
    }

    #[test]
    fn decimal_checks_precision_and_scale() {
        let money = DecimalType::new(10, 2).unwrap();
        assert_eq!((money.precision(), money.scale()), (10, 2));
        assert!(DecimalType::new(65, 38).is_ok());
        assert_eq!(DecimalType::new(0, 0), Err(TypeError::DecimalPrecision(0)));
        assert_eq!(
            DecimalType::new(66, 0),
            Err(TypeError::DecimalPrecision(66))
        );
        assert_eq!(
            DecimalType::new(5, 6),
            Err(TypeError::DecimalScale {
                precision: 5,
                scale: 6
            })
        );
    }

    #[test]
    fn type_carries_its_nullability() {
        let integer = DataType::Integer(IntegerType::signed(IntegerSize::Int));
        assert!(Type::nullable(integer.clone()).nullable);
        assert!(!Type::not_null(integer.clone()).nullable);
        assert_eq!(
            Type::not_null(integer.clone()).with_nullable(true),
            Type::nullable(integer)
        );
    }
}
