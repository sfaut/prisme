//! `CAST` and `CONVERT`: the type of each target, as MySQL and MariaDB give
//! it (`crates/probe/cases/cast-forms.txt`, measured on MySQL 8.0.11 to
//! 9.3.0, MariaDB 10.6.28 and 11.8.9).
//!
//! The integers are `BIGINT` (`INT` in the `CREATE TABLE ... AS SELECT` of
//! MySQL before 8.0.24, for an `INT` operand, of the same values), a
//! character string a `VARCHAR` of the length written, a byte string a
//! `VARBINARY`. A date or a time is nullable (an invalid value gives NULL),
//! as is a `DOUBLE` in MariaDB, and a character string read from bytes (MySQL
//! gives NULL for invalid ones from 8.0.28).

use prisme_plan::{Cast, CastForm, CastTarget, CharTarget, CharWord, Charset, Expr};
use prisme_types::{
    BinaryType, DataType, DecimalType, FloatType, IntegerSize, IntegerType, TextType, Type,
};

use super::scope::Scope;
use super::{BuildError, Builder};
use crate::Dialect;
use crate::sql::ast;

/// Precision of a `DECIMAL` written without one.
const DEFAULT_PRECISION: u8 = 10;

/// Length of the string of `CAST(x AS INTERVAL DAY_SECOND(n))` (MariaDB).
const INTERVAL_LENGTH: u32 = 20;

impl Builder<'_> {
    /// `CAST` or `CONVERT`, of the type of its target.
    pub(super) fn cast(&mut self, cast: &ast::CastCall, scope: &Scope) -> Result<Expr, BuildError> {
        let operand = self.value(&cast.operand, scope)?;
        let ty = cast_type(self.dialect, &cast.target, &cast.form, &operand.ty);
        let cast = Cast {
            operand,
            target: cast.target.clone(),
            form: cast.form.clone(),
        };
        Ok(Expr::cast(cast, ty))
    }
}

/// Type of the conversion of a value of type `operand` to `target`.
fn cast_type(dialect: Dialect, target: &CastTarget, form: &CastForm, operand: &Type) -> Type {
    let nullable = operand.nullable;
    let (data_type, nullable) = match target {
        CastTarget::Signed(_) | CastTarget::Integer(_) => (
            DataType::Integer(IntegerType::signed(IntegerSize::Big)),
            nullable,
        ),
        CastTarget::Unsigned(_) => (
            DataType::Integer(IntegerType::unsigned(IntegerSize::Big)),
            nullable,
        ),
        CastTarget::Decimal {
            precision, scale, ..
        } => {
            let precision = precision
                .filter(|precision| *precision > 0)
                .unwrap_or(DEFAULT_PRECISION);
            let decimal = DecimalType::new(precision, scale.unwrap_or(0))
                .expect("a precision and a scale checked by the parser");
            (DataType::Decimal(decimal), nullable)
        }
        CastTarget::Double { .. } | CastTarget::Real { float: false } => (
            DataType::Float(FloatType::Double),
            nullable || dialect == Dialect::MariaDb,
        ),
        CastTarget::Float(bits) => {
            let float = if bits.is_some_and(|bits| bits > 24) {
                FloatType::Double
            } else {
                FloatType::Single
            };
            (
                DataType::Float(float),
                nullable || dialect == Dialect::MariaDb,
            )
        }
        CastTarget::Real { float: true } => (DataType::Float(FloatType::Single), nullable),
        CastTarget::Char(char_target) => (
            text(char_target, form, &operand.data_type),
            nullable || reads_bytes(&operand.data_type),
        ),
        CastTarget::Binary(length) => (
            DataType::Binary(BinaryType {
                length: *length,
                fixed: false,
            }),
            nullable,
        ),
        CastTarget::Date => (DataType::Date, true),
        CastTarget::DateTime(fraction) => (
            DataType::DateTime {
                fraction: fraction.unwrap_or(0),
            },
            true,
        ),
        CastTarget::Time(fraction) => (
            DataType::Time {
                fraction: fraction.unwrap_or(0),
            },
            true,
        ),
        CastTarget::Json => (DataType::Json, nullable),
        CastTarget::Year => (DataType::Year, true),
        CastTarget::Spatial(_) => (DataType::Geometry, true),
        CastTarget::Interval(_) => (
            DataType::Text(TextType {
                length: Some(INTERVAL_LENGTH),
                fixed: false,
                charset: None,
                collation: None,
            }),
            nullable,
        ),
        CastTarget::Named(name) => (DataType::Other(name.clone()), true),
    };
    Type {
        data_type,
        nullable,
    }
}

/// Type of a character string target: a byte string for the character set
/// `binary`; for `CONVERT ... USING`, the length of a string operand.
fn text(target: &CharTarget, form: &CastForm, operand: &DataType) -> DataType {
    let charset = match (&target.charset, target.word) {
        (Some(charset), _) => Some(charset.name()),
        (None, CharWord::NChar | CharWord::NationalChar) => Some("utf8mb3".to_string()),
        (None, _) => None,
    };
    let length = match (form, operand) {
        (CastForm::ConvertUsing, DataType::Text(text)) => text.length,
        _ => target.length,
    };
    if target.charset == Some(Charset::Byte) || charset.as_deref() == Some("binary") {
        return DataType::Binary(BinaryType {
            length,
            fixed: false,
        });
    }
    DataType::Text(TextType {
        length,
        fixed: false,
        charset,
        collation: target.collation.clone(),
    })
}

/// Tells whether a value of type `data_type` may hold bytes that are not
/// characters of the target.
fn reads_bytes(data_type: &DataType) -> bool {
    matches!(
        data_type,
        DataType::Binary(_) | DataType::Unknown | DataType::Other(_) | DataType::Geometry
    )
}
