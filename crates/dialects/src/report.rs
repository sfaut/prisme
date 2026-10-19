//! Report of a transpilation: every difference between the input and the
//! output, and every construct the output dialect cannot express
//! (`docs/DEV-DIALECTS.md`, sections 1 and 2.2).
//!
//! Each step names its rule by a stable identifier, cited by its test. The
//! lexical differences are not steps: case of the keywords, spaces, quote
//! character of an identifier, synonyms of an operator (`!=` and `<>`, `&&`
//! and `AND`), canonical spelling of a literal of the same value.

use prisme_plan::Span;

/// Result of a transpilation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transpilation {
    /// Query in the output dialect; to run only without `errors`.
    pub sql: String,
    /// Rewrites, emulations and losses of precision.
    pub steps: Vec<Step>,
    /// Constructs the output dialect cannot express.
    pub errors: Vec<Untranslatable>,
}

/// Difference between the input and the output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    /// Identifier of the rule, stable.
    pub rule: &'static str,
    /// Kind of difference.
    pub kind: StepKind,
    /// What changed and why.
    pub message: String,
    /// Position in the input of the construct changed.
    pub span: Option<Span>,
}

/// Kind of a step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StepKind {
    /// Another form of the same meaning.
    Rewrite,
    /// A construct written with other means of the output dialect.
    Emulation,
    /// The output gives less than the input (a type it gives no expression
    /// of, the `YEAR` of MySQL an `INT` in MariaDB), or than the same request
    /// written in the output dialect (a catalog statement).
    LossOfPrecision,
    /// A construct without effect on the result, left out.
    Omission,
}

/// Construct the output dialect cannot express.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Untranslatable {
    /// Identifier of the rule, stable.
    pub rule: &'static str,
    /// Why.
    pub message: String,
    /// Position in the input.
    pub span: Option<Span>,
}
