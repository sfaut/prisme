//! Recursive common table expressions (`WITH RECURSIVE`): the parts of the
//! `UNION` of an expression that reads itself, and how each part reads it.
//! The parts that do not read it, the anchors, give its columns; the others,
//! the recursive parts, read the rows that the iteration before them added
//! (`crates/probe/cases/with-recursive.sql`).

use prisme_plan::{CtePart, Ident, Span};

use super::same_name;
use crate::sql::ast::{self, ExprKind, Query, QueryBody, SelectItem, SetOperator, TableRef};

/// Part of the `UNION` of a recursive common table expression.
pub struct Part<'a> {
    /// Body of the part.
    pub body: &'a QueryBody,
    /// `UNION` that joins it to the parts before it: `ALL` or not, and its
    /// position; `None` for the first part.
    pub union: Option<(bool, Span)>,
}

/// Parts of `body`, a `UNION` of query bodies, in the order written; `None`
/// when another set operator joins two of them (`EXCEPT`, `INTERSECT`).
pub fn union_parts(body: &QueryBody) -> Option<Vec<Part<'_>>> {
    let QueryBody::SetOperation(operation) = body else {
        return Some(vec![Part { body, union: None }]);
    };
    if operation.operator != SetOperator::Union {
        return None;
    }
    let mut parts = union_parts(&operation.left)?;
    let mut right = union_parts(&operation.right)?;
    right[0].union = Some((operation.all, operation.span));
    parts.append(&mut right);
    Some(parts)
}

/// Tells whether `query` reads the common table expression `name`, an inner
/// common table expression of the same name hiding it.
pub fn reads(query: &Query, name: &Ident) -> bool {
    let mut finder = Finder::new(name);
    finder.query(query);
    finder.part.recursive
}

/// How the part `body` reads the common table expression `name`.
pub fn part_reading(body: &QueryBody, name: &Ident) -> CtePart {
    let mut finder = Finder::new(name);
    finder.body(body);
    finder.part
}

/// State of the search.
struct Finder<'a> {
    /// Name of the common table expression.
    name: &'a Ident,
    /// Readings found.
    part: CtePart,
    /// The search is inside a derived table.
    derived: bool,
    /// The search is on the inner side of an outer join.
    outer_join: bool,
}

impl<'a> Finder<'a> {
    /// Search of the readings of `name`.
    fn new(name: &'a Ident) -> Self {
        Self {
            name,
            part: CtePart::default(),
            derived: false,
            outer_join: false,
        }
    }
}

impl Finder<'_> {
    /// Query, whose common table expressions of the name hide it from the
    /// definitions after them and from its body.
    fn query(&mut self, query: &Query) {
        if let Some(with) = &query.with {
            for definition in &with.ctes {
                let hides = same_name(&definition.name, self.name);
                if !(hides && with.recursive) {
                    self.query(&definition.query);
                }
                if hides {
                    return;
                }
            }
        }
        self.body(&query.body);
        for item in &query.order_by {
            self.expr(&item.expr);
        }
    }

    /// Body of a query.
    fn body(&mut self, body: &QueryBody) {
        match body {
            QueryBody::Select(select) => {
                for item in &select.items {
                    if let SelectItem::Expr { expr, .. } = item {
                        self.expr(expr);
                    }
                }
                if let Some(from) = &select.from {
                    self.table_ref(from);
                }
                let clauses = select
                    .filter
                    .iter()
                    .chain(&select.group_by)
                    .chain(&select.having);
                for expr in clauses {
                    self.expr(expr);
                }
                for window in &select.windows {
                    window
                        .spec
                        .children()
                        .into_iter()
                        .for_each(|expr| self.expr(expr));
                }
            }
            QueryBody::Values(values) => {
                for row in &values.rows {
                    for value in &row.values {
                        self.expr(value);
                    }
                }
            }
            QueryBody::Table(name) => self.table(name),
            QueryBody::Nested(query) => self.query(query),
            QueryBody::SetOperation(operation) => {
                self.body(&operation.left);
                self.body(&operation.right);
            }
        }
    }

    /// Source of a `FROM`.
    fn table_ref(&mut self, table: &TableRef) {
        match table {
            TableRef::Named { name, .. } => self.table(name),
            TableRef::Derived { query, .. } => {
                let derived = std::mem::replace(&mut self.derived, true);
                self.query(query);
                self.derived = derived;
            }
            TableRef::Join(join) => {
                let outer_join = self.outer_join;
                self.outer_join = outer_join || join.kind == ast::JoinKind::Right;
                self.table_ref(&join.left);
                self.outer_join = outer_join || join.kind == ast::JoinKind::Left;
                self.table_ref(&join.right);
                self.outer_join = outer_join;
                if let Some(ast::JoinConstraint::On(condition)) = &join.constraint {
                    self.expr(condition);
                }
            }
            TableRef::Nested { inner, .. } => self.table_ref(inner),
        }
    }

    /// Table named.
    fn table(&mut self, name: &ast::ObjectName) {
        if let [single] = name.parts.as_slice()
            && same_name(single, self.name)
        {
            self.part.recursive = true;
            self.part.derived |= self.derived;
            self.part.outer_join |= self.outer_join;
        }
    }

    /// Subqueries of an expression.
    fn expr(&mut self, expr: &ast::Expr) {
        if let ExprKind::Subquery(query)
        | ExprKind::Exists(query)
        | ExprKind::InSubquery { query, .. }
        | ExprKind::Quantified { query, .. } = &expr.kind
        {
            self.query(query);
        }
        for child in expr.children() {
            self.expr(child);
        }
    }
}
