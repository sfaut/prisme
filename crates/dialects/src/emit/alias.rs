//! Items of the select list read through their alias inside an expression
//! (`ORDER BY x + 0`, `HAVING x > 1`), written by their name where the
//! output reads it as the input does (`crates/probe/cases/alias-references.sql`,
//! measured on each local image of MySQL, 8.0.11 to 9.3.0, and on MariaDB
//! 10.5.29, 10.6.28, 10.11.19 and 11.8.9), by the expression of the item
//! otherwise:
//!
//! - towards MariaDB, an alias of an aggregate function inside an expression
//!   of `ORDER BY`, which MariaDB refuses (error 1247) and MySQL reads (rule
//!   `alias.aggregate`);
//! - towards MySQL 8.0.16 to 8.0.28, an alias of a window function inside an
//!   expression of `ORDER BY`, which these versions compute again for the
//!   sort (`ROW_NUMBER` gives 4 to the third row; Bug #32695670, fixed in
//!   8.0.29; rule `alias.window`);
//! - a `JSON` that the dialects read otherwise, for the rules of the `json`
//!   module.
//!
//! Written again, the expression must give the same value: an aggregate or
//! window function of the same arguments, over a window of the same
//! definition, computed over the same rows **[decided autonomously]**.
//!
//! A name alone in a window designates an alias of the select list first in
//! MariaDB, a column of the source only in MySQL: towards MySQL, the alias
//! is written as its expression (rule `alias.window-name`); towards MariaDB,
//! and towards MySQL before 8.0.14, which refuses it (Bug #27808099), a
//! column that an alias names is written with its table (rule
//! `alias.window-column`).
//!
//! In `HAVING`, MySQL reads a name of a grouping column before an alias of
//! the select list (warning 1052), MariaDB refuses it as ambiguous when the
//! alias is not that of an aggregate function or of the column itself
//! (error 1052): an error (`alias.having`). A grouping key is written in
//! `HAVING` as `GROUP BY` writes it: qualified (`t.b`), both read the
//! column.

use std::borrow::Cow;

use prisme_plan::{
    AliasForm, AliasRef, ColumnRef, Expr, ExprKind, Ident, Project, ProjectItem, Span,
};

use super::Emitter;
use crate::Dialect;
use crate::report::StepKind;
use crate::versions::Construct;

impl Emitter<'_> {
    /// Writes the item `alias` of the select list, read through its alias
    /// at `expr`, at level `min` at least: by its name, or by its
    /// expression when the dialects read the `JSON` of the item otherwise.
    pub(super) fn alias(&mut self, expr: &Expr, alias: &AliasRef, min: u8) {
        if self.json_differs(&alias.expr) {
            self.expr(&alias.expr, min);
            return;
        }
        self.column = false;
        self.number = false;
        self.json_as_is = false;
        self.as_bytes = false;
        self.at(expr.span);
        self.ident(&alias.name, expr.span);
    }

    /// Key `expr` of `ORDER BY`, the items of the select list that the output
    /// reads otherwise through their alias replaced by their expression.
    pub(super) fn sort_aliases<'e>(&mut self, expr: &'e Expr) -> Cow<'e, Expr> {
        if !self.reads_alias(expr) {
            return Cow::Borrowed(expr);
        }
        let mut copy = expr.clone();
        self.replace_aliases(&mut copy);
        Cow::Owned(copy)
    }

    /// Tells whether `expr` reads an item of the select list through its
    /// alias, outside its subqueries.
    fn reads_alias(&self, expr: &Expr) -> bool {
        let mut copy = expr.clone();
        reads_alias_mut(&mut copy)
    }

    /// Replaces in `expr` the items read through their alias that the output
    /// reads otherwise in `ORDER BY`.
    fn replace_aliases(&mut self, expr: &mut Expr) {
        if let ExprKind::Alias(alias) = &mut expr.kind {
            if let Some((rule, message)) = self.sort_alias_rule(alias) {
                if self.repeatable_computed(&alias.expr) {
                    self.step(rule, StepKind::Emulation, message, expr.span);
                    let span = expr.span;
                    *expr = alias.expr.clone();
                    expr.span = span.or(expr.span);
                } else {
                    let message = format!(
                        "{message}, which would compute it again: not supported yet by Prisme"
                    );
                    self.error(rule, &message, expr.span);
                }
            }
            return;
        }
        for operand in expr.operands_mut() {
            self.replace_aliases(operand);
        }
    }

    /// Rule and message of the item `alias`, read through its alias inside
    /// an expression of `ORDER BY`, that the output reads otherwise than the
    /// input.
    fn sort_alias_rule(&self, alias: &AliasRef) -> Option<(&'static str, String)> {
        let name = &alias.name.value;
        if self.source.dialect == Dialect::MySql
            && self.output.dialect == Dialect::MariaDb
            && self.reads_computed(&alias.expr, false)
        {
            let message = format!(
                "MariaDB refuses the alias of an aggregate function inside an expression of \
                 `ORDER BY` (error 1247): `{name}` written as its aggregate function"
            );
            return Some(("alias.aggregate", message));
        }
        if self.output.dialect == Dialect::MySql
            && !self.has(Construct::WindowAliasOnce)
            && self.input_has(Construct::WindowAliasOnce)
            && self.reads_computed(&alias.expr, true)
        {
            let message = format!(
                "{} computes again a window function read through its alias inside an \
                 expression of `ORDER BY` (Bug #32695670): `{name}` written as its window \
                 function",
                self.target()
            );
            return Some(("alias.window", message));
        }
        None
    }

    /// Key `expr` of a window, written so that the output reads it as the
    /// input: an alias read alone written as its expression towards MySQL, a
    /// column that an alias names written with its table towards MariaDB and
    /// MySQL before 8.0.14.
    pub(super) fn window_key<'e>(&mut self, expr: &'e Expr) -> Cow<'e, Expr> {
        if let ExprKind::Alias(alias) = &expr.kind {
            // The alias of the column of its name (`b AS b`) names it alike.
            let itself = matches!(&alias.expr.kind, ExprKind::Column(column)
                if column.name.value.eq_ignore_ascii_case(&alias.name.value));
            if self.output.dialect != Dialect::MySql || itself {
                return Cow::Borrowed(expr);
            }
            let message = format!(
                "MySQL reads a name alone in a window as a column of the source, MariaDB as \
                 an alias of the select list first: `{}` written as its expression",
                alias.name.value
            );
            if !self.repeatable_computed(&alias.expr) {
                let message =
                    format!("{message}, which would compute it again: not supported yet by Prisme");
                self.error("alias.window-name", &message, expr.span);
                return Cow::Borrowed(expr);
            }
            self.step("alias.window-name", StepKind::Rewrite, message, expr.span);
            let mut value = alias.expr.clone();
            value.span = expr.span.or(value.span);
            return Cow::Owned(value);
        }
        let Some(column) = self.shadowed_column(expr) else {
            return Cow::Borrowed(expr);
        };
        let Some(qualified) = self.qualified(&column) else {
            let message = format!(
                "{} reads `{}` alone in a window as an alias of the select list, and Prisme finds \
                 no single table of this column to name it: not supported yet by Prisme",
                self.target(),
                column.name.value
            );
            self.error("alias.window-column", &message, expr.span);
            return Cow::Borrowed(expr);
        };
        let message = match self.output.dialect {
            Dialect::MariaDb => format!(
                "MariaDB reads a name alone in a window as an alias of the select list first: \
                 the column `{}` written with its table",
                column.name.value
            ),
            Dialect::MySql => format!(
                "{} refuses a column named alone in a window when an alias of the select list \
                 has its name (error 1054, Bug #27808099): `{}` written with its table",
                self.target(),
                column.name.value
            ),
        };
        self.step("alias.window-column", StepKind::Rewrite, message, expr.span);
        let mut value = Expr::column(qualified, expr.ty.clone());
        value.span = expr.span;
        Cow::Owned(value)
    }

    /// Column written alone that `expr`, a key of a window, stands for, when
    /// an alias of the select list other than this column has its name and
    /// the output reads the alias or refuses the name.
    fn shadowed_column(&self, expr: &Expr) -> Option<ColumnRef> {
        if self.output.dialect == Dialect::MySql && self.has(Construct::WindowShadowedColumn) {
            return None;
        }
        let column = match &expr.kind {
            ExprKind::Column(column) if !crate::build::is_internal_name(&column.name.value) => {
                column.clone()
            }
            _ => self.key_column(expr)?,
        };
        if column.qualifier.is_some() {
            return None;
        }
        let aliases = self.block_aliases();
        let same = |value: &Expr| {
            let named = match &value.kind {
                ExprKind::Column(found) => Some(found.clone()),
                _ => self.key_column(value),
            };
            named.is_some_and(|found| found.name.value.eq_ignore_ascii_case(&column.name.value))
        };
        aliases
            .iter()
            .any(|(alias, value)| {
                alias.value.eq_ignore_ascii_case(&column.name.value) && !same(value)
            })
            .then_some(column)
    }

    /// `column`, named alone, with the table it belongs to in the source;
    /// `None` when no single table has a column of this name.
    fn qualified(&self, column: &ColumnRef) -> Option<ColumnRef> {
        let columns = self.block_columns();
        let mut found = columns.iter().filter(|found| {
            found.qualifier.is_some() && found.name.value.eq_ignore_ascii_case(&column.name.value)
        });
        let first = found.next()?;
        if found.next().is_some() {
            return None;
        }
        Some(ColumnRef {
            qualifier: first.qualifier.clone(),
            name: column.name.clone(),
        })
    }

    /// Refuses, towards MariaDB, a name of a grouping column written in
    /// `HAVING` without qualifier that an item of `project` other than an
    /// aggregate function or the column itself has as alias: MySQL reads the
    /// column, MariaDB refuses the name as ambiguous (error 1052).
    pub(super) fn having_aliases(&mut self, predicate: &Expr, project: &Project) {
        if self.source.dialect != Dialect::MySql || self.output.dialect != Dialect::MariaDb {
            return;
        }
        let mut copy = predicate.clone();
        let mut names = Vec::new();
        self.key_columns(&mut copy, &mut names);
        for (name, span) in names {
            let ambiguous = project.items.iter().any(|item| match item {
                ProjectItem::Expr(named) => {
                    named.alias != AliasForm::Implicit
                        && named.name.value.eq_ignore_ascii_case(&name.value)
                        && !self.key_column(&named.expr).is_some_and(|column| {
                            column.name.value.eq_ignore_ascii_case(&name.value)
                        })
                        && !self.reads_computed(&named.expr, false)
                }
                ProjectItem::Wildcard(_) => false,
            });
            if ambiguous {
                let message = format!(
                    "MariaDB reads `{}` in `HAVING` as ambiguous, the name of a grouping column \
                     and the alias of an item of the select list (error 1052): not supported yet \
                     by Prisme",
                    name.value
                );
                self.error("alias.having", &message, span);
            }
        }
    }

    /// Names of the grouping columns that `expr` reads outside its aggregate
    /// functions and its subqueries, written without qualifier, with their
    /// position.
    fn key_columns(&self, expr: &mut Expr, names: &mut Vec<(Ident, Option<Span>)>) {
        if let Some(column) = self.key_column(expr) {
            if column.qualifier.is_none() {
                names.push((column.name, expr.span));
            }
            return;
        }
        for operand in expr.operands_mut() {
            self.key_columns(operand, names);
        }
    }
}

/// Same as `Emitter::reads_alias`, on the operands given by `operands_mut`.
fn reads_alias_mut(expr: &mut Expr) -> bool {
    matches!(expr.kind, ExprKind::Alias(_))
        || (expr.operands_mut().into_iter()).any(reads_alias_mut)
}
