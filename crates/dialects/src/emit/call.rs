//! Function calls, aggregation and windows written in the MySQL family.
//!
//! Above the aggregation, the plan of a `SELECT` block reads columns the
//! block computes for itself (`#key1`, `#aggregate1`, `#window1`, see the
//! builder): each one is written as what it stands for, its grouping key,
//! its aggregate function or its window function. A grouping key comes from
//! the `GROUP BY` clause: written in the select list, it keeps the comments
//! of the input in their place.
//!
//! The scalar functions are written as is towards their own dialect; towards
//! the other one, they wait for the function catalog of step 7
//! (`docs/DEV-DIALECTS.md`). The aggregate and window functions that MySQL and
//! MariaDB share go through, with the forms MariaDB lacks emulated: an
//! ordered window for `RANK`, `LAG`...; the named windows written in the
//! order MariaDB reads them.

use prisme_plan::{
    Aggregate, AggregateCall, Expr, Frame, FrameBound, FrameUnits, FunctionCall, Ident,
    NamedAggregate, NamedExpr, Project, ProjectItem, SortReference, Span, WindowCall,
    WindowDefinition,
};

use super::{Block, Emitter};
use crate::Dialect;
use crate::build::{functions, is_internal_name};
use crate::report::StepKind;
use crate::sql::ast::AGGREGATES;

/// Level of a bound of a frame: `1 PRECEDING` reads a value below the
/// comparisons.
const BOUND: u8 = 6;

/// Columns a `SELECT` block computes for itself, with what they stand for.
#[derive(Debug, Clone, Default)]
pub(super) struct Internal {
    /// Grouping keys.
    keys: Vec<NamedExpr>,
    /// Aggregate functions.
    aggregates: Vec<NamedAggregate>,
    /// Window functions.
    windows: Vec<NamedExpr>,
    /// Named windows.
    definitions: Vec<WindowDefinition>,
}

/// What a column of a block stands for.
enum Substitute {
    /// Grouping key or window function.
    Expr(Expr),
    /// Aggregate function.
    Aggregate(AggregateCall),
}

impl Internal {
    /// Columns computed by `block`.
    pub(super) fn new(block: &Block<'_>) -> Self {
        let (keys, aggregates) = block.aggregate.map_or_else(Default::default, |aggregate| {
            (aggregate.keys.clone(), aggregate.aggregates.clone())
        });
        let (windows, definitions) = block.window.map_or_else(Default::default, |window| {
            (window.functions.clone(), window.definitions.clone())
        });
        Self {
            keys,
            aggregates,
            windows,
            definitions,
        }
    }

    /// What the column `name` stands for.
    fn substitute(&self, name: &Ident) -> Option<Substitute> {
        let named = |list: &[NamedExpr]| {
            list.iter()
                .find(|named| named.name == *name)
                .map(|named| Substitute::Expr(named.expr.clone()))
        };
        named(&self.keys)
            .or_else(|| named(&self.windows))
            .or_else(|| {
                self.aggregates
                    .iter()
                    .find(|aggregate| aggregate.name == *name)
                    .map(|aggregate| Substitute::Aggregate(aggregate.call.clone()))
            })
    }

    /// Tells whether the named window `name` has an order and a frame, those
    /// of the windows it completes included; `None` when unknown.
    fn resolved(&self, name: &Ident, depth: usize) -> Option<(bool, bool)> {
        if depth > self.definitions.len() {
            return None;
        }
        let definition = self
            .definitions
            .iter()
            .find(|definition| definition.name.value.eq_ignore_ascii_case(&name.value))?;
        let ordered = !definition.order_by.is_empty()
            || match &definition.base {
                Some(base) => self.resolved(base, depth + 1)?.0,
                None => false,
            };
        Some((ordered, definition.frame.is_some()))
    }
}

impl Emitter<'_> {
    /// Writes the column `expr`, named `name`, that the `SELECT` block
    /// computes for itself, as what it stands for, at level `min` at least;
    /// `false` when the block computes no column of this name.
    pub(super) fn internal(&mut self, expr: &Expr, name: &Ident, min: u8) -> bool {
        let Some(substitute) = self.blocks.last().and_then(|block| block.substitute(name)) else {
            return false;
        };
        self.at(expr.span);
        // Written away from its place in the input, its comments wait for
        // their own place.
        let span = match &substitute {
            Substitute::Expr(value) => value.span,
            Substitute::Aggregate(call) => call.span,
        };
        let quiet = span != expr.span;
        if quiet {
            self.quiet += 1;
        }
        match substitute {
            Substitute::Expr(value) => self.expr(&value, min),
            Substitute::Aggregate(call) if call.implicit => self.expr(&call.args[0], min),
            Substitute::Aggregate(call) => self.aggregate_call(&call),
        }
        if quiet {
            self.quiet -= 1;
        }
        true
    }

    /// Value that names a column of the select list without alias: what a
    /// column the block computes stands for, when it is a grouping key or the
    /// column of an implicit `ANY_VALUE`; `expr` otherwise.
    pub(super) fn naming_value(&self, expr: &Expr) -> Expr {
        if let prisme_plan::ExprKind::Column(column) = &expr.kind
            && column.qualifier.is_none()
            && is_internal_name(&column.name.value)
            && let Some(substitute) = self
                .blocks
                .last()
                .and_then(|block| block.substitute(&column.name))
        {
            match substitute {
                Substitute::Expr(value)
                    if !matches!(value.kind, prisme_plan::ExprKind::Window(_)) =>
                {
                    return self.naming_value(&value);
                }
                Substitute::Aggregate(call) if call.implicit => {
                    return self.naming_value(&call.args[0]);
                }
                _ => {}
            }
        }
        expr.clone()
    }

    /// Refuses a function of the input dialect that the output dialect
    /// lacks or computes otherwise, `shared` telling whether both have it.
    fn function_dialect(&mut self, name: &Ident, shared: bool, span: Option<Span>) {
        if shared || self.source.dialect == self.output.dialect {
            return;
        }
        let message = format!(
            "function `{}` of {}: its translation to {} waits for the function catalog of Prisme",
            name.value,
            self.source.dialect.label(),
            self.output.dialect.label()
        );
        self.error("function.dialect", &message, span);
    }

    /// Call to a scalar function. A function stored under the name of an
    /// aggregate function keeps a space before `(`.
    pub(super) fn function_call(&mut self, expr: &Expr, call: &FunctionCall) {
        self.function_dialect(&call.name, false, expr.span);
        if let Some(qualifier) = &call.qualifier {
            self.ident(qualifier, expr.span);
            self.write(".");
        }
        if call.name.quoted {
            self.ident(&call.name, expr.span);
        } else {
            self.write(&call.name.value);
        }
        let upper = call.name.value.to_ascii_uppercase();
        let stored = call.qualifier.is_none() && AGGREGATES.contains(&upper.as_str());
        self.write(if stored { " (" } else { "(" });
        self.expr_list(&call.args);
        self.write(")");
    }

    /// Call to an aggregate function.
    pub(super) fn aggregate_call(&mut self, call: &AggregateCall) {
        let upper = call.function.value.to_ascii_uppercase();
        self.function_dialect(&call.function, functions::is_shared(&upper), call.span);
        self.write(&call.function.value);
        self.write("(");
        if call.distinct {
            self.write("DISTINCT ");
        }
        if call.args.is_empty() {
            self.write("*");
        } else {
            self.expr_list(&call.args);
        }
        if !call.order_by.is_empty() {
            self.write(" ORDER BY ");
            self.sort_keys(&call.order_by, None);
        }
        if let Some(separator) = &call.separator {
            self.write(" SEPARATOR ");
            let text = self.string(separator);
            self.write(&text);
        }
        self.write(")");
    }

    /// Call to a window function, with its window.
    pub(super) fn window_call(&mut self, expr: &Expr, call: &WindowCall) {
        let upper = call.function.value.to_ascii_uppercase();
        self.function_dialect(&call.function, functions::is_shared(&upper), expr.span);
        let mariadb = self.output.dialect == Dialect::MariaDb;
        if mariadb && matches!(upper.as_str(), "LAG" | "LEAD") && call.args.len() > 2 {
            let message = format!(
                "{} has no default value in `{upper}`: not supported yet by Prisme",
                self.target()
            );
            self.error("window.default", &message, expr.span);
        }
        self.write(&call.function.value);
        self.write("(");
        if call.args.is_empty() && upper == "COUNT" {
            self.write("*");
        } else {
            self.expr_list(&call.args);
        }
        self.write(") OVER ");
        let unordered = mariadb
            && functions::ORDERED_IN_MARIADB.contains(&upper.as_str())
            && call.order_by.is_empty();
        if unordered {
            let message = format!(
                "{} computes `{upper}` over an ordered window only: ordered by `NULL`, which keeps all the rows peers",
                self.target()
            );
            self.step("window.order", StepKind::Emulation, message, expr.span);
        }
        self.over(call, unordered, expr.span);
    }

    /// Window of a call: the named window it uses, or a specification that
    /// completes it, or its own specification. `unordered` adds
    /// `ORDER BY NULL` to a window without order.
    fn over(&mut self, call: &WindowCall, unordered: bool, span: Option<Span>) {
        let named = call.window.as_ref().and_then(|name| {
            self.blocks
                .last()
                .and_then(|block| block.resolved(name, 0))
                .map(|resolved| (name, resolved))
        });
        let Some((name, (ordered, framed))) = named else {
            self.write("(");
            self.window_parts(
                &call.partition_by,
                &call.order_by,
                call.frame.as_ref(),
                unordered,
            );
            self.write(")");
            return;
        };
        let order_by = if ordered { &[][..] } else { &call.order_by[..] };
        let frame = if framed { None } else { call.frame.as_ref() };
        if order_by.is_empty() && frame.is_none() && !unordered {
            self.ident(name, span);
            return;
        }
        if framed {
            let message = format!(
                "window `{}` has a frame: {} cannot complete it with an order",
                name.value,
                self.target()
            );
            self.error("window.order", &message, span);
        }
        self.write("(");
        self.ident(name, span);
        self.write(" ");
        self.window_parts(&[], order_by, frame, unordered);
        self.write(")");
    }

    /// `PARTITION BY ... ORDER BY ... frame`, the parts written.
    fn window_parts(
        &mut self,
        partition_by: &[Expr],
        order_by: &[prisme_plan::SortKey],
        frame: Option<&Frame>,
        unordered: bool,
    ) {
        let mut parts = 0;
        let mut separate = |emitter: &mut Self| {
            if parts > 0 {
                emitter.write(" ");
            }
            parts += 1;
        };
        if !partition_by.is_empty() {
            separate(self);
            self.write("PARTITION BY ");
            self.expr_list(partition_by);
        }
        if !order_by.is_empty() {
            separate(self);
            self.write("ORDER BY ");
            self.sort_keys(order_by, None);
        } else if unordered {
            separate(self);
            self.write("ORDER BY NULL");
        }
        if let Some(frame) = frame {
            separate(self);
            self.frame(frame);
        }
    }

    /// `ROWS` or `RANGE` frame.
    fn frame(&mut self, frame: &Frame) {
        self.write(match frame.units {
            FrameUnits::Rows => "ROWS ",
            FrameUnits::Range => "RANGE ",
        });
        match &frame.end {
            None => self.frame_bound(&frame.start),
            Some(end) => {
                self.write("BETWEEN ");
                self.frame_bound(&frame.start);
                self.write(" AND ");
                self.frame_bound(end);
            }
        }
    }

    /// Bound of a frame.
    fn frame_bound(&mut self, bound: &FrameBound) {
        match bound {
            FrameBound::UnboundedPreceding => self.write("UNBOUNDED PRECEDING"),
            FrameBound::Preceding(value) => {
                self.expr(value, BOUND);
                self.write(" PRECEDING");
            }
            FrameBound::CurrentRow => self.write("CURRENT ROW"),
            FrameBound::Following(value) => {
                self.expr(value, BOUND);
                self.write(" FOLLOWING");
            }
            FrameBound::UnboundedFollowing => self.write("UNBOUNDED FOLLOWING"),
        }
    }

    /// `GROUP BY`, each key designating its value as written: by a position
    /// or an alias of the select list, or as an expression.
    pub(super) fn group_by(&mut self, aggregate: &Aggregate, project: &Project) {
        if aggregate.keys.is_empty() {
            return;
        }
        self.at(aggregate.keys[0].expr.span);
        self.clause(" GROUP BY ");
        let all_expressions = project
            .items
            .iter()
            .all(|item| matches!(item, ProjectItem::Expr(_)));
        for (index, key) in aggregate.keys.iter().enumerate() {
            if index > 0 {
                self.write(", ");
            }
            let found = project
                .items
                .iter()
                .enumerate()
                .find_map(|(position, item)| match item {
                    ProjectItem::Expr(named) if reads_column(&named.expr, &key.name) => {
                        Some((position, named))
                    }
                    _ => None,
                });
            let reference = aggregate
                .references
                .get(index)
                .copied()
                .unwrap_or(SortReference::Expression);
            match (reference, found) {
                (SortReference::Position, Some((position, _))) if all_expressions => {
                    self.write(&(position + 1).to_string());
                }
                (SortReference::Alias, Some((_, named)))
                    if named.alias != prisme_plan::AliasForm::Implicit =>
                {
                    self.ident(&named.name, key.expr.span);
                }
                _ => self.expr(&key.expr, 0),
            }
        }
        if aggregate.rollup {
            self.write(" WITH ROLLUP");
        }
    }

    /// `WINDOW w AS (...), ...`, in an order where each window comes after
    /// the window it completes when the output is MariaDB.
    pub(super) fn window_clause(&mut self, definitions: &[WindowDefinition]) {
        if definitions.is_empty() {
            return;
        }
        let ordered = match self.output.dialect {
            Dialect::MySql => definitions.iter().collect(),
            Dialect::MariaDb => dependency_order(definitions),
        };
        if ordered
            .iter()
            .zip(definitions)
            .any(|(written, defined)| !std::ptr::eq(*written, defined))
        {
            let message = format!(
                "{} reads the window a definition completes before it: the named windows are written in this order",
                self.target()
            );
            self.step("window.definitions", StepKind::Rewrite, message, None);
        }
        self.clause(" WINDOW ");
        for (index, definition) in ordered.into_iter().enumerate() {
            if index > 0 {
                self.write(", ");
            }
            self.ident(&definition.name, None);
            self.write(" AS (");
            if let Some(base) = &definition.base {
                self.ident(base, None);
                if !definition.partition_by.is_empty()
                    || !definition.order_by.is_empty()
                    || definition.frame.is_some()
                {
                    self.write(" ");
                }
            }
            self.window_parts(
                &definition.partition_by,
                &definition.order_by,
                definition.frame.as_ref(),
                false,
            );
            self.write(")");
        }
    }
}

/// Tells whether `expr` is the column `name` a block computes for itself.
fn reads_column(expr: &Expr, name: &Ident) -> bool {
    matches!(&expr.kind, prisme_plan::ExprKind::Column(column)
        if column.qualifier.is_none() && column.name == *name)
}

/// Named windows in an order where each one comes after the window it
/// completes, the order written kept otherwise.
fn dependency_order(definitions: &[WindowDefinition]) -> Vec<&WindowDefinition> {
    let mut ordered: Vec<&WindowDefinition> = Vec::with_capacity(definitions.len());
    while ordered.len() < definitions.len() {
        let before = ordered.len();
        for definition in definitions {
            let written = ordered.iter().any(|done| std::ptr::eq(*done, definition));
            let ready = definition.base.as_ref().is_none_or(|base| {
                ordered
                    .iter()
                    .any(|done| done.name.value.eq_ignore_ascii_case(&base.value))
            });
            if !written && ready {
                ordered.push(definition);
            }
        }
        if ordered.len() == before {
            // A cycle, which the builder refuses: the rest as written.
            for definition in definitions {
                if !ordered.iter().any(|done| std::ptr::eq(*done, definition)) {
                    ordered.push(definition);
                }
            }
        }
    }
    ordered
}
