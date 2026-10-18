//! Locking clauses, each one on the query block it locks
//! (`prisme_plan::Lock`), as the servers read them
//! (`crates/probe/cases/locking-scope.sql`): the block it follows, the last
//! operand of a set operation, the query between parentheses it follows.
//! The tables it locks are those of the `FROM` of that block, by their alias
//! or, without alias, by their name (`OF`, MySQL): neither its subqueries
//! nor its derived tables nor its common table expressions.

use prisme_plan::{BlockTable, BlockTableKind, Lock, LockClause, Plan, Span, block_tables};

use super::{BuildError, Builder, table_name};
use crate::locking::Shape;
use crate::sql::ast::{Locking, ObjectName, Query, QueryBody};

impl Builder<'_> {
    /// `plan` with the locking clauses `locks`, written in that order, on the
    /// block they lock, after the parentheses around it when `outside`.
    ///
    /// # Errors
    ///
    /// `BuildError` on a name of `OF` that no table of the block has, on a
    /// derived table in `OF` (MySQL, error 3569), on a table in several
    /// clauses (error 3569), on a block locked between parentheses and after
    /// them.
    pub(super) fn locked(
        &self,
        plan: Plan,
        locks: &[&Locking],
        outside: bool,
    ) -> Result<Plan, BuildError> {
        let Some(first) = locks.first() else {
            return Ok(plan);
        };
        let target = lock_target(&plan);
        if matches!(target, Plan::Lock(_)) {
            return Err(self.several_locks(first.span));
        }
        let tables = block_tables(target);
        let mut clauses = Vec::with_capacity(locks.len());
        let mut locked: Vec<usize> = Vec::new();
        for lock in locks {
            let covered: Vec<usize> = if lock.tables.is_empty() {
                (0..tables.len()).collect()
            } else {
                (lock.tables.iter())
                    .map(|name| self.locked_table(name, &tables))
                    .collect::<Result<_, _>>()?
            };
            if let Some(&twice) = covered.iter().find(|index| locked.contains(index)) {
                let message = format!(
                    "table `{}` in several locking clauses (MySQL, error 3569)",
                    tables[twice].name.value
                );
                return Err(self.error(message, lock.span));
            }
            locked.extend(covered);
            clauses.push(LockClause {
                strength: lock.strength,
                tables: lock.tables.iter().map(table_name).collect(),
                wait: lock.wait.clone(),
                form: lock.form,
                span: Some(lock.span),
            });
        }
        Ok(wrap(plan, clauses, outside))
    }

    /// Error of a query block locked between parentheses and after them, at
    /// `span`.
    pub(super) fn several_locks(&self, span: Span) -> BuildError {
        self.error(
            "several locking clauses on one query block, between parentheses and after them: \
             not supported by Prisme",
            span,
        )
    }

    /// Checks that the locking clauses after `query`, a query between
    /// parentheses followed by its `ORDER BY` and its `LIMIT`, and the one at
    /// the end of the query between them, lock rows in the input (`locking`
    /// module).
    ///
    /// # Errors
    ///
    /// `BuildError` on a clause that locks no row in the input.
    pub(super) fn lock_placement(&self, query: &Query) -> Result<(), BuildError> {
        let QueryBody::Nested(inner) = &query.body else {
            return Ok(());
        };
        let inner = bare(inner);
        let shape = Shape {
            outer_sort: !query.order_by.is_empty(),
            outer_limit: query.limit.is_some(),
            inner_sort: !inner.order_by.is_empty(),
            inner_limit: inner.limit.is_some(),
            inner_set: is_set_operation(&inner.body),
        };
        let has = |construct| self.has(construct);
        let source = format!("{} {}", self.dialect.label(), self.version);
        if let Some(lock) = query.locks.first()
            && !shape.locked_after(has)
        {
            let hint = if shape.locked_inside(has) {
                " (at the end of the query between parentheses, it locks its rows)"
            } else {
                ""
            };
            let message = format!(
                "{source} locks no row by a locking clause there, the clauses after the \
                 parentheses making a block of their own, without table: not supported by \
                 Prisme{hint}"
            );
            return Err(self.error(message, lock.span));
        }
        let lock = (query.locks.first()).or_else(|| inner.locks.first());
        if let Some(lock) = lock
            && !shape.limited_when_locked(has)
        {
            let message = format!(
                "{source} gives every row of a `SELECT` between parentheses with its own \
                 `ORDER BY` and a locking clause, the `LIMIT` after the parentheses left aside: \
                 not supported by Prisme"
            );
            return Err(self.error(message, lock.span));
        }
        if let Some(lock) = inner.locks.first()
            && !shape.locked_inside(has)
        {
            let hint = if shape.locked_after(has) {
                " (after the `LIMIT`, it locks the rows of the query between parentheses)"
            } else {
                ""
            };
            let message = format!(
                "{source} drops a locking clause at the end of a query between parentheses \
                 followed by a `LIMIT` without `ORDER BY` (Bug #30521098), and locks no row: \
                 not supported by Prisme{hint}"
            );
            return Err(self.error(message, lock.span));
        }
        Ok(())
    }

    /// Position in `tables` of the table that `name`, written after `OF`,
    /// designates (`BlockTable::is_named`).
    fn locked_table(
        &self,
        name: &ObjectName,
        tables: &[BlockTable<'_>],
    ) -> Result<usize, BuildError> {
        let written = table_name(name);
        match tables.iter().position(|table| table.is_named(&written)) {
            Some(index) if tables[index].kind == BlockTableKind::Derived => {
                let message = format!(
                    "`{}`, a derived table, in a locking clause: MySQL refuses it (error 3569)",
                    written.name.value
                );
                Err(self.error(message, name.span))
            }
            Some(index) => Ok(index),
            None => {
                let message = format!(
                    "`{}` in a locking clause: no table of the block has this alias or name",
                    self.written(name.span)
                );
                Err(self.error(message, name.span))
            }
        }
    }
}

/// `plan` with its `LIMIT` joined to the locked block it bounds, a block
/// without `LIMIT` of its own: `(SELECT a FROM t ORDER BY a FOR UPDATE)
/// LIMIT 1` reads as `SELECT a FROM t ORDER BY a LIMIT 1 FOR UPDATE`, which
/// every server reads alike (`locking` module).
pub(super) fn joined_limit(plan: Plan) -> Plan {
    let Plan::Limit(mut limit) = plan else {
        return plan;
    };
    match *limit.input {
        Plan::Lock(mut lock) if !matches!(*lock.input, Plan::Limit(_)) && is_block(&lock.input) => {
            limit.input = lock.input;
            lock.input = Box::new(Plan::Limit(limit));
            Plan::Lock(lock)
        }
        input => {
            limit.input = Box::new(input);
            Plan::Limit(limit)
        }
    }
}

/// `query`, or the query between the parentheses it is made of, without
/// clauses of its own: `((SELECT a FROM t LIMIT 2))` reads as `(SELECT a
/// FROM t LIMIT 2)`.
fn bare(query: &Query) -> &Query {
    match &query.body {
        QueryBody::Nested(inner)
            if query.order_by.is_empty() && query.limit.is_none() && query.locks.is_empty() =>
        {
            bare(inner)
        }
        _ => query,
    }
}

/// Tells whether `body` is a set operation, between parentheses without
/// clauses of their own or not.
fn is_set_operation(body: &QueryBody) -> bool {
    match body {
        QueryBody::SetOperation(_) => true,
        QueryBody::Nested(inner) => {
            let inner = bare(inner);
            inner.order_by.is_empty()
                && inner.limit.is_none()
                && inner.locks.is_empty()
                && is_set_operation(&inner.body)
        }
        _ => false,
    }
}

/// Position of a locking clause written at the top of `body`, a part of a
/// recursive common table expression: between its parentheses.
pub(super) fn written_lock(body: &QueryBody) -> Option<Span> {
    let QueryBody::Nested(query) = body else {
        return None;
    };
    (query.locks.first().map(|lock| lock.span)).or_else(|| written_lock(&query.body))
}

/// Node of `plan` that a locking clause written after it locks: a block
/// (`Limit(Distinct(Project(...)))`, a `TABLE` or a `VALUES` with its sort
/// and its limit), the last operand of a set operation, the body of a
/// `WITH`; a block already locked.
fn lock_target(plan: &Plan) -> &Plan {
    if is_block(plan) {
        return plan;
    }
    match plan {
        Plan::Limit(limit) => lock_target(&limit.input),
        Plan::Sort(sort) => lock_target(&sort.input),
        Plan::SetOperation(operation) => lock_target(&operation.right),
        Plan::With(with) => lock_target(&with.body),
        other => other,
    }
}

/// `plan` with `clauses` above the node that `lock_target` finds, written
/// after the parentheses around it when `outside`.
fn wrap(plan: Plan, clauses: Vec<LockClause>, outside: bool) -> Plan {
    let lock = |input: Plan, clauses| {
        Plan::Lock(Lock {
            input: Box::new(input),
            clauses,
            outside,
        })
    };
    if is_block(&plan) {
        return lock(plan, clauses);
    }
    match plan {
        Plan::Limit(mut limit) => {
            limit.input = Box::new(wrap(*limit.input, clauses, outside));
            Plan::Limit(limit)
        }
        Plan::Sort(mut sort) => {
            sort.input = Box::new(wrap(*sort.input, clauses, outside));
            Plan::Sort(sort)
        }
        Plan::SetOperation(mut operation) => {
            operation.right = Box::new(wrap(*operation.right, clauses, outside));
            Plan::SetOperation(operation)
        }
        Plan::With(mut with) => {
            with.body = Box::new(wrap(*with.body, clauses, outside));
            Plan::With(with)
        }
        other => lock(other, clauses),
    }
}

/// Tells whether `plan` is a query block: a `SELECT`
/// (`Limit(Distinct(Project(...)))`), a `TABLE` or a `VALUES`, with its
/// sort and its limit (`Limit(Sort(Scan))`).
fn is_block(plan: &Plan) -> bool {
    let mut node = plan;
    if let Plan::Limit(limit) = node {
        node = &limit.input;
    }
    match node {
        Plan::Distinct(distinct) => matches!(distinct.input.as_ref(), Plan::Project(_)),
        Plan::Project(_) => true,
        Plan::Sort(sort) => is_statement(&sort.input),
        other => is_statement(other),
    }
}

/// Tells whether `plan` is a `TABLE` or a `VALUES` statement.
fn is_statement(plan: &Plan) -> bool {
    matches!(plan, Plan::Scan(_) | Plan::CteRef(_) | Plan::Values(_))
}
