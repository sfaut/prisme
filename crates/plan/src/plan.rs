//! Relational nodes of the plan (`docs/FEDERATED.md`, section 5.1) and their
//! output schema.
//!
//! The schema of a node is derived from its inputs, with the nullability
//! rules decided for every dialect: an outer join makes the other side
//! nullable, an aggregate without grouping keys can give NULL, a set
//! operation is nullable when one side is. The nodes whose column types
//! depend on the dialect (`VALUES`, set operations) or whose names are checked
//! (aliases with a column list) compute their schema when built.

mod aggregate;
mod join;
mod set;
mod source;
mod unary;

pub use aggregate::{Aggregate, AggregateCall, NamedAggregate, OnEmpty};
pub use join::{Join, JoinForm, JoinKind, JoinUsing};
pub use set::{SetOperation, SetOperator};
pub use source::{
    Cte, CtePart, CteRef, Derived, IndexHint, IndexHintAction, IndexHintScope, Scan, TableAlias,
    TableName, Values, ValuesForm, With,
};
pub use unary::{
    Distinct, Extend, Filter, FilterForm, Limit, LimitForm, NamedExpr, Project, ProjectItem, Sort,
    Wildcard, Window, WindowDefinition,
};

use crate::{Schema, Span};

/// Relational node: a relation computed from its inputs.
#[derive(Debug, Clone, PartialEq)]
pub enum Plan {
    /// Rows of a table.
    Scan(Scan),
    /// Rows written in the query (`VALUES`).
    Values(Values),
    /// Rows of a common table expression, by its name.
    CteRef(CteRef),
    /// Common table expressions, then the query that uses them (`WITH`).
    With(With),
    /// Subquery of the `FROM`, with its alias.
    Derived(Derived),
    /// Rows for which a predicate is true (`WHERE`, `HAVING`).
    Filter(Filter),
    /// Columns computed from each row (`SELECT` list).
    Project(Project),
    /// Columns of the input, plus computed ones.
    Extend(Extend),
    /// One row per group (`GROUP BY` and aggregate functions).
    Aggregate(Aggregate),
    /// Columns of the input, plus window functions.
    Window(Window),
    /// Rows in order (`ORDER BY`).
    Sort(Sort),
    /// Part of the rows (`LIMIT`, `OFFSET`).
    Limit(Limit),
    /// Rows without duplicates (`DISTINCT`), NULLs being identical.
    Distinct(Distinct),
    /// Join of two inputs.
    Join(Join),
    /// Union, difference or intersection of two inputs.
    SetOperation(SetOperation),
}

impl Plan {
    /// Columns output by the node, in order, with their type.
    pub fn schema(&self) -> Schema {
        match self {
            Self::Scan(scan) => scan.schema(),
            Self::Values(values) => values.schema.clone(),
            Self::CteRef(cte_ref) => cte_ref.schema.clone(),
            Self::With(with) => with.body.schema(),
            Self::Derived(derived) => derived.schema.clone(),
            Self::Filter(filter) => filter.input.schema(),
            Self::Project(project) => project.schema(),
            Self::Extend(extend) => extend.schema(),
            Self::Aggregate(aggregate) => aggregate.schema(),
            Self::Window(window) => window.schema(),
            Self::Sort(sort) => sort.input.schema(),
            Self::Limit(limit) => limit.input.schema(),
            Self::Distinct(distinct) => distinct.input.schema(),
            Self::Join(join) => join.schema(),
            Self::SetOperation(operation) => operation.schema.clone(),
        }
    }

    /// Position of the node in the input; `None` for a node written by
    /// Prisme.
    pub fn span(&self) -> Option<Span> {
        match self {
            Self::Scan(scan) => scan.span,
            Self::Values(values) => values.span,
            Self::CteRef(cte_ref) => cte_ref.span,
            Self::With(with) => with.span,
            Self::Derived(derived) => derived.span,
            Self::Filter(filter) => filter.span,
            Self::Project(project) => project.span,
            Self::Extend(extend) => extend.span,
            Self::Aggregate(aggregate) => aggregate.span,
            Self::Window(window) => window.span,
            Self::Sort(sort) => sort.span,
            Self::Limit(limit) => limit.span,
            Self::Distinct(distinct) => distinct.span,
            Self::Join(join) => join.span,
            Self::SetOperation(operation) => operation.span,
        }
    }
}
