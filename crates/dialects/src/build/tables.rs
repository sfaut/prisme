//! Tables a statement reads, whose columns the builder needs: those of its
//! sources, of its derived tables, of its set operations and of the
//! subqueries of its expressions, common table expressions left out.

use prisme_plan::{Ident, TableName};

use super::{same_name, table_name};
use crate::sql::ast::{self, ExprKind, Query, QueryBody, SelectItem, StatementKind, TableRef};

/// Tables `statement` reads, each one once, in the order written.
pub fn referenced_tables(statement: &ast::Statement) -> Vec<TableName> {
    let mut collector = Collector::default();
    match &statement.kind {
        StatementKind::Query(query) => collector.query(query),
    }
    collector.tables
}

/// State of the walk.
#[derive(Default)]
struct Collector {
    /// Common table expressions in scope, the innermost last.
    ctes: Vec<Ident>,
    /// Tables found.
    tables: Vec<TableName>,
}

impl Collector {
    /// Query, with its common table expressions.
    fn query(&mut self, query: &Query) {
        let in_scope = self.ctes.len();
        if let Some(with) = &query.with {
            for definition in &with.ctes {
                // A recursive expression reads itself.
                if with.recursive {
                    self.ctes.push(definition.name.clone());
                    self.query(&definition.query);
                } else {
                    self.query(&definition.query);
                    self.ctes.push(definition.name.clone());
                }
            }
        }
        self.body(&query.body);
        for item in &query.order_by {
            self.expr(&item.expr);
        }
        self.ctes.truncate(in_scope);
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
            TableRef::Derived { query, .. } => self.query(query),
            TableRef::Join(join) => {
                self.table_ref(&join.left);
                self.table_ref(&join.right);
                if let Some(ast::JoinConstraint::On(condition)) = &join.constraint {
                    self.expr(condition);
                }
            }
            TableRef::Nested { inner, .. } => self.table_ref(inner),
        }
    }

    /// Table named, unless a common table expression in scope has its name.
    fn table(&mut self, name: &ast::ObjectName) {
        let is_cte =
            name.parts.len() == 1 && self.ctes.iter().any(|cte| same_name(cte, &name.parts[0]));
        if is_cte {
            return;
        }
        let table = table_name(name);
        if !self.tables.contains(&table) {
            self.tables.push(table);
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
