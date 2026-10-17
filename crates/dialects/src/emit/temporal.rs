//! Typed literals (`DATE '...'`, `TIME '...'`, `TIMESTAMP '...'` and their
//! ODBC form) towards MySQL and MariaDB, read as each dialect reads them
//! (`temporal` module).
//!
//! Within a dialect, the literal is written as is, a time zone offset
//! checked against the version of the server (rule `literal.offset`).
//! Between the dialects, the literal is written as is when the output reads
//! the same value or refuses it as the input does; as its value otherwise,
//! when the output reads that value the same (`TIME '07:08:09.1234567'`,
//! rounded by MySQL, truncated by MariaDB, written `TIME '07:08:09.123457'`
//! from MySQL) (rule `literal.temporal`). A value out of the range of the
//! output (`TIME '838:59:59.5'`, of MariaDB), a string one dialect refuses
//! and the other reads, a form Prisme does not read, and an offset, which
//! MySQL converts to the time zone of the session, are errors.

use prisme_plan::Expr;

use super::Emitter;
use crate::report::StepKind;
use crate::sql::ast::TemporalKind;
use crate::temporal::{self, Reading};
use crate::versions::Construct;

impl Emitter<'_> {
    /// Text of the typed literal `expr`, of `kind` and string `text`, when
    /// the output reads it otherwise; `None` to write it as is.
    pub(super) fn temporal_literal(
        &mut self,
        expr: &Expr,
        kind: TemporalKind,
        text: &str,
    ) -> Option<String> {
        let source = temporal::read(kind, text, self.source.dialect);
        let written = format!("`{} {}`", keyword(kind), self.string(text));
        if let Reading::Valid(value) = &source
            && value.offset.is_some()
            && !self.has(Construct::DatetimeOffset)
        {
            let message = format!(
                "{} has no time zone offset in a date and time literal: {written}, which {} converts to the time zone of the session",
                self.target(),
                self.source_name()
            );
            self.error("literal.offset", &message, expr.span);
            return None;
        }
        if self.source.dialect == self.output.dialect {
            return None;
        }
        let output = temporal::read(kind, text, self.output.dialect);
        let message = match (&source, &output) {
            (Reading::Valid(input), Reading::Valid(output)) if input == output => return None,
            (Reading::Invalid, Reading::Invalid) => return None,
            (Reading::Valid(input), _) => {
                let value = input.text();
                if temporal::read(kind, &value, self.output.dialect) == source {
                    let rewritten = format!("{} {}", keyword(kind), self.string(&value));
                    let message = format!(
                        "{written} is {value} in {}, read otherwise by {}: written `{rewritten}`, of the same value",
                        self.source_name(),
                        self.target()
                    );
                    self.step("literal.temporal", StepKind::Rewrite, message, expr.span);
                    return Some(rewritten);
                }
                format!(
                    "{written} is {value} in {}, out of the range of {}",
                    self.source_name(),
                    self.target()
                )
            }
            (Reading::Invalid, Reading::Valid(_)) => format!(
                "{written} is an error in {}, a value in {}",
                self.source_name(),
                self.target()
            ),
            _ => format!(
                "{written}: a form of string Prisme does not read, which the dialects may read differently"
            ),
        };
        self.error("literal.temporal", &message, expr.span);
        None
    }
}

/// Keyword of a typed literal of `kind`.
const fn keyword(kind: TemporalKind) -> &'static str {
    match kind {
        TemporalKind::Date => "DATE",
        TemporalKind::Time => "TIME",
        TemporalKind::Timestamp => "TIMESTAMP",
    }
}
