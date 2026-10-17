//! Entry point of the transpilation: a query written in an input dialect,
//! run by a server of an output dialect (`docs/DEV-DIALECTS.md`, section
//! 2.2).
//!
//! The processing has two stages, as the plan builder needs the columns of
//! the tables the query reads, which the caller reads on the connection
//! (`docs/QUESTIONS.md`, point 44): `prepare` reads the query and gives the
//! tables, `Prepared::transpile` builds the plan from their columns and
//! writes the output with its report.

use std::fmt;

use prisme_plan::{Span, TableName};

use crate::Dialect;
use crate::build::{BuildError, Catalog, build, referenced_tables};
use crate::emit::{OutputDialect, Source, emit};
use crate::report::{Step, StepKind, Transpilation};
use crate::sql::{ParseError, ParseOptions, Parsed, parse};
use crate::versions::Version;

/// Dialect the user writes in, with its version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputDialect {
    /// Dialect.
    pub dialect: Dialect,
    /// Version chosen.
    pub version: Version,
}

/// Query that cannot be transpiled: syntax error, construct absent from the
/// input dialect or not supported yet, unknown name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranspileError {
    /// What is wrong.
    pub message: String,
    /// Position in the input.
    pub span: Span,
}

impl fmt::Display for TranspileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for TranspileError {}

impl From<ParseError> for TranspileError {
    fn from(error: ParseError) -> Self {
        Self {
            message: error.message,
            span: error.span,
        }
    }
}

impl From<BuildError> for TranspileError {
    fn from(error: BuildError) -> Self {
        Self {
            message: error.message,
            span: error.span,
        }
    }
}

/// Query read, waiting for the columns of its tables.
#[derive(Debug, Clone)]
pub struct Prepared {
    /// Text of the query.
    text: String,
    /// Input dialect.
    input: InputDialect,
    /// Output dialect.
    output: OutputDialect,
    /// Tree and comments.
    parsed: Parsed,
}

/// Reads `text`, written in `input`, for a server of `output`. The SQL mode
/// of the session reads the input as it reads the output; the executable
/// comments are resolved against the version of the server when the input
/// dialect is that of the server, against the version of the input dialect
/// otherwise.
///
/// # Errors
///
/// `TranspileError` when the text is not a query of the input dialect in its
/// version, or uses a construct not supported yet.
pub fn prepare(
    text: &str,
    input: InputDialect,
    output: OutputDialect,
) -> Result<Prepared, TranspileError> {
    let conditional_version = if input.dialect == output.dialect {
        output.version
    } else {
        input.version
    };
    let options = ParseOptions {
        dialect: input.dialect,
        version: input.version,
        mode: output.mode,
        conditional_version,
    };
    let parsed = parse(text, &options)?;
    Ok(Prepared {
        text: text.to_string(),
        input,
        output,
        parsed,
    })
}

impl Prepared {
    /// Tables the query reads, whose columns `transpile` needs.
    pub fn tables(&self) -> Vec<TableName> {
        referenced_tables(&self.parsed.statement)
    }

    /// Builds the plan from the columns of `catalog`, then writes the query
    /// in the output dialect.
    ///
    /// # Errors
    ///
    /// `TranspileError` on an unknown table or column, or an ambiguous
    /// column.
    pub fn transpile(&self, catalog: &dyn Catalog) -> Result<Transpilation, TranspileError> {
        let plan = build(
            &self.parsed.statement,
            &self.text,
            self.input.dialect,
            self.input.version,
            catalog,
        )?;
        let source = Source {
            text: &self.text,
            mode: self.output.mode,
            comments: &self.parsed.comments,
        };
        let emitted = emit(&plan, &self.output, &source);
        let mut steps: Vec<Step> = self
            .parsed
            .conditionals
            .iter()
            .map(|&span| Step {
                rule: "executable-comment.code",
                kind: StepKind::Rewrite,
                message: "executable comment whose condition holds: its content is written as code"
                    .to_string(),
                span: Some(span),
            })
            .collect();
        steps.extend(emitted.steps);
        steps.sort_by_key(|step| step.span.map_or(usize::MAX, |span| span.start));
        Ok(Transpilation {
            sql: emitted.sql,
            steps,
            errors: emitted.errors,
        })
    }
}

/// Transpiles `text` in one go.
///
/// # Errors
///
/// See `prepare` and `Prepared::transpile`.
pub fn transpile(
    text: &str,
    input: InputDialect,
    output: OutputDialect,
    catalog: &dyn Catalog,
) -> Result<Transpilation, TranspileError> {
    prepare(text, input, output)?.transpile(catalog)
}
