//! Plan builder of the MySQL family: syntax tree to logical plan
//! (`docs/DEV-DIALECTS.md`, steps 4 and 5).
//!
//! The builder gives the tree its meaning: names resolved against the
//! columns of the tables (read from the catalog of the connection), of the
//! common table expressions and of the derived tables, then of the enclosing
//! queries for a subquery (outer references); types of the MySQL family
//! (`types` module), explicit conversions between booleans and numbers,
//! NULL position of the sort keys (NULL is the smallest value).
//!
//! A `SELECT` gives the nodes
//! `Limit(Distinct(Project(Sort(Window(Filter(Aggregate(Filter(source))))))))`,
//! each one when its clause is written: the sort comes before the
//! projection, its keys being expressions of the source, as an `ORDER BY`
//! can name columns the select list leaves out; a key that names an alias
//! or a position of the select list takes its expression, and keeps the
//! form written. The source is a table, a common table expression, a derived
//! table or a join of sources; a set operation joins query bodies. The
//! aggregation, its `HAVING` and the window functions are described in the
//! `group` module; a recursive common table expression builds its anchors
//! before its recursive parts (`recursive` module).

mod call;
mod cast;
mod expr;
pub(crate) mod functions;
mod group;
mod interval;
mod json;
mod lock;
mod now;
mod recursive;
mod scope;
mod string;
mod tables;
mod temporal;
pub(crate) mod types;
mod variable;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::fmt;

use prisme_plan::{
    AliasForm, Comparison, Cte, CtePart, CteRef, Derived, Direction, Distinct, Expr, Filter,
    FilterForm, Ident, Join, JoinKind, JoinUsing, Limit, Logical, NamedExpr, Nulls, Plan,
    PlanError, Project, ProjectItem, Scan, SetOperation, SetOperator, Sort, SortForm, SortKey,
    SortReference, Span, TableAlias, TableName, Values as ValuesNode, ValuesForm, Wildcard, With,
};
use prisme_types::Type;

use crate::Dialect;
use crate::sql::ast::{
    self, CteDefinition, ExprKind, JoinConstraint, Literal, ObjectName, OrderItem, Query,
    QueryBody, Select, SelectItem, StatementKind, TableRef,
};
use crate::versions::{Construct, Version, registry};
pub(crate) use group::is_internal_name;
use group::{Above, Block, Clause};
use scope::{ResolveError, Scope};
pub use tables::{referenced_tables, referenced_user_variables};

/// Columns of the tables of the database, read on demand for the tables of a
/// query, and types of the user variables of the connection
/// (`docs/QUESTIONS.md`, point 44).
pub trait Catalog {
    /// Columns of `table` in `database` (the current database when `None`),
    /// with their type; `None` when the table is unknown.
    fn columns(&self, database: Option<&Ident>, table: &Ident) -> Option<Vec<(Ident, Type)>>;

    /// Type of the value of the user variable `name` (without `@`) on the
    /// connection; `None` when not read.
    fn user_variable(&self, _name: &str) -> Option<Type> {
        None
    }
}

/// Catalog held in memory: the columns of the tables read for a query, and
/// the types of its user variables. Names are compared without case.
#[derive(Debug, Clone, Default)]
pub struct Tables {
    /// Columns, by database (empty for the current one) and table, in lower
    /// case.
    tables: HashMap<(String, String), Vec<(Ident, Type)>>,
    /// Types of the user variables, by name in lower case.
    variables: HashMap<String, Type>,
}

impl Tables {
    /// Adds the columns of a table.
    pub fn insert(&mut self, database: Option<&str>, table: &str, columns: Vec<(Ident, Type)>) {
        self.tables.insert(key(database, table), columns);
    }

    /// Adds the type of the user variable `name`, without `@`.
    pub fn insert_user_variable(&mut self, name: &str, ty: Type) {
        self.variables.insert(name.to_lowercase(), ty);
    }
}

impl Catalog for Tables {
    fn columns(&self, database: Option<&Ident>, table: &Ident) -> Option<Vec<(Ident, Type)>> {
        let database = database.map(|database| database.value.as_str());
        self.tables.get(&key(database, &table.value)).cloned()
    }

    fn user_variable(&self, name: &str) -> Option<Type> {
        self.variables.get(&name.to_lowercase()).cloned()
    }
}

/// Key of a table in `Tables`.
fn key(database: Option<&str>, table: &str) -> (String, String) {
    (
        database.unwrap_or_default().to_lowercase(),
        table.to_lowercase(),
    )
}

/// Query whose meaning the builder cannot give: unknown name, ambiguous
/// column, rows of different lengths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildError {
    /// What is wrong.
    pub message: String,
    /// Position of the faulty text.
    pub span: Span,
}

impl fmt::Display for BuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for BuildError {}

/// Builds the plan of a statement written in `dialect` and its `version`,
/// `text` being the text it was read from.
///
/// # Errors
///
/// `BuildError` on an unknown table or column, an ambiguous column, rows or
/// sides of different lengths, a construct the version refuses.
pub fn build(
    statement: &ast::Statement,
    text: &str,
    dialect: Dialect,
    version: Version,
    catalog: &dyn Catalog,
) -> Result<prisme_plan::Statement, BuildError> {
    let mut builder = Builder {
        text,
        dialect,
        version,
        catalog,
        ctes: Vec::new(),
        outer: Vec::new(),
        derived: Vec::new(),
        blocks: Vec::new(),
    };
    match &statement.kind {
        StatementKind::Query(query) => Ok(prisme_plan::Statement::Query(builder.query(query)?)),
    }
}

/// Name of a table: `t` or `db.t`.
fn table_name(name: &ObjectName) -> TableName {
    match name.parts.as_slice() {
        [database, table] => TableName {
            database: Some(database.clone()),
            name: table.clone(),
        },
        parts => TableName {
            database: None,
            name: parts[parts.len() - 1].clone(),
        },
    }
}

/// Position of the first subquery of `expr`.
fn subquery_span(expr: &ast::Expr) -> Option<Span> {
    match &expr.kind {
        ExprKind::Subquery(_)
        | ExprKind::Exists(_)
        | ExprKind::InSubquery { .. }
        | ExprKind::Quantified { .. } => Some(expr.span),
        _ => expr.children().into_iter().find_map(subquery_span),
    }
}

/// Tells whether two names designate the same object, without case.
fn same_name(left: &Ident, right: &Ident) -> bool {
    left.value.eq_ignore_ascii_case(&right.value)
}

/// State of the builder.
struct Builder<'a> {
    /// Text the tree was read from.
    text: &'a str,
    /// Dialect of the query.
    dialect: Dialect,
    /// Version of the dialect.
    version: Version,
    /// Columns of the tables.
    catalog: &'a dyn Catalog,
    /// Common table expressions in scope, the innermost last.
    ctes: Vec<Cte>,
    /// Scopes of the enclosing queries of a subquery, the innermost last.
    outer: Vec<Scope>,
    /// Derived tables being built, the innermost last.
    derived: Vec<DerivedFrame>,
    /// `SELECT` blocks being built, the innermost last; each query starts
    /// with an empty one, so that its expressions do not read the
    /// aggregation of an enclosing block.
    blocks: Vec<Block>,
}

/// Derived table being built.
#[derive(Debug, Clone, Copy)]
struct DerivedFrame {
    /// Number of enclosing scopes outside the derived table.
    outside: usize,
    /// A column of one of them was read.
    outer_references: bool,
}

/// Error of the plan crate, at `span`.
fn plan_error(error: &PlanError, span: Span) -> BuildError {
    BuildError {
        message: error.to_string(),
        span,
    }
}

impl Builder<'_> {
    /// Error at `span`.
    fn error(&self, message: impl Into<String>, span: Span) -> BuildError {
        BuildError {
            message: message.into(),
            span,
        }
    }

    /// Checks that the dialect has `construct` in its version.
    fn check(&self, construct: Construct, span: Span) -> Result<(), BuildError> {
        registry(self.dialect)
            .check(self.dialect, construct, self.version)
            .map_err(|error| self.error(error.to_string(), span))
    }

    /// Tells whether the dialect has `construct` in its version.
    fn has(&self, construct: Construct) -> bool {
        (registry(self.dialect))
            .check(self.dialect, construct, self.version)
            .is_ok()
    }

    /// Text written at `span`.
    fn written(&self, span: Span) -> &str {
        &self.text[span.start..span.end]
    }

    /// Column designated by `name`, in `scope` then in the scopes of the
    /// enclosing queries.
    fn resolve(
        &mut self,
        name: &ObjectName,
        scope: &Scope,
    ) -> Result<(prisme_plan::ColumnRef, Type), BuildError> {
        let ambiguous = |builder: &Self| {
            let written = builder.written(name.span);
            builder.error(format!("column `{written}` is ambiguous"), name.span)
        };
        match scope.resolve(name) {
            Ok(found) => return Ok(found),
            Err(ResolveError::Ambiguous) => return Err(ambiguous(self)),
            Err(ResolveError::Unknown) => {}
        }
        for index in (0..self.outer.len()).rev() {
            match self.outer[index].resolve(name) {
                Ok(found) => {
                    for frame in &mut self.derived {
                        if index < frame.outside {
                            frame.outer_references = true;
                        }
                    }
                    return Ok(found);
                }
                Err(ResolveError::Ambiguous) => return Err(ambiguous(self)),
                Err(ResolveError::Unknown) => {}
            }
        }
        let written = self.written(name.span);
        Err(self.error(format!("unknown column `{written}`"), name.span))
    }

    /// Plan of a subquery of an expression, which reads the columns of
    /// `scope`.
    fn subquery(&mut self, query: &Query, scope: &Scope) -> Result<Plan, BuildError> {
        self.outer.push(scope.clone());
        let plan = self.query(query);
        self.outer.pop();
        plan
    }

    /// Query, with its common table expressions, its sort and its limit.
    fn query(&mut self, query: &Query) -> Result<Plan, BuildError> {
        self.blocks.push(Block::default());
        let plan = self.query_with(query);
        self.blocks.pop();
        plan
    }

    /// Query, with its common table expressions, its sort and its limit, in
    /// its own block.
    fn query_with(&mut self, query: &Query) -> Result<Plan, BuildError> {
        let in_scope = self.ctes.len();
        let mut ctes = Vec::new();
        if let Some(with) = &query.with {
            for (index, definition) in with.ctes.iter().enumerate() {
                // MariaDB reads a later expression there, MySQL a table.
                if with.recursive
                    && let Some(later) = (with.ctes[index + 1..].iter())
                        .find(|later| recursive::reads(&definition.query, &later.name))
                {
                    let message = format!(
                        "`{}`, the name of a common table expression after `{}` in `WITH \
                         RECURSIVE`: not supported yet by Prisme",
                        later.name.value, definition.name.value
                    );
                    return Err(self.error(message, definition.span));
                }
                let cte = if with.recursive && recursive::reads(&definition.query, &definition.name)
                {
                    self.recursive_cte(definition)?
                } else {
                    self.cte(definition)?
                };
                self.ctes.push(cte.clone());
                ctes.push(cte);
            }
        }
        let body = self.query_body(query);
        self.ctes.truncate(in_scope);
        let body = body?;
        if ctes.is_empty() {
            return Ok(body);
        }
        Ok(Plan::With(With {
            recursive: query.with.as_ref().is_some_and(|with| with.recursive),
            ctes,
            body: Box::new(body),
            span: query.with.as_ref().map(|with| with.span),
        }))
    }

    /// Common table expression.
    fn cte(&mut self, definition: &CteDefinition) -> Result<Cte, BuildError> {
        let plan = self.query(&definition.query)?;
        let mut cte = Cte::new(definition.name.clone(), definition.columns.clone(), plan)
            .map_err(|error| plan_error(&error, definition.span))?;
        cte.span = Some(definition.span);
        Ok(cte)
    }

    /// Recursive common table expression, in a block of its own: a `UNION`
    /// whose parts that do not read it, the anchors, are built first. Its
    /// columns take the type of the anchors, nullable, which the other parts
    /// read: MySQL and MariaDB store the rows of each iteration in columns of
    /// that type (`crates/probe/cases/with-recursive.sql`).
    fn recursive_cte(&mut self, definition: &CteDefinition) -> Result<Cte, BuildError> {
        self.blocks.push(Block::default());
        let cte = self.recursive_parts(definition);
        self.blocks.pop();
        cte
    }

    /// Parts of the recursive common table expression `definition`, joined
    /// in the order written, then its sort and its limit.
    fn recursive_parts(&mut self, definition: &CteDefinition) -> Result<Cte, BuildError> {
        let (name, span, query) = (&definition.name, definition.span, &definition.query);
        if query.with.is_some() {
            return Err(self.error(
                "`WITH` inside a recursive common table expression: not supported yet by Prisme",
                span,
            ));
        }
        let Some(parts) = recursive::union_parts(&query.body) else {
            return Err(self.error(
                "a recursive common table expression of another set operation than `UNION`: \
                 not supported yet by Prisme",
                span,
            ));
        };
        if let Some(span) = parts.iter().find_map(|part| lock::written_lock(part.body)) {
            return Err(self.error(
                "a locking clause in a part of a recursive common table expression: not \
                 supported yet by Prisme",
                span,
            ));
        }
        let readings: Vec<CtePart> = (parts.iter())
            .map(|part| recursive::part_reading(part.body, name))
            .collect();
        let reading: Vec<bool> = readings.iter().map(|part| part.recursive).collect();
        if !reading.contains(&false) {
            let message = format!(
                "the recursive common table expression `{}` has no part that does not read it",
                name.value
            );
            return Err(self.error(message, span));
        }
        if reading[0] && definition.columns.is_empty() {
            return Err(self.error(
                "a recursive common table expression read by its first part, without column \
                 list: not supported yet by Prisme",
                span,
            ));
        }
        let mut plans = Vec::with_capacity(parts.len());
        for (part, reads) in parts.iter().zip(&reading) {
            plans.push(if *reads {
                None
            } else {
                Some(self.body(part.body)?)
            });
        }
        let schemas: Vec<_> = plans.iter().flatten().map(Plan::schema).collect();
        let width = schemas[0].len();
        if let Some(other) = schemas.iter().find(|schema| schema.len() != width) {
            return Err(self.error(
                format!(
                    "the parts of the common table expression have {width} and {} columns",
                    other.len()
                ),
                span,
            ));
        }
        let first = plans.iter().flatten().next().expect("an anchor").clone();
        let mut columns = Cte::new(name.clone(), definition.columns.clone(), first)
            .map_err(|error| plan_error(&error, span))?;
        for (index, column) in columns.schema.columns.iter_mut().enumerate() {
            let anchors = schemas
                .iter()
                .map(|schema| &schema.columns[index].ty.data_type);
            column.ty = Type::nullable(types::aggregate(anchors));
        }
        self.ctes.push(columns.clone());
        let mut built = Vec::with_capacity(parts.len());
        for (part, plan) in parts.iter().zip(plans) {
            match plan.map_or_else(|| self.body(part.body), Ok) {
                Ok(plan) => built.push(plan),
                Err(error) => {
                    self.ctes.pop();
                    return Err(error);
                }
            }
        }
        self.ctes.pop();
        let mut built = built.into_iter();
        let mut plan = built.next().expect("a first part");
        for (part, right) in parts.iter().skip(1).zip(built) {
            let (all, union_span) = part.union.expect("a `UNION` before each later part");
            let operator = if all {
                SetOperator::UnionAll
            } else {
                SetOperator::UnionDistinct
            };
            plan = self.combine(operator, plan, right, union_span)?;
        }
        let plan = self.sorted(plan, &query.order_by)?;
        if let Some(limit) = &query.limit {
            self.check(Construct::RecursiveLimit, limit.span)?;
        }
        if let Some(lock) = query.locks.first() {
            return Err(self.error(
                "a locking clause after the parts of a recursive common table expression: not \
                 supported yet by Prisme",
                lock.span,
            ));
        }
        let plan = self.limit(plan, query.limit.as_ref())?;
        let mut cte = Cte::new(name.clone(), definition.columns.clone(), plan)
            .map_err(|error| plan_error(&error, span))?;
        cte.schema = columns.schema;
        cte.parts = readings;
        cte.span = Some(span);
        Ok(cte)
    }

    /// Body of a query, then its sort and its limit.
    fn query_body(&mut self, query: &Query) -> Result<Plan, BuildError> {
        self.lock_placement(query)?;
        if let Some(select) = self.sorted_select(query)? {
            let plan = self.select(select, &query.order_by, query.limit.as_ref())?;
            // The locking clauses between the parentheses lock the same
            // block.
            let mut locks: Vec<&ast::Locking> = Vec::new();
            let mut body = &query.body;
            while let QueryBody::Nested(inner) = body {
                locks.extend(&inner.locks);
                body = &inner.body;
            }
            if let (Some(_), Some(lock)) = (locks.first(), query.locks.first()) {
                return Err(self.several_locks(lock.span));
            }
            let outside = matches!(query.body, QueryBody::Nested(_)) && !query.locks.is_empty();
            locks.extend(&query.locks);
            return self.locked(plan, &locks, outside);
        }
        let plan = self.body(&query.body)?;
        let plan = self.sorted(plan, &query.order_by)?;
        let plan = lock::joined_limit(self.limit(plan, query.limit.as_ref())?);
        let outside = matches!(query.body, QueryBody::Nested(_));
        self.locked(plan, &query.locks.iter().collect::<Vec<_>>(), outside)
    }

    /// `SELECT` that the `ORDER BY` of `query` sorts as its own: its body,
    /// or a `SELECT` between parentheses without `ORDER BY` or `LIMIT` of its
    /// own, whose block the outer `ORDER BY` sorts by the rule measured
    /// (`crates/probe/cases/alias-references.sql`): MariaDB and MySQL read
    /// its names as those of an `ORDER BY` of the `SELECT`, a column of the
    /// source first inside an expression (`(SELECT a AS b FROM t) ORDER BY
    /// b + 0` sorts by the column `b`, `(SELECT id FROM t) ORDER BY a` by a
    /// column outside the select list), except MySQL 8.0.31 to 8.0.41, 8.1.0
    /// to 8.4.4 and 9.0.0 to 9.2.0, which read the columns of the result
    /// (construct `parenthesized_order_result`). With an `ORDER BY` or a
    /// `LIMIT` between the parentheses, the plan reads the columns of the
    /// result, as MySQL and MariaDB do, the exceptions of MariaDB aside
    /// (`emit::parentheses`).
    ///
    /// # Errors
    ///
    /// `BuildError` on a `WITH` between the parentheses, and, from MySQL
    /// 8.0.42, 8.4.5 and 9.3.0, on a subquery in the outer `ORDER BY`, whose
    /// names these versions read as the columns of the result
    /// (`parenthesized_order_subquery`): not supported yet.
    fn sorted_select<'q>(&self, query: &'q Query) -> Result<Option<&'q ast::Select>, BuildError> {
        let mut body = &query.body;
        if matches!(body, QueryBody::Nested(_))
            && (query.order_by.is_empty() || self.has(Construct::ParenthesizedOrderResult))
        {
            return Ok(None);
        }
        while let QueryBody::Nested(inner) = body {
            if !inner.order_by.is_empty() || inner.limit.is_some() {
                return Ok(None);
            }
            if let Some(with) = &inner.with {
                return Err(self.error(
                    "`WITH` between parentheses followed by an `ORDER BY`: not supported yet by \
                     Prisme",
                    with.span,
                ));
            }
            body = &inner.body;
        }
        let QueryBody::Select(select) = body else {
            return Ok(None);
        };
        if matches!(query.body, QueryBody::Nested(_))
            && self.has(Construct::ParenthesizedOrderSubquery)
            && let Some(span) = (query.order_by.iter()).find_map(|item| subquery_span(&item.expr))
        {
            let message = format!(
                "a subquery in an `ORDER BY` after a `SELECT` between parentheses, whose names \
                 {} {} reads as the columns of the result, and the other names of that `ORDER \
                 BY` as those of the `SELECT`: not supported yet by Prisme",
                self.dialect.label(),
                self.version
            );
            return Err(self.error(message, span));
        }
        Ok(Some(select))
    }

    /// `plan` sorted by `order_by` when written: the keys after a `VALUES`,
    /// a `TABLE`, a set operation or a query between parentheses name its
    /// output columns.
    fn sorted(&mut self, plan: Plan, order_by: &[OrderItem]) -> Result<Plan, BuildError> {
        if order_by.is_empty() {
            return Ok(plan);
        }
        let scope = Scope::new(plan.schema());
        let outputs = scope.outputs();
        let keys = self.sort_keys(order_by, &scope, &outputs)?;
        Ok(Plan::Sort(Sort {
            input: Box::new(plan),
            keys,
            span: None,
        }))
    }

    /// Body of a query, without sort or limit of its own.
    fn body(&mut self, body: &QueryBody) -> Result<Plan, BuildError> {
        match body {
            QueryBody::Select(select) => self.select(select, &[], None),
            QueryBody::Values(values) => Ok(Plan::Values(self.values(values)?)),
            QueryBody::Table(name) => self.named(name, None, &[], &[], name.span),
            QueryBody::Nested(query) => self.query(query),
            QueryBody::SetOperation(operation) => self.set_operation(operation),
        }
    }

    /// `UNION`, `EXCEPT` or `INTERSECT`: the columns take their names from
    /// the left side and a type that holds the values of both sides.
    fn set_operation(&mut self, operation: &ast::SetOperation) -> Result<Plan, BuildError> {
        let left = self.body(&operation.left)?;
        let right = self.body(&operation.right)?;
        let operator = match (operation.operator, operation.all) {
            (ast::SetOperator::Union, true) => SetOperator::UnionAll,
            (ast::SetOperator::Union, false) => SetOperator::UnionDistinct,
            (ast::SetOperator::Except, true) => SetOperator::MinusAll,
            (ast::SetOperator::Except, false) => SetOperator::MinusDistinct,
            (ast::SetOperator::Intersect, true) => SetOperator::IntersectAll,
            (ast::SetOperator::Intersect, false) => SetOperator::IntersectDistinct,
        };
        self.combine(operator, left, right, operation.span)
    }

    /// Set operation `operator` of `left` and `right`, at `span`.
    fn combine(
        &mut self,
        operator: SetOperator,
        left: Plan,
        right: Plan,
        span: Span,
    ) -> Result<Plan, BuildError> {
        let (left_schema, right_schema) = (left.schema(), right.schema());
        if left_schema.len() != right_schema.len() {
            return Err(self.error(
                format!(
                    "the sides of the set operation have {} and {} columns",
                    left_schema.len(),
                    right_schema.len()
                ),
                span,
            ));
        }
        let data_types = left_schema
            .columns
            .iter()
            .zip(&right_schema.columns)
            .map(|(left, right)| types::aggregate([&left.ty.data_type, &right.ty.data_type]))
            .collect();
        let mut node = SetOperation::new(operator, left, right, data_types)
            .map_err(|error| plan_error(&error, span))?;
        node.span = Some(span);
        Ok(Plan::SetOperation(node))
    }

    /// `LIMIT` of a query, around `plan`.
    fn limit(&mut self, plan: Plan, limit: Option<&ast::Limit>) -> Result<Plan, BuildError> {
        let Some(limit) = limit else {
            return Ok(plan);
        };
        let scope = Scope::empty();
        let count = self.expr(&limit.count, &scope)?;
        let offset = match &limit.offset {
            Some(offset) => Some(self.expr(offset, &scope)?),
            None => None,
        };
        Ok(Plan::Limit(Limit {
            input: Box::new(plan),
            count: Some(count),
            offset,
            form: limit.form,
            span: Some(limit.span),
        }))
    }

    /// `SELECT` block, with the sort and the limit of its query.
    fn select(
        &mut self,
        select: &Select,
        order_by: &[OrderItem],
        limit: Option<&ast::Limit>,
    ) -> Result<Plan, BuildError> {
        self.blocks.push(Block::default());
        let plan = self.select_block(select, order_by);
        self.blocks.pop();
        self.limit(plan?, limit)
    }

    /// `SELECT` block, with the sort of its query.
    fn select_block(
        &mut self,
        select: &Select,
        order_by: &[OrderItem],
    ) -> Result<Plan, BuildError> {
        let (mut plan, scope) = match &select.from {
            Some(table) => self.source(table, None)?,
            None => (dual(), Scope::empty()),
        };
        if let Some(filter) = &select.filter {
            self.set_clause(Clause::Where);
            let predicate = self.condition(filter, &scope)?;
            plan = Plan::Filter(Filter {
                input: Box::new(plan),
                predicate,
                form: FilterForm::Where,
                span: Some(filter.span),
            });
        }
        let grouped = group::is_grouped(select, order_by);
        let block = self.block_mut();
        block.grouped = grouped;
        block.keyed = !select.group_by.is_empty();
        block.source = scope.clone();
        block.alias_items = (select.items.iter())
            .filter_map(|item| match item {
                ast::SelectItem::Expr {
                    expr,
                    alias: Some(alias),
                    ..
                } => Some((alias.name.clone(), expr.clone())),
                _ => None,
            })
            .collect();
        self.set_clause(Clause::Window);
        self.window_definitions(&select.windows, &scope)?;
        self.set_clause(Clause::Select);
        let (mut items, outputs) = self.select_items(&select.items, &scope)?;
        self.block_mut().outputs.clone_from(&outputs);
        self.set_clause(Clause::GroupBy);
        let keys = self.group_keys(&select.group_by, &scope, &outputs)?;
        self.block_mut().key_columns = (keys.iter())
            .filter_map(|(expr, _)| match &expr.kind {
                prisme_plan::ExprKind::Column(column) => scope.canonical(column),
                _ => None,
            })
            .collect();
        let mut having = match &select.having {
            Some(having) => {
                self.set_clause(Clause::Having);
                Some(self.having(having, &scope)?)
            }
            None => None,
        };
        let mut sort = Vec::new();
        if !order_by.is_empty() {
            self.set_clause(Clause::OrderBy);
            sort = self.sort_keys(order_by, &scope, &outputs)?;
        }
        self.set_clause(Clause::Other);
        if grouped {
            let above = Above {
                items: &mut items,
                having: having.as_mut(),
                sort: &mut sort,
            };
            plan = self.aggregate_node(plan, keys, select.rollup, above);
        }
        if let Some(predicate) = having {
            plan = Plan::Filter(Filter {
                input: Box::new(plan),
                span: select.having.as_ref().map(|having| having.span),
                predicate,
                form: FilterForm::Having,
            });
        }
        plan = self.window_node(plan);
        if !sort.is_empty() {
            plan = Plan::Sort(Sort {
                input: Box::new(plan),
                keys: sort,
                span: None,
            });
        }
        let mut project =
            Project::new(plan, items).map_err(|error| plan_error(&error, select.span))?;
        project.span = Some(select.span);
        project.options.clone_from(&select.options);
        let mut plan = Plan::Project(project);
        if select.distinct {
            plan = Plan::Distinct(Distinct {
                input: Box::new(plan),
                span: None,
            });
        }
        Ok(plan)
    }

    /// Select list, with its columns for the keys of `ORDER BY`, `GROUP BY`
    /// and `HAVING`.
    fn select_items(
        &mut self,
        select_items: &[SelectItem],
        scope: &Scope,
    ) -> Result<(Vec<ProjectItem>, Vec<Output>), BuildError> {
        let mut items = Vec::new();
        let mut outputs = Vec::new();
        for item in select_items {
            match item {
                SelectItem::Wildcard { qualifier, span } => {
                    let qualifier = qualifier
                        .as_ref()
                        .map(|name| &name.parts[name.parts.len() - 1]);
                    let columns = scope
                        .wildcard(qualifier)
                        .ok_or_else(|| self.error("unknown table in `.*`", *span))?;
                    let (positions, columns): (Vec<usize>, Vec<Output>) =
                        columns.into_iter().unzip();
                    // Above the aggregation, the columns of the source that
                    // it computes (`aggregate_node`), named as the source
                    // names them.
                    let names = match self.block().grouped {
                        true => (columns.iter())
                            .filter_map(|output| match &output.expr.kind {
                                prisme_plan::ExprKind::Column(column) => Some(column.clone()),
                                _ => None,
                            })
                            .collect(),
                        false => Vec::new(),
                    };
                    outputs.extend(columns);
                    items.push(ProjectItem::Wildcard(Wildcard {
                        qualifier: qualifier.cloned(),
                        columns: positions,
                        names,
                    }));
                }
                SelectItem::Expr { expr, alias, .. } => {
                    let built = self.value(expr, scope)?;
                    let (name, form) = match alias {
                        Some(alias) => (alias.name.clone(), alias.form),
                        None => (
                            implicit_name(expr, self.text, self.dialect),
                            AliasForm::Implicit,
                        ),
                    };
                    outputs.push(Output {
                        alias: alias.as_ref().map(|alias| alias.name.clone()),
                        name: name.clone(),
                        expr: built.clone(),
                    });
                    items.push(ProjectItem::Expr(NamedExpr {
                        expr: built,
                        name,
                        alias: form,
                    }));
                }
            }
        }
        Ok((items, outputs))
    }

    /// Keys of an `ORDER BY`: a position of the select list, an alias of the
    /// select list, or an expression of the source, in this order (the
    /// rule of the MySQL family).
    fn sort_keys(
        &mut self,
        items: &[OrderItem],
        scope: &Scope,
        outputs: &[Output],
    ) -> Result<Vec<SortKey>, BuildError> {
        items
            .iter()
            .map(|item| {
                let (expr, reference) = self.sort_value(item, scope, outputs)?;
                let direction = item.direction.unwrap_or(Direction::Ascending);
                Ok(SortKey {
                    expr,
                    direction,
                    nulls: Nulls::as_smallest(direction),
                    form: SortForm {
                        direction: item.direction.is_some(),
                        nulls: false,
                        reference,
                    },
                })
            })
            .collect()
    }

    /// Value of a sort key, and how it designates it.
    fn sort_value(
        &mut self,
        item: &OrderItem,
        scope: &Scope,
        outputs: &[Output],
    ) -> Result<(prisme_plan::Expr, SortReference), BuildError> {
        match &item.expr.kind {
            ExprKind::Literal(Literal::Integer(text)) => {
                let position = text.parse::<usize>().unwrap_or(0);
                let output = position
                    .checked_sub(1)
                    .and_then(|index| outputs.get(index))
                    .ok_or_else(|| {
                        self.error(
                            format!("unknown column `{text}` in `ORDER BY`"),
                            item.expr.span,
                        )
                    })?;
                Ok((output.expr.clone(), SortReference::Position))
            }
            ExprKind::Column(name) if name.parts.len() == 1 => {
                let mut aliases = outputs.iter().filter(|output| {
                    output
                        .alias
                        .as_ref()
                        .is_some_and(|alias| same_name(alias, &name.parts[0]))
                });
                let alias = aliases.next();
                if aliases.next().is_some() {
                    let message = format!(
                        "`{}` is ambiguous: several items of the select list have this alias",
                        name.parts[0].value
                    );
                    return Err(self.error(message, name.span));
                }
                match alias {
                    Some(output) => Ok((output.expr.clone(), SortReference::Alias)),
                    None => Ok((self.value(&item.expr, scope)?, SortReference::Expression)),
                }
            }
            _ => Ok((self.value(&item.expr, scope)?, SortReference::Expression)),
        }
    }

    /// `VALUES`: its columns are named `column_0`, `column_1`... in MySQL,
    /// after the text of the values of the first row in MariaDB.
    fn values(&mut self, values: &ast::Values) -> Result<ValuesNode, BuildError> {
        let scope = Scope::empty();
        let mut rows = Vec::new();
        for row in &values.rows {
            let built = row
                .values
                .iter()
                .map(|value| self.value(value, &scope))
                .collect::<Result<Vec<_>, _>>()?;
            if let Some(first) = rows.first().map(Vec::len)
                && built.len() != first
            {
                return Err(self.error(
                    "the rows of `VALUES` do not have the same number of columns",
                    row.span,
                ));
            }
            rows.push(built);
        }
        let first = &values.rows[0].values;
        let names = match self.dialect {
            Dialect::MySql => (0..first.len())
                .map(|index| Ident::new(format!("column_{index}")))
                .collect(),
            Dialect::MariaDb => first
                .iter()
                .map(|value| implicit_name(value, self.text, Dialect::MariaDb))
                .collect(),
        };
        let data_types = (0..first.len())
            .map(|column| types::aggregate(rows.iter().map(|row| &row[column].ty.data_type)))
            .collect();
        let mut node = ValuesNode::new(rows, names, data_types, values.form)
            .map_err(|error| plan_error(&error, values.span))?;
        node.span = Some(values.span);
        Ok(node)
    }

    /// Source of a `FROM`, with its columns; `left` is the scope of the
    /// sources before it, which a `LATERAL` derived table reads.
    fn source(
        &mut self,
        table: &TableRef,
        left: Option<&Scope>,
    ) -> Result<(Plan, Scope), BuildError> {
        match table {
            TableRef::Named {
                name,
                partitions,
                alias,
                index_hints,
                span,
            } => {
                let alias = alias.as_ref().map(|alias| TableAlias {
                    name: alias.name.clone(),
                    columns: Vec::new(),
                    form: alias.form,
                });
                let plan = self.named(name, alias, partitions, index_hints, *span)?;
                let scope = Scope::new(plan.schema());
                Ok((plan, scope))
            }
            TableRef::Derived {
                query,
                lateral,
                alias,
                columns,
                span,
            } => {
                let plan = self.derived(query, *lateral, alias, columns, *span, left)?;
                let scope = Scope::new(plan.schema());
                Ok((plan, scope))
            }
            TableRef::Join(join) => self.join(join, left),
            TableRef::Nested { inner, .. } => self.source(inner, left),
        }
    }

    /// Table or common table expression, by its name.
    fn named(
        &mut self,
        name: &ObjectName,
        alias: Option<TableAlias>,
        partitions: &[Ident],
        index_hints: &[prisme_plan::IndexHint],
        span: Span,
    ) -> Result<Plan, BuildError> {
        if let [single] = name.parts.as_slice()
            && let Some(cte) = self
                .ctes
                .iter()
                .rev()
                .find(|cte| same_name(&cte.name, single))
        {
            if !partitions.is_empty() || !index_hints.is_empty() {
                return Err(self.error(
                    "a common table expression has neither partitions nor indexes",
                    span,
                ));
            }
            let mut cte_ref = CteRef::new(cte, alias).map_err(|error| plan_error(&error, span))?;
            cte_ref.name = single.clone();
            cte_ref.span = Some(span);
            return Ok(Plan::CteRef(cte_ref));
        }
        let table = table_name(name);
        let columns = self
            .catalog
            .columns(table.database.as_ref(), &table.name)
            .ok_or_else(|| {
                let written = self.written(name.span);
                self.error(format!("unknown table `{written}`"), name.span)
            })?;
        let mut scan = Scan::new(table, columns);
        scan.alias = alias;
        scan.partitions = partitions.to_vec();
        scan.index_hints = index_hints.to_vec();
        scan.span = Some(span);
        Ok(Plan::Scan(scan))
    }

    /// Derived table: its subquery reads the columns of the enclosing
    /// queries, and those of the sources before it when `LATERAL`.
    fn derived(
        &mut self,
        query: &Query,
        lateral: bool,
        alias: &ast::Alias,
        columns: &[Ident],
        span: Span,
        left: Option<&Scope>,
    ) -> Result<Plan, BuildError> {
        self.derived.push(DerivedFrame {
            outside: self.outer.len(),
            outer_references: false,
        });
        let lateral_scope = lateral.then(|| left.cloned().unwrap_or_default());
        if let Some(scope) = lateral_scope {
            self.outer.push(scope);
        }
        let plan = self.query(query);
        if lateral {
            self.outer.pop();
        }
        let frame = self.derived.pop().expect("pushed above");
        let plan = plan?;
        if frame.outer_references {
            self.check(Construct::DerivedOuterReference, span)?;
        }
        let alias = TableAlias {
            name: alias.name.clone(),
            columns: columns.to_vec(),
            form: alias.form,
        };
        let mut derived = Derived::new(plan, alias).map_err(|error| plan_error(&error, span))?;
        derived.lateral = lateral;
        derived.outer_references = frame.outer_references;
        derived.span = Some(span);
        Ok(Plan::Derived(derived))
    }

    /// Join of two sources.
    fn join(
        &mut self,
        join: &ast::Join,
        left: Option<&Scope>,
    ) -> Result<(Plan, Scope), BuildError> {
        let (left_plan, left_scope) = self.source(&join.left, left)?;
        let (right_plan, right_scope) = self.source(&join.right, Some(&left_scope))?;
        let left_len = left_plan.schema().len();
        let mut kind = match join.kind {
            ast::JoinKind::Inner => JoinKind::Inner,
            ast::JoinKind::Left => JoinKind::Left,
            ast::JoinKind::Right => JoinKind::Right,
        };
        let (left_scope, right_scope) = match kind {
            JoinKind::Left => (left_scope, right_scope.nullable()),
            JoinKind::Right => (left_scope.nullable(), right_scope),
            _ => (left_scope, right_scope),
        };
        let names = match (&join.constraint, join.natural) {
            (_, true) => Some(Scope::common_names(&left_scope, &right_scope)),
            (Some(JoinConstraint::Using(names)), false) => Some(names.clone()),
            _ => None,
        };
        let (scope, condition, using) = match names {
            Some(names) => {
                let (scope, pairs) = Scope::using(left_scope, right_scope, left_len, &names, kind)
                    .map_err(|(error, name)| {
                        let message = match error {
                            ResolveError::Unknown => {
                                format!("unknown column `{}` in `USING`", name.value)
                            }
                            ResolveError::Ambiguous => {
                                format!("column `{}` is ambiguous in `USING`", name.value)
                            }
                        };
                        self.error(message, join.span)
                    })?;
                let condition = pairs
                    .into_iter()
                    .map(|(left, right)| Expr::compare(Comparison::Eq, left, right))
                    .reduce(|all, next| Expr::logical(Logical::And, all, next));
                let using = JoinUsing {
                    columns: names,
                    natural: join.natural,
                };
                (scope, condition, Some(using))
            }
            None => {
                let scope = Scope::concat(left_scope, right_scope, left_len);
                let condition = match &join.constraint {
                    Some(JoinConstraint::On(predicate)) => Some(self.condition(predicate, &scope)?),
                    _ => None,
                };
                (scope, condition, None)
            }
        };
        if kind == JoinKind::Inner && condition.is_none() {
            kind = JoinKind::Cross;
        }
        let plan = Plan::Join(Join {
            left: Box::new(left_plan),
            right: Box::new(right_plan),
            kind,
            condition,
            using,
            form: join.form,
            span: Some(join.span),
        });
        Ok((plan, scope))
    }
}

/// Column of the select list, for the keys of an `ORDER BY`, a `GROUP BY`
/// and the aliases of `HAVING`.
#[derive(Debug, Clone)]
struct Output {
    /// Alias written.
    alias: Option<Ident>,
    /// Name of the column: the alias, or the name the dialect gives without
    /// alias.
    name: Ident,
    /// Value.
    expr: prisme_plan::Expr,
}

/// Source of a `SELECT` without `FROM`: one row without columns.
fn dual() -> Plan {
    let values = ValuesNode::new(vec![Vec::new()], Vec::new(), Vec::new(), ValuesForm::Row)
        .expect("one empty row has no columns");
    Plan::Values(values)
}

/// Tells whether `plan` is the source of a `SELECT` without `FROM`.
pub(crate) fn is_dual(plan: &Plan) -> bool {
    matches!(plan, Plan::Values(values) if values.schema.is_empty() && values.rows.len() == 1)
}

/// Name `dialect` gives to a column without alias: the name of a column,
/// the value of a string, national or introduced (`N'a'` and `_latin1'a'`
/// are named `a`), of its first string in MySQL (`'a' 'b'` is named `a`,
/// `ab` in MariaDB), the text written otherwise.
fn implicit_name(expr: &ast::Expr, text: &str, dialect: Dialect) -> Ident {
    let string = match &expr.kind {
        ExprKind::Column(name) => return name.parts[name.parts.len() - 1].clone(),
        ExprKind::Literal(Literal::String(strings) | Literal::National(strings)) => strings,
        ExprKind::Literal(Literal::Introduced { literal, .. })
            if let Literal::String(strings) = literal.as_ref() =>
        {
            strings
        }
        _ => return Ident::new(&text[expr.span.start..expr.span.end]),
    };
    Ident::new(match dialect {
        Dialect::MySql => string.first(),
        Dialect::MariaDb => &string.value,
    })
}

/// Predicate from an expression: a number becomes a boolean, true when not
/// zero (`WHERE a`).
fn as_predicate(expr: prisme_plan::Expr) -> prisme_plan::Expr {
    match expr.ty.data_type {
        prisme_types::DataType::Boolean | prisme_types::DataType::Null => expr,
        _ => {
            let span = expr.span;
            let converted = prisme_plan::Expr::to_boolean(expr);
            match span {
                Some(span) => converted.with_span(span),
                None => converted,
            }
        }
    }
}
