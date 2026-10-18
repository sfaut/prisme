//! `ORDER BY` of a grouped `SELECT` with window functions, towards MariaDB,
//! measured on each image of MySQL 8.0.11 to 9.3.0 and of MariaDB 10.5.29 to
//! 10.11.19 and 11.8.9 (`crates/probe/cases/grouped-window-order.sql`).
//!
//! MariaDB takes the order of the grouping for an `ORDER BY` that is a
//! prefix of `GROUP BY`, constants aside, in either direction, and drops the
//! sort when an index gives that order (`EXPLAIN` shows no `filesort` above
//! `window_functions_computation`): the rows come out in the order of the
//! last sort of the windows, in every version measured. With the primary
//! key `id`, `SELECT id, ROW_NUMBER() OVER (ORDER BY id DESC) AS r FROM t
//! GROUP BY id ORDER BY id` gives 4, 3, 2, 1, and `LIMIT 2` keeps 4 and 3;
//! MySQL sorts the rows by `id`. MDEV-30890, closed as incomplete, reports it
//! with `HAVING`.
//!
//! The index is a choice of the optimizer, which Prisme does not see. From
//! MySQL, an `ORDER BY` of grouping keys and constants alone takes a last
//! key, the position of the first item with a window function
//! (`ORDER BY id, 2`): that key orders only the rows of equal keys, and
//! MariaDB sorts the rows after the windows, `COUNT(*) OVER ()` included. A
//! derived table would sort them as well, but MariaDB refuses one that reads
//! the enclosing query, error 1054 (**[decided autonomously]**). From
//! MariaDB, the query keeps the text written, which its server sorts or not
//! according to its indexes.

use prisme_plan::{Expr, ExprKind, ProjectItem, SortKey};

use super::{Block, Emitter};
use crate::Dialect;
use crate::report::StepKind;

impl Emitter<'_> {
    /// Last key of the `ORDER BY` of `block` that keeps its order after the
    /// window functions towards MariaDB, from MySQL: the position of the
    /// first item with a window function, when the keys are grouping keys
    /// and constants alone.
    pub(super) fn grouped_window_order(&mut self, block: &Block<'_>) {
        let (Some(sort), Some(aggregate), Some(_)) = (block.sort, block.aggregate, block.window)
        else {
            return;
        };
        let towards_mariadb =
            self.source.dialect == Dialect::MySql && self.output.dialect == Dialect::MariaDb;
        if !towards_mariadb || aggregate.keys.is_empty() || aggregate.rollup {
            return;
        }
        let grouping =
            |key: &SortKey| self.key_column(&key.expr).is_some() || self.expression_key(&key.expr);
        if !sort.keys.iter().any(grouping)
            || !(sort.keys.iter()).all(|key| is_constant(&key.expr) || grouping(key))
        {
            return;
        }
        let mut position = 0;
        for item in &block.project.items {
            match item {
                ProjectItem::Wildcard(wildcard) => position += wildcard.columns.len(),
                ProjectItem::Expr(named) => {
                    position += 1;
                    if self.reads_computed(&named.expr, true) {
                        self.write(&format!(", {position}"));
                        let message = format!(
                            "{} takes the order of the grouping for an `ORDER BY` of grouping \
                             keys, and loses it to the window functions when an index gives it \
                             (`SELECT id, ROW_NUMBER() OVER (ORDER BY id DESC) AS r FROM t GROUP \
                             BY id ORDER BY id` gives the order of the window, `id` a primary \
                             key): emulated by a last key on the item {position}, of a window \
                             function",
                            self.target()
                        );
                        let span = sort.keys.first().and_then(|key| key.expr.span);
                        self.step("order.grouped-windows", StepKind::Emulation, message, span);
                        return;
                    }
                }
            }
        }
    }
}

/// Tells whether `expr` is a constant, which MariaDB drops from an
/// `ORDER BY`.
fn is_constant(expr: &Expr) -> bool {
    matches!(expr.kind, ExprKind::Literal(_))
}
