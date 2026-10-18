//! Query between parentheses followed by an outer `ORDER BY` or `LIMIT`. With
//! its own `ORDER BY` or `LIMIT` (rule `limit.nested`), measured on each
//! image of MySQL 8.0.11 to 9.3.0 and MariaDB 10.5.29 to 11.8.9
//! (`crates/probe/cases/parenthesized-limits.sql`,
//! `docs/sources/mysql/parenthesized-limits.md`).
//!
//! The plan reads it by the standard, as MariaDB does:
//! `(SELECT ... LIMIT 2) LIMIT 5` gives 2 rows at most. MySQL reads an outer
//! `LIMIT` without outer `ORDER BY` over an inner `LIMIT` otherwise:
//!
//! - up to 8.0.30, the outer `LIMIT` overrides the inner one, its offset
//!   included (5 rows; construct `parenthesized_limit_kept`), and up to
//!   8.0.18 it drops the inner `ORDER BY` as well (construct
//!   `parenthesized_order_kept`, Bug #30521098);
//! - from 8.0.31, an outer `OFFSET` counts from the start of the inner rows
//!   without bounding them by the inner `LIMIT`:
//!   `(SELECT a ... ORDER BY a LIMIT 3) LIMIT 5 OFFSET 1` gives 2, 3, 4, and
//!   2, 3 in MariaDB, in every version measured (construct
//!   `parenthesized_offset_kept`, absent from MySQL);
//! - from 8.0.19 to 8.0.30, a set operation between parentheses with its own
//!   `ORDER BY` or `LIMIT`, followed by another, is refused, error 1235
//!   (construct `parenthesized_set_limit`);
//! - up to 8.0.18, an outer `ORDER BY` sorts the rows of a set operation
//!   between parentheses before its own `LIMIT`:
//!   `(SELECT a ... UNION SELECT 6 ORDER BY 1 LIMIT 2) ORDER BY 1 DESC` gives
//!   6, 5, and 2, 1 by the standard (construct `parenthesized_set_order`).
//!
//! An outer `ORDER BY` after a query with its own `ORDER BY` or `LIMIT`
//! reads the columns of its result, as the plan reads them, except
//! (`crates/probe/cases/parenthesized-order.sql`, rule `order.parenthesized`):
//!
//! - MariaDB before the fix of MDEV-29681 (construct
//!   `parenthesized_order_inner`), which reads the names of a key other than
//!   a name alone or a position in the context of the query between
//!   parentheses: `(SELECT id, a AS b FROM t LIMIT 3) ORDER BY -b` sorts the
//!   rows in no defined order, `-b` reads the column `b` of the table
//!   without `LIMIT`, error 1054 or 1052 over a set operation or a join, and
//!   an expression of the alias of a window function stops the server;
//! - a subquery of that `ORDER BY`, which every MariaDB measured sorts in no
//!   defined order (`ORDER BY (SELECT -b)`).
//!
//! The derived table stands for the parentheses towards these servers, the
//! query is an error from them. Without `ORDER BY` or `LIMIT` of its own,
//! MariaDB and MySQL read the names of the outer `ORDER BY` as those of an
//! `ORDER BY` of the `SELECT`, which the plan folds into its block, except
//! MySQL 8.0.31 to 8.0.41, 8.1.0 to 8.4.4 and 9.0.0 to 9.2.0, which read the
//! columns of the result, as the plan reads them from these versions
//! (construct `parenthesized_order_result`, rule `order.parenthesized`,
//! `crates/probe/cases/alias-references.sql`):
//! `(SELECT id, a AS b FROM t) ORDER BY b + 0` sorts by the column `b` of
//! the source in the others. These versions also sort the rows otherwise by
//! an expression of the alias of a grouping key that is an expression, in a
//! block with aggregate functions
//! (`(SELECT a % 2 AS x, COUNT(*) AS n FROM t GROUP BY x) ORDER BY -x`): an
//! error towards the others.
//!
//! A derived table reads by the standard in every version
//! (`SELECT * FROM (SELECT ... LIMIT 2) AS d LIMIT 5`): it stands for the
//! parentheses towards a server that reads them otherwise than the input,
//! when the input reads them by the standard; when the input reads them
//! otherwise, and the output otherwise than the input, the query is an error
//! (**[decided autonomously]**).

use prisme_plan::{Expr, ExprKind, Ident, Limit, Lock, Nulls, Plan, ProjectItem, Sort, SortKey};
use prisme_types::Value;

use super::lock::LockEnd;
use super::{Block, Emitter, call};
use crate::Dialect;
use crate::locking::Shape;
use crate::report::StepKind;
use crate::versions::Construct;

/// How a server reads a query between parentheses followed by an outer
/// `ORDER BY` or `LIMIT`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reading {
    /// By the standard: each `LIMIT` bounds the rows of its own level.
    Standard,
    /// The outer `LIMIT` overrides the inner one, offset included.
    Override,
    /// The outer `LIMIT` overrides the inner one and drops the inner
    /// `ORDER BY`.
    OverrideUnordered,
    /// The outer `OFFSET` counts from the start of the inner rows, without
    /// bounding them by the inner `LIMIT`.
    OffsetUnbounded,
    /// The outer `ORDER BY` sorts the rows of the set operation before its
    /// own `LIMIT`.
    SortedFirst,
    /// Refused (error 1235).
    Refused,
}

/// How a server reads a key of an `ORDER BY` after a query between
/// parentheses with its own `ORDER BY` or `LIMIT`, otherwise than by the
/// columns of the result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InnerKey {
    /// Its names in the context of that query, a key other than a name
    /// alone or a position (MariaDB before the fix of MDEV-29681, construct
    /// `parenthesized_order_inner`).
    Context,
    /// A subquery, which sorts the rows in no defined order (MariaDB).
    Subquery,
}

impl InnerKey {
    /// What the server does, after its name.
    const fn describe(self) -> &'static str {
        match self {
            Self::Context => {
                "reads the names of an `ORDER BY` after a query between parentheses with its \
                 own `ORDER BY` or `LIMIT` in the context of that query (MDEV-29681): a key \
                 other than a name alone or a position sorts the rows otherwise than by the \
                 columns of the result"
            }
            Self::Subquery => {
                "sorts the rows in no defined order by a subquery of an `ORDER BY` after a \
                 query between parentheses with its own `ORDER BY` or `LIMIT`"
            }
        }
    }
}

/// Shape of a query between parentheses and of the clauses after it.
#[derive(Debug, Clone, Copy)]
struct Nesting {
    /// The outer query has an `ORDER BY`.
    outer_sort: bool,
    /// The outer query has a `LIMIT`.
    outer_limit: bool,
    /// The outer `LIMIT` has an offset other than 0.
    outer_offset: bool,
    /// The query between parentheses has an `ORDER BY` at its top.
    inner_sort: bool,
    /// The query between parentheses has a `LIMIT` at its top.
    inner_limit: bool,
    /// The query between parentheses is a set operation.
    inner_set: bool,
}

impl Nesting {
    /// Shape of `inner`, between parentheses, followed by `sort` and
    /// `limit`.
    fn of(inner: &Plan, sort: Option<&Sort>, limit: Option<&Limit>) -> Self {
        let mut node = unlocked(inner);
        let mut inner_limit = false;
        let mut inner_sort = false;
        if let Some(block) = Block::fold(node) {
            inner_limit = block.limit.is_some();
            inner_sort = block.sort.is_some();
        } else {
            if let Plan::Limit(limit) = node {
                inner_limit = true;
                node = &limit.input;
            }
            if let Plan::Sort(sort) = node {
                inner_sort = true;
                node = &sort.input;
            }
        }
        Self {
            outer_sort: sort.is_some(),
            outer_limit: limit.is_some(),
            outer_offset: limit.is_some_and(|limit| {
                limit.offset.as_ref().is_some_and(|offset| {
                    !matches!(
                        &offset.kind,
                        prisme_plan::ExprKind::Literal(Value::Integer(0))
                    )
                })
            }),
            inner_sort,
            inner_limit,
            inner_set: matches!(node, Plan::SetOperation(_)),
        }
    }

    /// Shape of the clauses, for the locking clauses (`locking` module).
    const fn lock_shape(self) -> Shape {
        Shape {
            outer_sort: self.outer_sort,
            outer_limit: self.outer_limit,
            inner_sort: self.inner_sort,
            inner_limit: self.inner_limit,
            inner_set: self.inner_set,
        }
    }
}

/// `plan` without its `WITH`, nor the locking clauses at its top: the query
/// whose clauses meet those after the parentheses around it.
fn unlocked(plan: &Plan) -> &Plan {
    let plan = match plan {
        Plan::With(with) => with.body.as_ref(),
        _ => plan,
    };
    match plan {
        Plan::Lock(lock) => &lock.input,
        _ => plan,
    }
}

impl Emitter<'_> {
    /// `inner` between parentheses, before the outer `sort` and `limit`
    /// that the caller writes: as written when the output reads it as the
    /// input does, from a derived table when the input reads it by the
    /// standard and the output otherwise, an error otherwise.
    pub(super) fn parenthesized(
        &mut self,
        inner: &Plan,
        sort: Option<&Sort>,
        limit: Option<&Limit>,
        names_matter: bool,
    ) {
        let nesting = Nesting::of(inner, sort, limit);
        if let Some(sort) = sort
            && let Some(order) = self.outer_order(inner, sort, nesting)
        {
            self.sorted_otherwise(inner, sort, order, names_matter);
            return;
        }
        let input = self.reading(nesting, true);
        let output = self.reading(nesting, false);
        if input == output {
            if let Some(sort) = sort
                && let Some(order) = self.inner_order(sort, nesting)
            {
                self.sorted_otherwise(inner, sort, order, names_matter);
                return;
            }
            if !self.place_trailing_lock(inner, nesting, limit) {
                return;
            }
            // MariaDB refuses the options of the first `SELECT` there
            // (`options` module).
            if self.output.dialect == Dialect::MariaDb && !nesting.inner_set {
                self.head = false;
            }
            self.write("(");
            self.query(inner, names_matter);
            self.write(")");
            return;
        }
        let span = limit.and_then(|limit| limit.span).or_else(|| inner.span());
        if input != Reading::Standard {
            let source = format!("{} {}", self.source.dialect.label(), self.source.version);
            let message = format!(
                "{source} {}, {} {}: not supported yet by Prisme",
                describe(input),
                self.target(),
                describe(output)
            );
            self.error("limit.nested", &message, span);
            self.write("(");
            self.query(inner, names_matter);
            self.write(")");
            return;
        }
        let message = format!("{} {}", self.target(), describe(output));
        self.as_derived(inner, "limit.nested", &message, span);
    }

    /// Places the locking clauses at the end of `inner`, a query between
    /// parentheses of `nesting` that the output reads as the input (rule
    /// `lock.parenthesized`): after the outer `ORDER BY` and `LIMIT` when
    /// written there and the output locks the rows of `inner` there, or when
    /// it would drop them at the end of `inner` (MySQL before 8.0.19);
    /// at the end of `inner` otherwise, or from a derived table when the
    /// output locks its rows in neither place. Returns false when the
    /// derived table is written.
    fn place_trailing_lock(
        &mut self,
        inner: &Plan,
        nesting: Nesting,
        limit: Option<&Limit>,
    ) -> bool {
        let Some(lock) = self.trailing_lock_of(inner) else {
            return true;
        };
        let shape = nesting.lock_shape();
        let inside = shape.locked_inside(|construct| self.has(construct));
        let after = shape.locked_after(|construct| self.has(construct));
        let span = lock.clauses.first().and_then(|clause| clause.span);
        if after && (lock.outside || !inside) {
            self.lock_end = LockEnd::Lift;
            if !lock.outside {
                let message = format!(
                    "{} drops a locking clause at the end of a query between parentheses \
                     followed by a `LIMIT` without `ORDER BY` (Bug #30521098): written after \
                     the `LIMIT`, which locks the same rows",
                    self.target()
                );
                self.step("lock.parenthesized", StepKind::Rewrite, message, span);
            }
            return true;
        }
        if inside {
            if lock.outside {
                let message = format!(
                    "{} locks no row by a locking clause after a query between parentheses \
                     with these clauses: written at the end of that query, which locks the \
                     same rows",
                    self.target()
                );
                self.step("lock.parenthesized", StepKind::Rewrite, message, span);
            }
            return true;
        }
        let message = format!(
            "{} locks no row by a locking clause after a query between parentheses with these \
             clauses, and drops it at the end of that query",
            self.target()
        );
        let span = limit.and_then(|limit| limit.span).or(span);
        self.as_derived(inner, "lock.parenthesized", &message, span);
        false
    }

    /// Lock whose clauses the text of `plan` ends with: a locked block, or the
    /// last operand of a set operation, written without parentheses.
    fn trailing_lock_of<'p>(&self, plan: &'p Plan) -> Option<&'p Lock> {
        match plan {
            Plan::With(with) => self.trailing_lock_of(&with.body),
            Plan::Lock(lock) => Some(lock),
            Plan::Limit(limit) if Block::fold(plan).is_none() => {
                self.trailing_lock_of(&limit.input)
            }
            Plan::Sort(sort) => self.trailing_lock_of(&sort.input),
            Plan::SetOperation(operation) => {
                let right = operation.right.as_ref();
                let written = match right {
                    Plan::Lock(lock) => &lock.input,
                    _ => right,
                };
                if self.operand_parenthesized(written, operation.operator, true, false) {
                    return None;
                }
                self.trailing_lock_of(right)
            }
            _ => None,
        }
    }

    /// `inner` between parentheses, before the `ORDER BY` `sort` that the
    /// output reads otherwise than the input (rule `order.parenthesized`):
    /// from a derived table that stands for the parentheses (`Ok`), or as
    /// written with an error (`Err`).
    fn sorted_otherwise(
        &mut self,
        inner: &Plan,
        sort: &Sort,
        order: Result<String, String>,
        names_matter: bool,
    ) {
        let span = (sort.keys.first())
            .and_then(|key| key.expr.span)
            .or_else(|| inner.span());
        match order {
            Ok(message) => self.as_derived(inner, "order.parenthesized", &message, span),
            Err(message) => {
                self.error("order.parenthesized", &message, span);
                self.write("(");
                self.query(inner, names_matter);
                self.write(")");
            }
        }
    }

    /// `inner` as a derived table that stands for its parentheses, which the
    /// output reads otherwise (`message`), under `rule`.
    fn as_derived(
        &mut self,
        inner: &Plan,
        rule: &'static str,
        message: &str,
        span: Option<prisme_plan::Span>,
    ) {
        if self.has_duplicate_names(inner) {
            let message =
                format!("{message}, and its columns have the same names: not supported yet");
            self.error(rule, &message, span);
        }
        self.write("SELECT * FROM (");
        self.head = false;
        self.query(inner, true);
        self.write(") AS `d`");
        self.step(
            rule,
            StepKind::Emulation,
            format!("{message}: emulated by a derived table"),
            span,
        );
    }

    /// How the output reads the `ORDER BY` `sort` after `inner`, a `SELECT`
    /// between parentheses without `ORDER BY` or `LIMIT` of its own, when
    /// the input reads its names as the columns of the result and the output
    /// as those of an `ORDER BY` of the `SELECT`: `None` when each key reads
    /// the same values in both; the message of the derived table that stands
    /// for the parentheses otherwise; an error message when the input sorts
    /// the rows otherwise than the plan.
    fn outer_order(
        &mut self,
        inner: &Plan,
        sort: &Sort,
        nesting: Nesting,
    ) -> Option<Result<String, String>> {
        if nesting.inner_sort
            || nesting.inner_limit
            || nesting.inner_set
            || !self.input_has(Construct::ParenthesizedOrderResult)
            || self.has(Construct::ParenthesizedOrderResult)
        {
            return None;
        }
        let block = Block::fold(unlocked(inner))?;
        self.blocks.push(call::Internal::new(&block));
        let misread = (block.aggregate).is_some_and(|aggregate| !aggregate.aggregates.is_empty())
            && (sort.keys.iter()).any(|key| self.reads_expression_key(&block, &key.expr, true));
        let alike = (sort.keys.iter()).all(|key| self.sorted_alike(&block, &key.expr, true));
        self.blocks.pop();
        let source = format!("{} {}", self.source.dialect.label(), self.source.version);
        let reads = "reads the names of an `ORDER BY` after a `SELECT` between parentheses as \
                     those of its own `ORDER BY`, a column of the source first inside an \
                     expression";
        if misread {
            return Some(Err(format!(
                "{source} sorts the rows otherwise by an expression of the alias of a grouping \
                 key that is an expression, after a `SELECT` between parentheses with aggregate \
                 functions, and {} {reads}: not supported by Prisme",
                self.target()
            )));
        }
        (!alike).then(|| Ok(format!("{} {reads}", self.target())))
    }

    /// How the output reads the `ORDER BY` `sort` after a query between
    /// parentheses with its own `ORDER BY` or `LIMIT`, which the plan sorts
    /// by the columns of the result: `None` when the input and the output
    /// read each key alike; the message of the derived table that stands for
    /// the parentheses when the output reads a key otherwise; an error
    /// message when the input does, whose order the plan does not follow.
    fn inner_order(&self, sort: &Sort, nesting: Nesting) -> Option<Result<String, String>> {
        if !nesting.inner_sort && !nesting.inner_limit {
            return None;
        }
        let differs = |key: &&SortKey| self.inner_key(key, true) != self.inner_key(key, false);
        let key = sort.keys.iter().find(differs)?;
        if let Some(reading) = self.inner_key(key, true) {
            let source = format!("{} {}", self.source.dialect.label(), self.source.version);
            return Some(Err(format!(
                "{source} {}: not supported by Prisme",
                reading.describe()
            )));
        }
        let reading = self.inner_key(key, false)?;
        Some(Ok(format!("{} {}", self.target(), reading.describe())))
    }

    /// How the input (`input`) or the output reads `key`, of an `ORDER BY`
    /// after a query between parentheses with its own `ORDER BY` or `LIMIT`:
    /// `None` by the columns of the result.
    fn inner_key(&self, key: &SortKey, input: bool) -> Option<InnerKey> {
        let (dialect, inner) = if input {
            (
                self.source.dialect,
                self.input_has(Construct::ParenthesizedOrderInner),
            )
        } else {
            (
                self.output.dialect,
                self.has(Construct::ParenthesizedOrderInner),
            )
        };
        if inner && !self.written_as_column(key) {
            Some(InnerKey::Context)
        } else if dialect == Dialect::MariaDb && has_subquery(&key.expr) {
            Some(InnerKey::Subquery)
        } else {
            None
        }
    }

    /// Tells whether the output writes `key` as a name alone or a position:
    /// a column of the result, its NULLs where the MySQL family puts them,
    /// sorted by its value (`json` module).
    fn written_as_column(&self, key: &SortKey) -> bool {
        matches!(&key.expr.kind, ExprKind::Column(column) if column.qualifier.is_none())
            && key.nulls == Nulls::as_smallest(key.direction)
            && !self.json_differs(&key.expr)
    }

    /// Tells whether `expr`, a key of an `ORDER BY` after `block`, reads
    /// inside an expression a column of the result whose item is a grouping
    /// key that is an expression.
    fn reads_expression_key(&self, block: &Block<'_>, expr: &Expr, top: bool) -> bool {
        if let ExprKind::Column(column) = &expr.kind {
            return !top
                && item(block, &column.name).is_some_and(|value| self.expression_key(value));
        }
        let mut copy = expr.clone();
        (copy.operands_mut().into_iter())
            .any(|operand| self.reads_expression_key(block, operand, false))
    }

    /// Tells whether `expr`, a key of an `ORDER BY` after `block` that names
    /// its columns of the result, reads the same values when the output
    /// reads its names as those of an `ORDER BY` of `block`: a name alone
    /// designates the item of the select list in both; inside an
    /// expression, a name that no column of the source has, or the column of
    /// the source that its item is. A subquery or a window function reads
    /// otherwise.
    fn sorted_alike(&self, block: &Block<'_>, expr: &Expr, top: bool) -> bool {
        match &expr.kind {
            ExprKind::Column(column) if column.qualifier.is_none() => {
                top || self.item_is_column(block, &column.name)
            }
            ExprKind::Column(_)
            | ExprKind::Exists { .. }
            | ExprKind::ScalarSubquery(_)
            | ExprKind::InSubquery { .. }
            | ExprKind::Quantified { .. }
            | ExprKind::Window(_) => false,
            _ => {
                let mut copy = expr.clone();
                (copy.operands_mut().into_iter())
                    .all(|operand| self.sorted_alike(block, operand, false))
            }
        }
    }

    /// Tells whether a name inside an expression of an `ORDER BY` of
    /// `block` reads the item `name` of its select list: no column of the
    /// source has the name, or a single one, which the item is.
    fn item_is_column(&self, block: &Block<'_>, name: &Ident) -> bool {
        let schema = block.source.schema();
        let same = |other: &Ident| other.value.eq_ignore_ascii_case(&name.value);
        let columns: Vec<_> = (schema.columns.iter())
            .filter(|column| same(&column.name))
            .collect();
        match columns.as_slice() {
            [] => true,
            [_] => item(block, name).is_none_or(|value| {
                let column = match &value.kind {
                    ExprKind::Column(column) => Some(column.clone()),
                    _ => None,
                };
                (self.key_column(value).or(column)).is_some_and(|column| same(&column.name))
            }),
            _ => false,
        }
    }

    /// Reading of `nesting` by the input when `input`, by the output
    /// otherwise.
    fn reading(&self, nesting: Nesting, input: bool) -> Reading {
        let has = |construct| {
            if input {
                self.input_has(construct)
            } else {
                self.has(construct)
            }
        };
        let limited = nesting.inner_sort || nesting.inner_limit;
        let outer = nesting.outer_sort || nesting.outer_limit;
        if nesting.inner_set && limited && outer && !has(Construct::ParenthesizedSetLimit) {
            return Reading::Refused;
        }
        if nesting.inner_set
            && nesting.inner_limit
            && nesting.outer_sort
            && !has(Construct::ParenthesizedSetOrder)
        {
            return Reading::SortedFirst;
        }
        if !nesting.outer_limit || nesting.outer_sort || !nesting.inner_limit {
            return Reading::Standard;
        }
        if !has(Construct::ParenthesizedLimitKept) {
            if nesting.inner_sort && !has(Construct::ParenthesizedOrderKept) {
                return Reading::OverrideUnordered;
            }
            return Reading::Override;
        }
        if nesting.outer_offset && !has(Construct::ParenthesizedOffsetKept) {
            return Reading::OffsetUnbounded;
        }
        Reading::Standard
    }

    /// Tells whether two columns of `plan` have the same name, which a
    /// derived table refuses.
    pub(super) fn has_duplicate_names(&self, plan: &Plan) -> bool {
        let schema = plan.schema();
        let names = &schema.columns;
        names.iter().enumerate().any(|(index, column)| {
            names[..index]
                .iter()
                .any(|other| other.name.value.eq_ignore_ascii_case(&column.name.value))
        })
    }
}

/// Expression of the item of the select list of `block` named `name`;
/// `None` for a column of a wildcard.
fn item<'b>(block: &Block<'b>, name: &Ident) -> Option<&'b Expr> {
    (block.project.items.iter()).find_map(|item| match item {
        ProjectItem::Expr(named) if named.name.value.eq_ignore_ascii_case(&name.value) => {
            Some(&named.expr)
        }
        _ => None,
    })
}

/// What a server does with a query between parentheses read as `reading`,
/// for the messages.
fn describe(reading: Reading) -> &'static str {
    match reading {
        Reading::Standard => "reads a `LIMIT` after a query between parentheses by the standard",
        Reading::Override => {
            "lets a `LIMIT` after a query between parentheses override the `LIMIT` of that query"
        }
        Reading::OverrideUnordered => {
            "lets a `LIMIT` after a query between parentheses override the `LIMIT` and the `ORDER BY` of that query"
        }
        Reading::OffsetUnbounded => {
            "counts an `OFFSET` after a query between parentheses without the `LIMIT` of that query"
        }
        Reading::SortedFirst => {
            "sorts the rows of a set operation between parentheses by the outer `ORDER BY` before its own `LIMIT`"
        }
        Reading::Refused => {
            "refuses an `ORDER BY` or a `LIMIT` after a set operation between parentheses with its own"
        }
    }
}

/// Tells whether `expr` holds a subquery.
fn has_subquery(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Exists { .. }
        | ExprKind::ScalarSubquery(_)
        | ExprKind::InSubquery { .. }
        | ExprKind::Quantified { .. } => true,
        _ => {
            let mut copy = expr.clone();
            (copy.operands_mut().into_iter()).any(|operand| has_subquery(operand))
        }
    }
}
