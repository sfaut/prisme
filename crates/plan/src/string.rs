//! Functions of strings whose arguments have a syntax of their own in the
//! MySQL family, with the form written: `TRIM`, of a side and of a pattern
//! before `FROM`, and `POSITION(substring IN string)`.

use crate::Expr;

/// `TRIM([[side] [pattern] FROM] operand)`: the string without the pattern
/// repeated at its start, at its end or at both.
#[derive(Debug, Clone, PartialEq)]
pub struct Trim {
    /// Side trimmed, when written; both sides otherwise.
    pub side: Option<TrimSide>,
    /// String removed, when written; a space otherwise.
    pub pattern: Option<Expr>,
    /// String trimmed.
    pub operand: Expr,
}

/// Side trimmed by `TRIM`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TrimSide {
    /// `BOTH`: the start and the end.
    Both,
    /// `LEADING`: the start.
    Leading,
    /// `TRAILING`: the end.
    Trailing,
}

impl TrimSide {
    /// Keyword of the side.
    pub const fn keyword(self) -> &'static str {
        match self {
            Self::Both => "BOTH",
            Self::Leading => "LEADING",
            Self::Trailing => "TRAILING",
        }
    }
}

/// `POSITION(substring IN string)`: the position of the first `substring` in
/// `string`, from 1, 0 when absent, as `LOCATE(substring, string)`.
#[derive(Debug, Clone, PartialEq)]
pub struct Position {
    /// String searched for.
    pub substring: Expr,
    /// String searched in.
    pub string: Expr,
}
