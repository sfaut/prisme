//! Grouping and windows of a `SELECT` block.
//!
//! A grouped block (`GROUP BY`, or an aggregate function in its select
//! list, its `HAVING` or its `ORDER BY`) gives
//! `Project(Sort(Window(Filter(Aggregate(...)))))`: the expressions above
//! the aggregation read its columns, named `#key1`, `#aggregate1`..., and
//! those of the window functions, `#window1`.... A part of an expression
//! equal to a grouping key reads the key; a column neither grouped nor
//! aggregated reads any value of its group, which the MySQL family allows
//! when the column depends on the keys, or without `ONLY_FULL_GROUP_BY`: an
//! implicit `ANY_VALUE`, written as the column alone. The emitter writes
//! each of these columns back as the expression it stands for.

use prisme_plan::{
    Aggregate, AggregateCall, AliasForm, ColumnRef, Expr, ExprKind, Ident, NamedAggregate,
    NamedExpr, OnEmpty, Plan, ProjectItem, SortKey, SortReference, Span, Window, WindowDefinition,
};
use prisme_types::{DataType, Type, Value};

use super::scope::{ResolveError, Scope};
use super::{BuildError, Builder, Output, same_name};
use crate::sql::ast::{self, Literal, ObjectName, OrderItem, Select, SelectItem};

/// Prefix of the names of the columns a block computes for itself.
pub(crate) const INTERNAL_PREFIX: char = '#';

/// Clause of a `SELECT` block being built.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum Clause {
    /// Outside the clauses that read aggregates: `FROM`, `LIMIT`, a
    /// `VALUES`.
    #[default]
    Other,
    /// `WHERE`.
    Where,
    /// `GROUP BY`.
    GroupBy,
    /// `WINDOW`.
    Window,
    /// Select list.
    Select,
    /// `HAVING`.
    Having,
    /// `ORDER BY`.
    OrderBy,
    /// Arguments of an aggregate function.
    Aggregate,
    /// Arguments and window of a window function.
    WindowFunction,
}

impl Clause {
    /// Name of the clause in the messages.
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::Other => "this clause",
            Self::Where => "`WHERE`",
            Self::GroupBy => "`GROUP BY`",
            Self::Window => "`WINDOW`",
            Self::Select => "the select list",
            Self::Having => "`HAVING`",
            Self::OrderBy => "`ORDER BY`",
            Self::Aggregate => "the arguments of an aggregate function",
            Self::WindowFunction => "the arguments of a window function",
        }
    }
}

/// State of a `SELECT` block being built.
#[derive(Debug, Default)]
pub(super) struct Block {
    /// Clause being built.
    pub(super) clause: Clause,
    /// The block aggregates its rows.
    pub(super) grouped: bool,
    /// The block has grouping keys.
    pub(super) keyed: bool,
    /// Columns of its source.
    pub(super) source: Scope,
    /// Aggregate functions read by the block.
    pub(super) aggregates: Vec<NamedAggregate>,
    /// The same, without positions, to read each one once.
    canonical: Vec<AggregateCall>,
    /// Window functions read by the block.
    pub(super) windows: Vec<NamedExpr>,
    /// Named windows of its `WINDOW` clause.
    pub(super) definitions: Vec<WindowDefinition>,
    /// Columns of its select list, for the aliases read by `GROUP BY`,
    /// `HAVING` and `ORDER BY`.
    pub(super) outputs: Vec<Output>,
    /// Clause whose names can designate an alias of the select list, inside
    /// its aggregate functions too: `GROUP BY`, `HAVING` or `ORDER BY`.
    aliases: Option<Clause>,
    /// Columns of the source that its grouping keys name, as the source
    /// names them: `HAVING` reads them before the aliases.
    pub(super) key_columns: Vec<ColumnRef>,
    /// Items of its select list written with an alias, by alias: the keys
    /// of its windows read them, known before the select list is built.
    pub(super) alias_items: Vec<(Ident, ast::Expr)>,
    /// An expression of a key of a window is being built: its names read a
    /// column of the source first, then an alias of the select list.
    pub(super) window_keys: bool,
}

/// Grouping key of a block, as the expressions above the aggregation see
/// it.
#[derive(Debug, Clone)]
pub(super) struct Key {
    /// Value, without positions and with its columns named as the source
    /// names them.
    canonical: Expr,
    /// Column of the aggregation that reads it.
    column: Expr,
}

impl Key {
    /// Key `index` (from 0) of value `expr`, NULL in the super-aggregate rows
    /// of `WITH ROLLUP`.
    pub(super) fn new(index: usize, expr: &Expr, source: &Scope, rollup: bool) -> Self {
        let ty = Type {
            data_type: expr.ty.data_type.clone(),
            nullable: expr.ty.nullable || rollup,
        };
        Self {
            canonical: canonical(expr, source),
            column: Expr::column(internal(&key_name(index)), ty),
        }
    }
}

/// Name of the column of key `index` (from 0).
pub(super) fn key_name(index: usize) -> Ident {
    Ident::new(format!("{INTERNAL_PREFIX}key{}", index + 1))
}

/// Reference to a column the block computes for itself.
fn internal(name: &Ident) -> ColumnRef {
    ColumnRef {
        qualifier: None,
        name: name.clone(),
    }
}

impl Block {
    /// Column that reads the aggregate function `call`, the same column for
    /// the same call written twice.
    pub(super) fn aggregate(&mut self, call: AggregateCall) -> Expr {
        let canonical = canonical_call(&call, &self.source);
        let index = match self.canonical.iter().position(|known| *known == canonical) {
            Some(index) => index,
            None => {
                self.canonical.push(canonical);
                let name = Ident::new(format!(
                    "{INTERNAL_PREFIX}aggregate{}",
                    self.aggregates.len() + 1
                ));
                self.aggregates.push(NamedAggregate {
                    call,
                    name,
                    alias: AliasForm::Implicit,
                });
                self.aggregates.len() - 1
            }
        };
        let aggregate = &self.aggregates[index];
        Expr::column(internal(&aggregate.name), aggregate.call.ty(self.keyed))
    }

    /// Column that reads the window function `expr`.
    pub(super) fn window(&mut self, expr: Expr) -> Expr {
        let name = Ident::new(format!("{INTERNAL_PREFIX}window{}", self.windows.len() + 1));
        let column = Expr::column(internal(&name), expr.ty.clone());
        self.windows.push(NamedExpr {
            expr,
            name,
            alias: AliasForm::As,
        });
        column
    }

    /// Rewrites `expr`, read above the aggregation: a part equal to a key
    /// reads the key, a column neither grouped nor aggregated reads any
    /// value of its group.
    pub(super) fn regroup(&mut self, expr: &mut Expr, keys: &[Key]) {
        if let ExprKind::Alias(alias) = &mut expr.kind {
            self.regroup(&mut alias.expr, keys);
            return;
        }
        let span = expr.span;
        let found = canonical(expr, &self.source);
        if let Some(key) = keys.iter().find(|key| key.canonical == found) {
            *expr = with_span(key.column.clone(), span);
            return;
        }
        if let ExprKind::Column(column) = &expr.kind
            && self.source.canonical(column).is_some()
        {
            let call = AggregateCall {
                function: Ident::new("ANY_VALUE"),
                args: vec![expr.clone()],
                distinct: false,
                order_by: Vec::new(),
                separator: None,
                implicit: true,
                data_type: expr.ty.data_type.clone(),
                on_empty: OnEmpty::Null,
                span,
            };
            *expr = with_span(self.aggregate(call), span);
            return;
        }
        for operand in expr.operands_mut() {
            self.regroup(operand, keys);
        }
    }

    /// Rewrites the window functions and the named windows of the block,
    /// read above the aggregation.
    pub(super) fn regroup_windows(&mut self, keys: &[Key]) {
        let mut windows = std::mem::take(&mut self.windows);
        for window in &mut windows {
            self.regroup(&mut window.expr, keys);
        }
        self.windows = windows;
        let mut definitions = std::mem::take(&mut self.definitions);
        for definition in &mut definitions {
            let values = definition
                .partition_by
                .iter_mut()
                .chain(definition.order_by.iter_mut().map(|key| &mut key.expr));
            for value in values {
                self.regroup(value, keys);
            }
        }
        self.definitions = definitions;
    }
}

/// Grouping keys built, each one with how it designates its value.
pub(super) type Keys = Vec<(Expr, SortReference)>;

/// Parts of a grouped block that read the aggregation.
pub(super) struct Above<'a> {
    /// Select list.
    pub(super) items: &'a mut [ProjectItem],
    /// `HAVING`.
    pub(super) having: Option<&'a mut Expr>,
    /// `ORDER BY`.
    pub(super) sort: &'a mut [SortKey],
}

impl Builder<'_> {
    /// State of the `SELECT` block being built.
    pub(super) fn block(&self) -> &Block {
        self.blocks.last().expect("a query is being built")
    }

    /// State of the `SELECT` block being built, to change.
    pub(super) fn block_mut(&mut self) -> &mut Block {
        self.blocks.last_mut().expect("a query is being built")
    }

    /// Enters `clause` of the block, and gives the clause it leaves.
    pub(super) fn set_clause(&mut self, clause: Clause) -> Clause {
        let block = self.block_mut();
        match clause {
            Clause::GroupBy | Clause::Having | Clause::OrderBy => block.aliases = Some(clause),
            Clause::Aggregate | Clause::WindowFunction => {}
            Clause::Other | Clause::Where | Clause::Window | Clause::Select => block.aliases = None,
        }
        std::mem::replace(&mut block.clause, clause)
    }

    /// Column designated by `name`, or the item of the select list whose
    /// alias it is (`alias_value`).
    pub(super) fn column(&mut self, name: &ObjectName, scope: &Scope) -> Result<Expr, BuildError> {
        if let [single] = name.parts.as_slice()
            && let Some(value) = self.alias_value(single, name, scope)?
        {
            return Ok(value);
        }
        let (column, ty) = self.resolve(name, scope)?;
        if column.qualifier.is_none() && is_internal_name(&column.name.value) {
            let message = format!(
                "column `{}`: the names `#key...`, `#aggregate...` and `#window...` are reserved by Prisme",
                column.name.value
            );
            return Err(self.error(message, name.span));
        }
        Ok(Expr::column(column, ty))
    }

    /// Item of the select list that the name `single` designates inside an
    /// expression of `GROUP BY`, `HAVING`, `ORDER BY` or of a key of a
    /// window, or of one of their aggregate functions, by the rule of the
    /// MySQL family (`crates/probe/cases/alias-references.sql`): a name that
    /// no column of the source has (`ORDER BY x + 0`), and in `HAVING`,
    /// outside an aggregate function, a name that no grouping key names
    /// (`GROUP BY a % 2 HAVING b = 1` reads the alias `b`). A name written
    /// alone in `GROUP BY`, `ORDER BY` and a window is read by `group_value`,
    /// `sort_value` and `window_key`.
    fn alias_value(
        &mut self,
        single: &Ident,
        name: &ObjectName,
        scope: &Scope,
    ) -> Result<Option<Expr>, BuildError> {
        let block = self.block();
        if block.window_keys && matches!(block.clause, Clause::Window | Clause::WindowFunction) {
            if scope.resolve(name) != Err(ResolveError::Unknown) {
                return Ok(None);
            }
            return self.window_alias(single, name.span, scope);
        }
        let (Some(aliases), clause) = (block.aliases, block.clause) else {
            return Ok(None);
        };
        if clause == Clause::WindowFunction {
            return Ok(None);
        }
        match scope.resolve(name) {
            Ok((column, _)) => {
                let grouped = scope
                    .canonical(&column)
                    .is_some_and(|column| block.key_columns.contains(&column));
                if aliases != Clause::Having || clause != Clause::Having || grouped {
                    return Ok(None);
                }
            }
            Err(ResolveError::Ambiguous) => return Ok(None),
            Err(ResolveError::Unknown) => {}
        }
        let Some(output) = self.alias(single, name.span)? else {
            return Ok(None);
        };
        let value = output.expr.clone();
        if clause == Clause::Aggregate && reads_internal(&value, &String::from(INTERNAL_PREFIX)) {
            let message = format!(
                "an aggregate function of `{}`, the alias of an aggregate or window function",
                single.value
            );
            return Err(self.error(message, name.span));
        }
        Ok(Some(
            Expr::alias(single.clone(), value).with_span(name.span),
        ))
    }

    /// Column of the select list whose alias is `name`; an error when
    /// several have it (error 1052 in both dialects).
    fn alias(&self, name: &Ident, span: Span) -> Result<Option<&Output>, BuildError> {
        let mut found = self.block().outputs.iter().filter(|output| {
            output
                .alias
                .as_ref()
                .is_some_and(|alias| same_name(alias, name))
        });
        let output = found.next();
        if found.next().is_some() {
            let message = format!(
                "`{}` is ambiguous: several items of the select list have this alias",
                name.value
            );
            return Err(self.error(message, span));
        }
        Ok(output)
    }

    /// Keys of a `GROUP BY`: a position of the select list, a column of the
    /// source, an alias of the select list, or an expression of the source,
    /// in this order (the rule of the MySQL family).
    pub(super) fn group_keys(
        &mut self,
        items: &[ast::Expr],
        scope: &Scope,
        outputs: &[Output],
    ) -> Result<Keys, BuildError> {
        items
            .iter()
            .map(|item| {
                let (expr, reference) = self.group_value(item, scope, outputs)?;
                if reads_internal(&expr, &String::from(INTERNAL_PREFIX)) {
                    let message = format!(
                        "grouping on `{}`, which reads an aggregate or window function",
                        self.written(item.span)
                    );
                    return Err(self.error(message, item.span));
                }
                Ok((expr, reference))
            })
            .collect()
    }

    /// Value of a grouping key, and how it designates it.
    fn group_value(
        &mut self,
        item: &ast::Expr,
        scope: &Scope,
        outputs: &[Output],
    ) -> Result<(Expr, SortReference), BuildError> {
        match &item.kind {
            ast::ExprKind::Literal(Literal::Integer(text)) => {
                let output = text
                    .parse::<usize>()
                    .ok()
                    .and_then(|position| position.checked_sub(1))
                    .and_then(|index| outputs.get(index))
                    .ok_or_else(|| {
                        self.error(format!("unknown column `{text}` in `GROUP BY`"), item.span)
                    })?;
                Ok((output.expr.clone(), SortReference::Position))
            }
            ast::ExprKind::Column(name)
                if name.parts.len() == 1 && scope.resolve(name) == Err(ResolveError::Unknown) =>
            {
                match self.alias(&name.parts[0], name.span)? {
                    Some(output) => Ok((output.expr.clone(), SortReference::Alias)),
                    None => Ok((self.value(item, scope)?, SortReference::Expression)),
                }
            }
            _ => Ok((self.value(item, scope)?, SortReference::Expression)),
        }
    }

    /// Predicate of `HAVING`, which reads no window function.
    pub(super) fn having(&mut self, having: &ast::Expr, scope: &Scope) -> Result<Expr, BuildError> {
        let predicate = self.condition(having, scope)?;
        if reads_internal(&predicate, &format!("{INTERNAL_PREFIX}window")) {
            return Err(self.error("window function in `HAVING`", having.span));
        }
        Ok(predicate)
    }

    /// Aggregation of a grouped block over `input`, its keys `keys`: the
    /// parts of the block above it read its columns.
    pub(super) fn aggregate_node(
        &mut self,
        input: Plan,
        keys: Keys,
        rollup: bool,
        above: Above<'_>,
    ) -> Plan {
        let block = self.block_mut();
        let views: Vec<Key> = keys
            .iter()
            .enumerate()
            .map(|(index, (expr, _))| Key::new(index, expr, &block.source, rollup))
            .collect();
        for item in above.items {
            if let ProjectItem::Expr(named) = item {
                block.regroup(&mut named.expr, &views);
            }
        }
        if let Some(having) = above.having {
            block.regroup(having, &views);
        }
        for key in above.sort {
            block.regroup(&mut key.expr, &views);
        }
        block.regroup_windows(&views);
        let (keys, references) = keys
            .into_iter()
            .enumerate()
            .map(|(index, (expr, reference))| {
                let key = NamedExpr {
                    expr,
                    name: key_name(index),
                    alias: AliasForm::As,
                };
                (key, reference)
            })
            .unzip();
        Plan::Aggregate(Aggregate {
            input: Box::new(input),
            keys,
            references,
            rollup,
            aggregates: std::mem::take(&mut block.aggregates),
            span: None,
        })
    }

    /// Window node of the block over `input`, when it calls window functions
    /// or names windows.
    pub(super) fn window_node(&mut self, input: Plan) -> Plan {
        let block = self.block_mut();
        if block.windows.is_empty() && block.definitions.is_empty() {
            return input;
        }
        Plan::Window(Window {
            input: Box::new(input),
            functions: std::mem::take(&mut block.windows),
            definitions: std::mem::take(&mut block.definitions),
            span: None,
        })
    }
}

/// Tells whether a `SELECT` block aggregates its rows: `GROUP BY`, or an
/// aggregate function in its select list, its `HAVING` or the `ORDER BY` of
/// its query.
pub(super) fn is_grouped(select: &Select, order_by: &[OrderItem]) -> bool {
    !select.group_by.is_empty()
        || select.items.iter().any(|item| match item {
            SelectItem::Expr { expr, .. } => has_aggregate(expr),
            SelectItem::Wildcard { .. } => false,
        })
        || select.having.as_ref().is_some_and(has_aggregate)
        || order_by.iter().any(|item| has_aggregate(&item.expr))
}

/// Tells whether `name` has the form of a column a block computes for
/// itself: `#key1`, `#aggregate1`, `#window1`.
pub(crate) fn is_internal_name(name: &str) -> bool {
    ["key", "aggregate", "window"].iter().any(|kind| {
        name.strip_prefix(INTERNAL_PREFIX)
            .and_then(|rest| rest.strip_prefix(kind))
            .is_some_and(|digits| {
                !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())
            })
    })
}

/// Tells whether `expr` reads a column the block computes for itself, whose
/// name starts with `prefix`.
fn reads_internal(expr: &Expr, prefix: &str) -> bool {
    let mut copy = expr.clone();
    reads_internal_mut(&mut copy, prefix)
}

/// Same as `reads_internal`, on the operands given by `operands_mut`.
fn reads_internal_mut(expr: &mut Expr, prefix: &str) -> bool {
    if let ExprKind::Column(column) = &expr.kind
        && column.qualifier.is_none()
        && column.name.value.starts_with(prefix)
    {
        return true;
    }
    expr.operands_mut()
        .into_iter()
        .any(|operand| reads_internal_mut(operand, prefix))
}

/// Same expression at `span`, when known.
fn with_span(expr: Expr, span: Option<prisme_plan::Span>) -> Expr {
    match span {
        Some(span) => expr.with_span(span),
        None => expr,
    }
}

/// Tells whether `expr` calls an aggregate function of its own block, out
/// of the window functions and of the subqueries.
pub(super) fn has_aggregate(expr: &ast::Expr) -> bool {
    if let ast::ExprKind::Function(call) = &expr.kind
        && call.over.is_none()
        && call.is_aggregate()
    {
        return true;
    }
    expr.children().into_iter().any(has_aggregate)
}

/// Same expression without positions, its columns named as `source` names
/// them: two expressions of the same value compare equal.
pub(super) fn canonical(expr: &Expr, source: &Scope) -> Expr {
    let mut copy = expr.clone().without_spans();
    canonical_columns(&mut copy, source);
    copy
}

/// Names the columns of `expr` as `source` names them, an item of the
/// select list read through its alias by its expression.
fn canonical_columns(expr: &mut Expr, source: &Scope) {
    if let ExprKind::Alias(alias) = &mut expr.kind {
        let value = std::mem::replace(&mut alias.expr, Expr::literal(Value::Null, DataType::Null));
        *expr = value;
    }
    if let ExprKind::Column(column) = &mut expr.kind
        && let Some(found) = source.canonical(column)
    {
        *column = found;
    }
    for operand in expr.operands_mut() {
        canonical_columns(operand, source);
    }
}

/// Aggregate call without positions, its columns named as `source` names
/// them.
fn canonical_call(call: &AggregateCall, source: &Scope) -> AggregateCall {
    let mut copy = call.clone();
    copy.span = None;
    for arg in &mut copy.args {
        *arg = canonical(arg, source);
    }
    for key in &mut copy.order_by {
        key.expr = canonical(&key.expr, source);
    }
    copy
}
