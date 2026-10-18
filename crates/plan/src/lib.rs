//! Logical plan of Prisme: the pivot between the parsers of the input
//! dialects and the emitters of the output dialects
//! (`docs/DEV-DIALECTS.md`, section 1).
//!
//! A query becomes a `Statement`: a relational `Plan`, a tree of nodes each
//! with its output schema, or a write or session statement, always at the
//! root. Each expression carries its type with its nullability.
//!
//! The rules that hold for every dialect are applied here, when a node or an
//! expression is built: the two equalities, three-valued logic, the
//! nullability of joins, of aggregates and of set operations, the explicit
//! position of NULLs in a sort (`docs/FEDERATED.md`, sections 5.1 and 7). The
//! types that depend on a dialect (type of a literal, of an arithmetic
//! operation, of a function, of the columns of a `VALUES` or of a `UNION`) are
//! given by the plan builder of the dialect.
//!
//! Each node keeps its origin: its position in the input and the form written
//! by the user when several forms have the same meaning (`origin` module).

mod cast;
mod error;
mod expr;
mod interval;
mod order;
mod origin;
mod plan;
mod schema;
mod statement;
mod string;

pub use cast::{Cast, CastForm, CastTarget, CharTarget, CharWord, Charset, IntegerWord};
pub use error::PlanError;
pub use expr::{
    Arithmetic, Bitwise, CallForm, Case, CaseBranch, ColumnRef, Comparison, Expr, ExprKind, Frame,
    FrameBound, FrameUnits, FunctionCall, Logical, NotDistinctForm, Quantifier, Truth, Variable,
    VariableScope, WindowCall,
};
pub use interval::{DateArithmetic, DateArithmeticForm, Extract, TimeUnit, TimestampDiff};
pub use order::{Direction, Nulls, SortForm, SortKey, SortReference};
pub use origin::{AliasForm, Ident, Span};
pub use plan::{
    Aggregate, AggregateCall, Cte, CtePart, CteRef, Derived, Distinct, Extend, Filter, IndexHint,
    IndexHintAction, IndexHintScope, Join, JoinForm, JoinKind, JoinUsing, Limit, LimitForm,
    NamedAggregate, NamedExpr, OnEmpty, Plan, Project, ProjectItem, Scan, SetOperation,
    SetOperator, Sort, TableAlias, TableName, Values, ValuesForm, Wildcard, Window,
    WindowDefinition, With,
};
pub use schema::{Column, Schema};
pub use statement::{
    Assignment, Delete, Explain, Insert, SetVariables, Statement, Update, UseDatabase,
    VariableAssignment,
};
pub use string::{Position, Trim, TrimSide};
