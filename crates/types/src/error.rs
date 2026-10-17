//! Errors raised when a type or a value breaks the rules of the model.

use std::fmt;

/// Type or value outside the model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeError {
    /// Decimal precision outside 1 to `MAX_DECIMAL_PRECISION`.
    DecimalPrecision(u8),
    /// Decimal scale above its precision.
    DecimalScale {
        /// Precision of the type.
        precision: u8,
        /// Scale asked for.
        scale: u8,
    },
    /// Text that is not an exact decimal number.
    DecimalText(String),
}

impl fmt::Display for TypeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DecimalPrecision(precision) => {
                write!(f, "decimal precision {precision} outside 1 to 65")
            }
            Self::DecimalScale { precision, scale } => {
                write!(f, "decimal scale {scale} above its precision {precision}")
            }
            Self::DecimalText(text) => write!(f, "{text:?} is not an exact decimal number"),
        }
    }
}

impl std::error::Error for TypeError {}
