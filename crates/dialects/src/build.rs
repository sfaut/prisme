//! Plan builder of the MySQL family: syntax tree to logical plan
//! (`docs/DEV-DIALECTS.md`, step 4).
//!
//! The builder gives the tree its meaning: names resolved against the
//! columns of the tables (read from the catalog of the connection) and of
//! the common table expressions, types of the MySQL family
//! (`types` module), explicit conversions between booleans and numbers,
//! NULL position of the sort keys (NULL is the smallest value).
//!
//! A `SELECT` gives the nodes `Limit(Distinct(Project(Sort(Filter(source)))))`,
//! each one when its clause is written: the sort comes before the
//! projection, its keys being expressions of the source, as an `ORDER BY`
//! can name columns the select list leaves out; a key that names an alias
//! or a position of the select list takes its expression, and keeps the
//! form written.

mod expr;
mod scope;
mod types;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::fmt;

use prisme_plan::{
    AliasForm, Cte, CteRef, Direction, Distinct, Filter, Ident, Limit, NamedExpr, Nulls, Plan,
    PlanError, Project, ProjectItem, Scan, Sort, SortForm, SortKey, SortReference, Span,
    TableAlias, TableName, Values as ValuesNode, ValuesForm, Wildcard, With,
};
use prisme_types::Type;

use crate::Dialect;
use crate::sql::ast::{
    self, CteDefinition, ExprKind, Literal, OrderItem, Query, QueryBody, Select, SelectItem,
    StatementKind, TableRef,
};
use scope::Scope;

/// Columns of the tables of the database, read on demand for the tables of a
/// query (`docs/QUESTIONS.md`, point 44).
pub trait Catalog {
    /// Columns of `table` in `database` (the current database when `None`),
    /// with their type; `None` when the table is unknown.
    fn columns(&self, database: Option<&Ident>, table: &Ident) -> Option<Vec<(Ident, Type)>>;
}

/// Catalog held in memory: the columns of the tables read for a query.
/// Names are compared without case.
#[derive(Debug, Clone, Default)]
pub struct Tables {
    /// Columns, by database (empty for the current one) and table, in lower
    /// case.
    tables: HashMap<(String, String), Vec<(Ident, Type)>>,
}

impl Tables {
    /// Adds the columns of a table.
    pub fn insert(&mut self, database: Option<&str>, table: &str, columns: Vec<(Ident, Type)>) {
        self.tables.insert(key(database, table), columns);
    }
}

impl Catalog for Tables {
    fn columns(&self, database: Option<&Ident>, table: &Ident) -> Option<Vec<(Ident, Type)>> {
        let database = database.map(|database| database.value.as_str());
        self.tables.get(&key(database, &table.value)).cloned()
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

/// Builds the plan of a statement written in `dialect`, `text` being the
/// text it was read from.
///
/// # Errors
///
/// `BuildError` on an unknown table or column, an ambiguous column, rows of
/// `VALUES` of different lengths.
pub fn build(
    statement: &ast::Statement,
    text: &str,
    dialect: Dialect,
    catalog: &dyn Catalog,
) -> Result<prisme_plan::Statement, BuildError> {
    let mut builder = Builder {
        text,
        dialect,
        catalog,
        ctes: Vec::new(),
    };
    match &statement.kind {
        StatementKind::Query(query) => Ok(prisme_plan::Statement::Query(builder.query(query)?)),
    }
}

/// Tables a statement reads, common table expressions aside: those whose
/// columns the catalog must give.
pub fn referenced_tables(statement: &ast::Statement) -> Vec<TableName> {
    let mut tables = Vec::new();
    match &statement.kind {
        StatementKind::Query(query) => collect_tables(query, &mut Vec::new(), &mut tables),
    }
    tables
}

/// Adds to `tables` the tables of `query`, `ctes` being the names of the
/// common table expressions in scope.
fn collect_tables(query: &Query, ctes: &mut Vec<Ident>, tables: &mut Vec<TableName>) {
    let in_scope = ctes.len();
    if let Some(with) = &query.with {
        for definition in &with.ctes {
            collect_tables(&definition.query, ctes, tables);
            ctes.push(definition.name.clone());
        }
    }
    match &query.body {
        QueryBody::Select(select) => {
            if let Some(TableRef::Named { name, .. }) = &select.from {
                let is_cte =
                    name.parts.len() == 1 && ctes.iter().any(|cte| same_name(cte, &name.parts[0]));
                if !is_cte {
                    let table = table_name(name);
                    if !tables.contains(&table) {
                        tables.push(table);
                    }
                }
            }
        }
        QueryBody::Nested(nested) => collect_tables(nested, ctes, tables),
        QueryBody::Values(_) => {}
    }
    ctes.truncate(in_scope);
}

/// Name of a table: `t` or `db.t`.
fn table_name(name: &ast::ObjectName) -> TableName {
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
    /// Columns of the tables.
    catalog: &'a dyn Catalog,
    /// Common table expressions in scope, the innermost last.
    ctes: Vec<Cte>,
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

    /// Query, with its common table expressions, its sort and its limit.
    fn query(&mut self, query: &Query) -> Result<Plan, BuildError> {
        let in_scope = self.ctes.len();
        let mut ctes = Vec::new();
        if let Some(with) = &query.with {
            for definition in &with.ctes {
                let cte = self.cte(definition)?;
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
            recursive: false,
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

    /// Body of a query, then its sort and its limit.
    fn query_body(&mut self, query: &Query) -> Result<Plan, BuildError> {
        let plan = match &query.body {
            QueryBody::Select(select) => return self.select(select, query),
            QueryBody::Values(values) => Plan::Values(self.values(values)?),
            QueryBody::Nested(nested) => self.query(nested)?,
        };
        let plan = if query.order_by.is_empty() {
            plan
        } else {
            // The keys of a `VALUES` or of a query between parentheses name
            // its output columns.
            let scope = Scope::new(plan.schema());
            let outputs = output_columns(&scope);
            let keys = self.sort_keys(&query.order_by, &scope, &outputs)?;
            Plan::Sort(Sort {
                input: Box::new(plan),
                keys,
                span: None,
            })
        };
        self.limit(plan, query)
    }

    /// `LIMIT` of a query, around `plan`.
    fn limit(&mut self, plan: Plan, query: &Query) -> Result<Plan, BuildError> {
        let Some(limit) = &query.limit else {
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
    fn select(&mut self, select: &Select, query: &Query) -> Result<Plan, BuildError> {
        let mut plan = match &select.from {
            Some(table) => self.table_ref(table)?,
            None => dual(),
        };
        let scope = Scope::new(plan.schema());
        if let Some(filter) = &select.filter {
            let predicate = self.expr(filter, &scope)?;
            plan = Plan::Filter(Filter {
                input: Box::new(plan),
                predicate: as_predicate(predicate),
                span: Some(filter.span),
            });
        }
        let mut items = Vec::new();
        let mut outputs = Vec::new();
        for item in &select.items {
            match item {
                SelectItem::Wildcard { qualifier, span } => {
                    let qualifier = qualifier
                        .as_ref()
                        .map(|name| &name.parts[name.parts.len() - 1]);
                    let columns = scope
                        .wildcard(qualifier)
                        .ok_or_else(|| self.error("unknown table in `.*`", *span))?;
                    outputs.extend(columns.iter().map(|&position| scope.output(position)));
                    items.push(ProjectItem::Wildcard(Wildcard {
                        qualifier: qualifier.cloned(),
                        columns,
                    }));
                }
                SelectItem::Expr { expr, alias, .. } => {
                    let built = self.value(expr, &scope)?;
                    let (name, form) = match alias {
                        Some(alias) => (alias.name.clone(), alias.form),
                        None => (implicit_name(expr, self.text), AliasForm::Implicit),
                    };
                    outputs.push(Output {
                        alias: alias.as_ref().map(|alias| alias.name.clone()),
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
        if !query.order_by.is_empty() {
            let keys = self.sort_keys(&query.order_by, &scope, &outputs)?;
            plan = Plan::Sort(Sort {
                input: Box::new(plan),
                keys,
                span: None,
            });
        }
        let mut project =
            Project::new(plan, items).map_err(|error| plan_error(&error, select.span))?;
        project.span = Some(select.span);
        let mut plan = Plan::Project(project);
        if select.distinct {
            plan = Plan::Distinct(Distinct {
                input: Box::new(plan),
                span: None,
            });
        }
        self.limit(plan, query)
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
                let alias = outputs.iter().find(|output| {
                    output
                        .alias
                        .as_ref()
                        .is_some_and(|alias| same_name(alias, &name.parts[0]))
                });
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
                .map(|value| implicit_name(value, self.text))
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

    /// Source of rows of a `FROM`: a common table expression in scope, or a
    /// table of the catalog.
    fn table_ref(&mut self, table: &TableRef) -> Result<Plan, BuildError> {
        let TableRef::Named { name, alias, span } = table;
        let alias = alias.as_ref().map(|alias| TableAlias {
            name: alias.name.clone(),
            columns: Vec::new(),
            form: alias.form,
        });
        if let [single] = name.parts.as_slice()
            && let Some(cte) = self
                .ctes
                .iter()
                .rev()
                .find(|cte| same_name(&cte.name, single))
        {
            let mut cte_ref = CteRef::new(cte, alias).map_err(|error| plan_error(&error, *span))?;
            cte_ref.name = single.clone();
            cte_ref.span = Some(*span);
            return Ok(Plan::CteRef(cte_ref));
        }
        let table = table_name(name);
        let columns = self
            .catalog
            .columns(table.database.as_ref(), &table.name)
            .ok_or_else(|| {
                let written = &self.text[name.span.start..name.span.end];
                self.error(format!("unknown table `{written}`"), name.span)
            })?;
        Ok(Plan::Scan(Scan {
            table,
            alias,
            columns,
            span: Some(*span),
        }))
    }
}

/// Column of the select list, for the keys of an `ORDER BY`.
#[derive(Debug, Clone)]
struct Output {
    /// Alias written.
    alias: Option<Ident>,
    /// Value.
    expr: prisme_plan::Expr,
}

/// Columns of a scope, as outputs without alias.
fn output_columns(scope: &Scope) -> Vec<Output> {
    (0..scope.len())
        .map(|position| scope.output(position))
        .collect()
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

/// Name the MySQL family gives to a column without alias: the name of a
/// column, the value of a string, the text written otherwise.
fn implicit_name(expr: &ast::Expr, text: &str) -> Ident {
    match &expr.kind {
        ExprKind::Column(name) => name.parts[name.parts.len() - 1].clone(),
        ExprKind::Literal(Literal::String(value)) => Ident::new(value.clone()),
        _ => Ident::new(&text[expr.span.start..expr.span.end]),
    }
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
