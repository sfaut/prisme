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
//! In `HAVING`, MySQL reads a name of a grouping column before an alias of
//! the select list (warning 1052), MariaDB refuses it as ambiguous when the
//! alias is not that of an aggregate function or of the column itself
//! (error 1052): an error (`alias.having`). A grouping key is written in
//! `HAVING` as `GROUP BY` writes it: qualified (`t.b`), both read the
//! column.

use std::borrow::Cow;

use prisme_plan::{AliasForm, AliasRef, Expr, ExprKind, Ident, Project, ProjectItem, Span};

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
