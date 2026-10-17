//! SQL dialects of the MySQL family: syntax tree and parser.
//!
//! One parser reads MySQL and MariaDB, set by the dialect, its version and
//! the SQL mode: each dialect stays in its domain, a construct absent from
//! the dialect or from the version chosen being an error
//! (`docs/DEV-DIALECTS.md`, section 1).

pub mod ast;
mod parser;

pub use parser::{ParseError, ParseOptions, Parsed, parse};
