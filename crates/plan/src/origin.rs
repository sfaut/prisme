//! Origin of the nodes and expressions of the plan: their position in the
//! input and the form written by the user when several forms have the same
//! meaning (`docs/DEV-DIALECTS.md`, section 2.3).
//!
//! The emitter writes the original form when the output dialect accepts it;
//! otherwise it writes another form and records a step in the report. The
//! forms specific to a construct live on its node (`ValuesForm`,
//! `LimitForm`, `NotDistinctForm`, `SortForm`); the parsers add the others
//! as they read them.

/// Position of a construct in the input, in bytes: `start..end`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    /// First byte.
    pub start: usize,
    /// Byte after the last one.
    pub end: usize,
}

/// Identifier, as written.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Ident {
    /// Text of the identifier, quotes removed.
    pub value: String,
    /// `true` when written between quotes (`` `name` ``, `"name"`).
    pub quoted: bool,
}

/// How an alias was written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AliasForm {
    /// No alias written: the dialect gives the name.
    Implicit,
    /// `expression AS name`.
    As,
    /// `expression name`.
    Bare,
}

impl Span {
    /// Position from `start` to `end`.
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
}

impl Ident {
    /// Identifier written without quotes.
    pub fn new(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            quoted: false,
        }
    }

    /// Identifier written between quotes.
    pub fn quoted(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            quoted: true,
        }
    }
}
