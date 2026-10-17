//! Internal types and values of Prisme, shared by the logical plan, the
//! dialects and the connectors.
//!
//! A type keeps what the transpilation must reproduce: the declared size and
//! sign of an integer, the precision and scale of a decimal, the character set
//! and collation of a string, the fractional seconds of a time. Its
//! nullability travels with it (`Type`), as the plan derives it node by node
//! (rule 5 of `CLAUDE.md`).
//!
//! The decisions behind each type, with the behaviour measured on MySQL and
//! MariaDB, are in `docs/FEDERATED.md`, section 6.

mod data_type;
mod error;
mod value;

pub use data_type::{
    BinaryType, DataType, DecimalType, FloatType, IntegerSize, IntegerType, MAX_DECIMAL_PRECISION,
    MAX_FRACTION_DIGITS, TextType, Type,
};
pub use error::TypeError;
pub use value::{Decimal, Value};
