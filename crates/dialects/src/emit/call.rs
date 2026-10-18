//! Function calls, aggregation and windows written in the MySQL family.
//!
//! Above the aggregation, the plan of a `SELECT` block reads columns the
//! block computes for itself (`#key1`, `#aggregate1`, `#window1`, see the
//! builder): each one is written as what it stands for, its grouping key,
//! its aggregate function or its window function. A grouping key comes from
//! the `GROUP BY` clause: written in the select list, it keeps the comments
//! of the input in their place.
//!
//! The aggregate and window functions are named by the reference files
//! (`builtin` module), with the forms MariaDB lacks emulated: an ordered
//! window for `RANK`, `LAG`...; the named windows written in the order
//! MariaDB reads them.

use prisme_plan::{
    Aggregate, AggregateCall, AliasForm, Column, ColumnRef, Expr, ExprKind, Frame, FrameBound,
    FrameUnits, Ident, NamedAggregate, NamedExpr, Project, ProjectItem, SortKey, SortReference,
    Span, WindowCall, WindowDefinition,
};
use prisme_types::{DataType, Value};

use super::json::reads_json_as_is;
use super::rules::repeatable;
use super::{Block, Emitter, result_position};
use crate::Dialect;
use crate::build::{functions, is_internal_name};
use crate::report::StepKind;

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
    /// Items of the select list written with an alias: alias and
    /// expression.
    aliases: Vec<(Ident, Expr)>,
    /// Columns of the source, with their table, when the block has window
    /// functions.
    columns: Vec<Column>,
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
        let aliases = (block.project.items.iter())
            .filter_map(|item| match item {
                ProjectItem::Expr(named) if named.alias != AliasForm::Implicit => {
                    Some((named.name.clone(), named.expr.clone()))
                }
                _ => None,
            })
            .collect();
        let columns = match block.window {
            Some(_) => block.source.schema().columns,
            None => Vec::new(),
        };
        Self {
            keys,
            aggregates,
            windows,
            definitions,
            aliases,
            columns,
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

    /// Named window `name`, when the block defines it.
    fn definition(&self, name: &Ident) -> Option<&WindowDefinition> {
        self.definitions
            .iter()
            .find(|definition| definition.name.value.eq_ignore_ascii_case(&name.value))
    }

    /// Tells whether the named window `name` has an order and a frame, those
    /// of the windows it completes included; `None` when unknown.
    fn resolved(&self, name: &Ident, depth: usize) -> Option<(bool, bool)> {
        if depth > self.definitions.len() {
            return None;
        }
        let definition = self.definition(name)?;
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

    /// Tells whether `expr` reads a column its `SELECT` block computes by a
    /// window function when `window`, by an aggregate function otherwise (an
    /// implicit `ANY_VALUE` aside), through its grouping keys too.
    pub(super) fn reads_computed(&self, expr: &Expr, window: bool) -> bool {
        let mut copy = expr.clone();
        self.reads_computed_mut(&mut copy, window)
    }

    /// Same as `reads_computed`, on the operands given by `operands_mut`.
    fn reads_computed_mut(&self, expr: &mut Expr, window: bool) -> bool {
        if let Some(substitute) = self.computed(expr) {
            return match substitute {
                Substitute::Aggregate(call) => !window && !call.implicit,
                Substitute::Expr(value) if matches!(value.kind, ExprKind::Window(_)) => window,
                Substitute::Expr(value) => self.reads_computed(&value, window),
            };
        }
        (expr.operands_mut().into_iter()).any(|operand| self.reads_computed_mut(operand, window))
    }

    /// Tells whether `expr` gives the same value each time it is written in
    /// its `SELECT` block, the columns the block computes read as what they
    /// stand for: an aggregate or window function of such arguments, over a
    /// window of the same definition, gives the same value written twice.
    pub(super) fn repeatable_computed(&self, expr: &Expr) -> bool {
        let mut copy = expr.clone();
        self.repeatable_computed_mut(&mut copy) && repeatable(&copy)
    }

    /// Checks the columns of `expr` its block computes, replaced by NULL for
    /// `repeatable` once checked.
    fn repeatable_computed_mut(&self, expr: &mut Expr) -> bool {
        if let Some(substitute) = self.computed(expr) {
            let checked = match substitute {
                Substitute::Aggregate(call) => {
                    call.args.iter().all(|arg| self.repeatable_computed(arg))
                        && (call.order_by.iter()).all(|key| self.repeatable_computed(&key.expr))
                }
                Substitute::Expr(value) => match &value.kind {
                    ExprKind::Window(call) => {
                        call.args.iter().all(|arg| self.repeatable_computed(arg))
                            && self.repeatable_window(
                                &call.partition_by,
                                &call.order_by,
                                call.window.as_ref(),
                                0,
                            )
                    }
                    _ => self.repeatable_computed(&value),
                },
            };
            *expr = Expr::literal(Value::Null, DataType::Null);
            return checked;
        }
        (expr.operands_mut().into_iter()).all(|operand| self.repeatable_computed_mut(operand))
    }

    /// Tells whether the window of `partition_by` and `order_by`, completing
    /// the named window `base`, partitions and sorts the rows by expressions
    /// of the same value at each evaluation, those of `base` included.
    fn repeatable_window(
        &self,
        partition_by: &[Expr],
        order_by: &[SortKey],
        base: Option<&Ident>,
        depth: usize,
    ) -> bool {
        if !partition_by
            .iter()
            .all(|value| self.repeatable_computed(value))
            || !order_by
                .iter()
                .all(|key| self.repeatable_computed(&key.expr))
        {
            return false;
        }
        let Some(base) = base else {
            return true;
        };
        let Some(block) = self.blocks.last() else {
            return false;
        };
        match block.definition(base) {
            Some(definition) if depth < block.definitions.len() => self.repeatable_window(
                &definition.partition_by,
                &definition.order_by,
                definition.base.as_ref(),
                depth + 1,
            ),
            _ => false,
        }
    }

    /// What `expr` stands for, when it is a column its `SELECT` block
    /// computes.
    fn computed(&self, expr: &Expr) -> Option<Substitute> {
        match &expr.kind {
            ExprKind::Column(column)
                if column.qualifier.is_none() && is_internal_name(&column.name.value) =>
            {
                self.blocks.last()?.substitute(&column.name)
            }
            _ => None,
        }
    }

    /// Items of the select list of the `SELECT` block written with an alias:
    /// alias and expression.
    pub(super) fn block_aliases(&self) -> &[(Ident, Expr)] {
        self.blocks.last().map_or(&[], |block| &block.aliases)
    }

    /// Columns of the source of the `SELECT` block, with their table, when
    /// it has window functions.
    pub(super) fn block_columns(&self) -> &[Column] {
        self.blocks.last().map_or(&[], |block| &block.columns)
    }

    /// Column of the source that `expr` stands for, when it is a column the
    /// `SELECT` block computes for a grouping key that is a column.
    pub(super) fn key_column(&self, expr: &Expr) -> Option<ColumnRef> {
        match self.computed(expr)? {
            Substitute::Expr(value) => match value.kind {
                ExprKind::Column(column) if !is_internal_name(&column.name.value) => Some(column),
                _ => None,
            },
            Substitute::Aggregate(_) => None,
        }
    }

    /// Tells whether `expr` is a column the `SELECT` block computes for a
    /// grouping key that is an expression other than a column.
    pub(super) fn expression_key(&self, expr: &Expr) -> bool {
        match self.computed(expr) {
            Some(Substitute::Expr(value)) => {
                !matches!(value.kind, ExprKind::Column(_) | ExprKind::Window(_))
            }
            _ => false,
        }
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

    /// Call to an aggregate function.
    pub(super) fn aggregate_call(&mut self, call: &AggregateCall) {
        let name = self.builtin_name(&call.function, call.span);
        let form = self.builtin_form(&name, &call.function, call.span);
        self.call_name(&name, form);
        if call.distinct {
            self.write("DISTINCT ");
        }
        let upper = call.function.value.to_ascii_uppercase();
        if BIT_AGGREGATES.contains(&upper.as_str()) {
            self.bit_aggregate(&upper, call.args.first());
        }
        // Without `DISTINCT`, `COUNT` and `GROUP_CONCAT` read the text of a
        // `JSON` in both dialects (`json` module).
        let json_as_is = !call.distinct && reads_json_as_is(&upper);
        let number = reads_numbers(&upper);
        if call.args.is_empty() {
            self.write("*");
        }
        for (index, arg) in call.args.iter().enumerate() {
            if index > 0 {
                self.write(", ");
            }
            self.json_as_is = json_as_is;
            if number {
                self.number_expr(arg, 0);
            } else {
                self.expr(arg, 0);
            }
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
        let name = self.builtin_name(&call.function, expr.span);
        let mariadb = self.output.dialect == Dialect::MariaDb;
        if mariadb && matches!(upper.as_str(), "LAG" | "LEAD") && call.args.len() > 2 {
            let message = format!(
                "{} has no default value in `{upper}`: not supported yet by Prisme",
                self.target()
            );
            self.error("window.default", &message, expr.span);
        }
        if BIT_AGGREGATES.contains(&upper.as_str()) {
            self.bit_aggregate(&upper, call.args.first());
            self.bit_window(&upper, call, expr.span);
        }
        self.write(&name);
        self.write("(");
        // `COUNT` reads the text of a `JSON` in both dialects (`json`
        // module).
        let json_as_is = reads_json_as_is(&upper);
        let number = reads_numbers(&upper);
        if call.args.is_empty() && upper == "COUNT" {
            self.write("*");
        }
        for (index, arg) in call.args.iter().enumerate() {
            if index > 0 {
                self.write(", ");
            }
            self.json_as_is = json_as_is;
            if number {
                self.number_expr(arg, 0);
            } else {
                self.expr(arg, 0);
            }
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
            let partition_by: Vec<Expr> = (partition_by.iter())
                .map(|value| {
                    let value = self.window_key(value);
                    self.sort_aliases(&value).into_owned()
                })
                .collect();
            self.expr_list(&partition_by);
        }
        if !order_by.is_empty() {
            separate(self);
            self.write("ORDER BY ");
            let order_by: Vec<SortKey> = (order_by.iter())
                .map(|key| SortKey {
                    expr: self.window_key(&key.expr).into_owned(),
                    ..key.clone()
                })
                .collect();
            self.sort_keys(&order_by, None);
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
    /// or an alias of the select list, or as an expression; `having` when
    /// the block has `HAVING`.
    pub(super) fn group_by(&mut self, aggregate: &Aggregate, project: &Project, having: bool) {
        if aggregate.keys.is_empty() {
            return;
        }
        self.at(aggregate.keys[0].expr.span);
        self.clause(" GROUP BY ");
        for (index, key) in aggregate.keys.iter().enumerate() {
            if index > 0 {
                self.write(", ");
            }
            let found = project.items.iter().find_map(|item| match item {
                ProjectItem::Expr(named) if reads_column(&named.expr, &key.name) => Some(named),
                _ => None,
            });
            let reference = aggregate
                .references
                .get(index)
                .copied()
                .unwrap_or(SortReference::Expression);
            let position = (reference == SortReference::Position)
                .then(|| {
                    result_position(
                        project,
                        |expr| reads_column(expr, &key.name),
                        |column| column.qualifier.is_none() && column.name == key.name,
                    )
                })
                .flatten();
            match (reference, found, position) {
                (SortReference::Position, _, Some(position)) => {
                    self.write(&position.to_string());
                }
                (SortReference::Alias, Some(named), _)
                    if named.alias != prisme_plan::AliasForm::Implicit =>
                {
                    self.ident(&named.name, key.expr.span);
                }
                _ => {
                    self.column = true;
                    self.expr(&key.expr, 0);
                }
            }
            if having {
                self.json_having_key(&key.expr, aggregate.rollup);
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

/// Aggregate functions of the bits of their argument, which MySQL computes
/// byte by byte for a byte string (`bitwise` module).
const BIT_AGGREGATES: [&str; 3] = ["BIT_AND", "BIT_OR", "BIT_XOR"];

/// Tells whether the aggregate or window function `name` (in capitals) reads
/// its arguments as numbers: they keep the scale of a `DECIMAL` `CASE`
/// (measured with `SUM`, `AVG`, `MIN`, `MAX`, `FIRST_VALUE` and `LAG`).
fn reads_numbers(name: &str) -> bool {
    matches!(
        name,
        "AVG"
            | "BIT_AND"
            | "BIT_OR"
            | "BIT_XOR"
            | "COUNT"
            | "FIRST_VALUE"
            | "LAG"
            | "LAST_VALUE"
            | "LEAD"
            | "MAX"
            | "MIN"
            | "NTH_VALUE"
            | "STD"
            | "STDDEV"
            | "STDDEV_POP"
            | "STDDEV_SAMP"
            | "SUM"
            | "VARIANCE"
            | "VAR_POP"
            | "VAR_SAMP"
    )
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
