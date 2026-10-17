//! Statements: a query, a write or a session statement.
//!
//! Writes are always at the root and to a single table
//! (`docs/FEDERATED.md`, section 5.1): the read / write classification reads
//! the root. The session statements (`USE`, `SET`, `EXPLAIN`) are translated
//! as far as needed (rule 2 of `CLAUDE.md`).

use crate::{ColumnRef, Expr, Ident, Plan, SortKey, Span, TableAlias, TableName, Variable};

/// Statement of the plan.
#[derive(Debug, Clone, PartialEq)]
pub enum Statement {
    /// Query that returns rows.
    Query(Plan),
    /// `INSERT`.
    Insert(Insert),
    /// `UPDATE`.
    Update(Update),
    /// `DELETE`.
    Delete(Delete),
    /// `USE`.
    Use(UseDatabase),
    /// `SET` of variables.
    Set(SetVariables),
    /// `EXPLAIN` of a statement.
    Explain(Explain),
}

/// Rows added to a table.
#[derive(Debug, Clone, PartialEq)]
pub struct Insert {
    /// Table written.
    pub table: TableName,
    /// Columns written, in order; empty for all of them.
    pub columns: Vec<Ident>,
    /// Rows added: a `Values` or a query.
    pub source: Plan,
    /// Assignments of `ON DUPLICATE KEY UPDATE` (MySQL family); empty when
    /// not written.
    pub on_duplicate: Vec<Assignment>,
    /// Position in the input.
    pub span: Option<Span>,
}

/// Rows of a table modified.
#[derive(Debug, Clone, PartialEq)]
pub struct Update {
    /// Table written.
    pub table: TableName,
    /// Alias of the table.
    pub alias: Option<TableAlias>,
    /// Columns modified, with their new value.
    pub assignments: Vec<Assignment>,
    /// Rows modified (`WHERE`); all of them when `None`.
    pub filter: Option<Expr>,
    /// Order of the modifications (`ORDER BY` of the MySQL family).
    pub order_by: Vec<SortKey>,
    /// Number of rows modified at most (`LIMIT` of the MySQL family).
    pub limit: Option<Expr>,
    /// Position in the input.
    pub span: Option<Span>,
}

/// Rows of a table removed.
#[derive(Debug, Clone, PartialEq)]
pub struct Delete {
    /// Table written.
    pub table: TableName,
    /// Alias of the table.
    pub alias: Option<TableAlias>,
    /// Rows removed (`WHERE`); all of them when `None`.
    pub filter: Option<Expr>,
    /// Order of the removals (`ORDER BY` of the MySQL family).
    pub order_by: Vec<SortKey>,
    /// Number of rows removed at most (`LIMIT` of the MySQL family).
    pub limit: Option<Expr>,
    /// Position in the input.
    pub span: Option<Span>,
}

/// New value of a column.
#[derive(Debug, Clone, PartialEq)]
pub struct Assignment {
    /// Column modified.
    pub column: ColumnRef,
    /// New value.
    pub value: Expr,
}

/// Database of the session changed (`USE db`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UseDatabase {
    /// New database.
    pub database: Ident,
    /// Position in the input.
    pub span: Option<Span>,
}

/// Variables of the session or of the server changed (`SET`).
#[derive(Debug, Clone, PartialEq)]
pub struct SetVariables {
    /// Assignments, in order.
    pub assignments: Vec<VariableAssignment>,
    /// Position in the input.
    pub span: Option<Span>,
}

/// New value of a variable.
#[derive(Debug, Clone, PartialEq)]
pub struct VariableAssignment {
    /// Variable changed.
    pub variable: Variable,
    /// New value.
    pub value: Expr,
}

/// Execution plan of the server for a statement (`EXPLAIN`).
#[derive(Debug, Clone, PartialEq)]
pub struct Explain {
    /// Statement explained.
    pub statement: Box<Statement>,
    /// Position in the input.
    pub span: Option<Span>,
}
