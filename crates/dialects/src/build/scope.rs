//! Columns visible to the expressions of a query block, and the resolution
//! of the names written.
//!
//! Column names are compared without case, as in the MySQL family. The
//! qualifier is compared without case as well: the case of table names
//! depends on a setting of the server (`lower_case_table_names`), and the
//! name keeps the case written, which the server then judges.

use prisme_plan::{Column, ColumnRef, Expr, Ident, Schema};
use prisme_types::Type;

use super::Output;
use crate::sql::ast::ObjectName;

/// Columns visible in a query block.
#[derive(Debug, Clone, Default)]
pub(super) struct Scope {
    /// Columns of the source, in order.
    columns: Vec<Column>,
}

/// Name that designates no column, or several.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ResolveError {
    /// No column has this name.
    Unknown,
    /// Several columns have this name.
    Ambiguous,
}

impl Scope {
    /// Columns of a source.
    pub(super) fn new(schema: Schema) -> Self {
        Self {
            columns: schema.columns,
        }
    }

    /// No column: the values of a `VALUES` or of a `LIMIT`.
    pub(super) fn empty() -> Self {
        Self::default()
    }

    /// Number of columns.
    pub(super) fn len(&self) -> usize {
        self.columns.len()
    }

    /// Column designated by `name`: `c`, `t.c` or `db.t.c`.
    pub(super) fn resolve(&self, name: &ObjectName) -> Result<(ColumnRef, Type), ResolveError> {
        let (qualifier, column) = match name.parts.as_slice() {
            [column] => (None, column),
            [.., table, column] => (Some(table), column),
            [] => return Err(ResolveError::Unknown),
        };
        let mut found = self.columns.iter().filter(|candidate| {
            candidate.name.value.eq_ignore_ascii_case(&column.value)
                && qualifier.is_none_or(|qualifier| matches_qualifier(candidate, qualifier))
        });
        let first = found.next().ok_or(ResolveError::Unknown)?;
        if found.next().is_some() {
            return Err(ResolveError::Ambiguous);
        }
        let reference = ColumnRef {
            qualifier: qualifier.cloned(),
            name: column.clone(),
        };
        Ok((reference, first.ty.clone()))
    }

    /// Positions of the columns of `*` (all of them) or of `t.*`; `None`
    /// when `t` designates no column.
    pub(super) fn wildcard(&self, qualifier: Option<&Ident>) -> Option<Vec<usize>> {
        let positions: Vec<usize> = (0..self.columns.len())
            .filter(|&position| {
                qualifier
                    .is_none_or(|qualifier| matches_qualifier(&self.columns[position], qualifier))
            })
            .collect();
        (qualifier.is_none() || !positions.is_empty()).then_some(positions)
    }

    /// Column at `position`, as an output of the select list without alias.
    pub(super) fn output(&self, position: usize) -> Output {
        let column = &self.columns[position];
        let reference = ColumnRef {
            qualifier: column.qualifier.clone(),
            name: column.name.clone(),
        };
        Output {
            alias: None,
            expr: Expr::column(reference, column.ty.clone()),
        }
    }
}

/// Tells whether `column` belongs to the table or alias `qualifier`.
fn matches_qualifier(column: &Column, qualifier: &Ident) -> bool {
    column
        .qualifier
        .as_ref()
        .is_some_and(|own| own.value.eq_ignore_ascii_case(&qualifier.value))
}
