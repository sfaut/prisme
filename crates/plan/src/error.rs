//! Errors raised when a node of the plan is built with inconsistent parts.

use std::fmt;

/// Node of the plan that cannot be built.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanError {
    /// Number of columns different from the expected one: rows of a
    /// `VALUES`, sides of a set operation, column list of an alias, subquery
    /// of a scalar or `IN` expression.
    ColumnCount {
        /// Number of columns expected.
        expected: usize,
        /// Number of columns found.
        found: usize,
    },
    /// Position outside the schema of the input.
    ColumnPosition {
        /// Position asked for, from 0.
        position: usize,
        /// Number of columns of the input.
        count: usize,
    },
}

impl fmt::Display for PlanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ColumnCount { expected, found } => {
                write!(f, "{found} columns where {expected} are expected")
            }
            Self::ColumnPosition { position, count } => {
                write!(f, "column {position} outside an input of {count} columns")
            }
        }
    }
}

impl std::error::Error for PlanError {}
