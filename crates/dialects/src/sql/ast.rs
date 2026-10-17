//! Syntax tree of the SQL dialects, as written: each node keeps its position
//! and, when several forms have the same meaning, the form written.
//!
//! The tree is common to MySQL and MariaDB; the nodes that only one dialect
//! has are refused by the parser of the other one. It stays close to the
//! text, and the plan builder gives it its meaning (names resolved, types,
//! explicit conversions).

use prisme_plan::{AliasForm, Direction, Ident, LimitForm, Span, ValuesForm};

/// Statement.
#[derive(Debug, Clone, PartialEq)]
pub struct Statement {
    /// What the statement does.
    pub kind: StatementKind,
    /// Position in the input.
    pub span: Span,
}

/// Kind of statement.
#[derive(Debug, Clone, PartialEq)]
pub enum StatementKind {
    /// Query: `SELECT`, `VALUES`, `WITH`.
    Query(Query),
}

/// Query, with its common table expressions, its sort and its limit.
#[derive(Debug, Clone, PartialEq)]
pub struct Query {
    /// `WITH`.
    pub with: Option<With>,
    /// Body.
    pub body: QueryBody,
    /// `ORDER BY`.
    pub order_by: Vec<OrderItem>,
    /// `LIMIT`.
    pub limit: Option<Limit>,
    /// Position in the input.
    pub span: Span,
}

/// `WITH` clause.
#[derive(Debug, Clone, PartialEq)]
pub struct With {
    /// `WITH RECURSIVE`.
    pub recursive: bool,
    /// Common table expressions, in order.
    pub ctes: Vec<CteDefinition>,
    /// Position in the input.
    pub span: Span,
}

/// Common table expression: `name (columns) AS (query)`.
#[derive(Debug, Clone, PartialEq)]
pub struct CteDefinition {
    /// Name.
    pub name: Ident,
    /// Column list; empty when not written.
    pub columns: Vec<Ident>,
    /// Query.
    pub query: Box<Query>,
    /// Position in the input.
    pub span: Span,
}

/// Body of a query.
#[derive(Debug, Clone, PartialEq)]
pub enum QueryBody {
    /// `SELECT`.
    Select(Box<Select>),
    /// `VALUES`.
    Values(Values),
    /// Query between parentheses.
    Nested(Box<Query>),
}

/// `VALUES` statement or table value constructor.
#[derive(Debug, Clone, PartialEq)]
pub struct Values {
    /// Rows.
    pub rows: Vec<Row>,
    /// `VALUES ROW (...)` or `VALUES (...)`.
    pub form: ValuesForm,
    /// Position in the input.
    pub span: Span,
}

/// Row of a `VALUES`.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    /// Values.
    pub values: Vec<Expr>,
    /// Position in the input.
    pub span: Span,
}

/// `SELECT` block.
#[derive(Debug, Clone, PartialEq)]
pub struct Select {
    /// `DISTINCT` (or `DISTINCTROW`).
    pub distinct: bool,
    /// Items of the select list.
    pub items: Vec<SelectItem>,
    /// `FROM`; `None` without it or with `FROM DUAL`.
    pub from: Option<TableRef>,
    /// `WHERE`.
    pub filter: Option<Expr>,
    /// Position in the input.
    pub span: Span,
}

/// Item of a select list.
#[derive(Debug, Clone, PartialEq)]
pub enum SelectItem {
    /// `*` or `t.*`.
    Wildcard {
        /// Table written before `.*`.
        qualifier: Option<ObjectName>,
        /// Position in the input.
        span: Span,
    },
    /// Expression, with its alias.
    Expr {
        /// Expression.
        expr: Expr,
        /// Alias; `None` without one.
        alias: Option<Alias>,
        /// Position in the input, alias included.
        span: Span,
    },
}

/// Alias of a column or a table.
#[derive(Debug, Clone, PartialEq)]
pub struct Alias {
    /// Name.
    pub name: Ident,
    /// With or without `AS`.
    pub form: AliasForm,
}

/// Name made of parts separated by dots: `t`, `db.t`, `t.a`, `db.t.a`.
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectName {
    /// Parts, in order.
    pub parts: Vec<Ident>,
    /// Position in the input.
    pub span: Span,
}

/// Source of rows in a `FROM`.
#[derive(Debug, Clone, PartialEq)]
pub enum TableRef {
    /// Table or common table expression, by name.
    Named {
        /// Name.
        name: ObjectName,
        /// Alias.
        alias: Option<Alias>,
        /// Position in the input.
        span: Span,
    },
}

/// Key of an `ORDER BY`.
#[derive(Debug, Clone, PartialEq)]
pub struct OrderItem {
    /// Value sorted.
    pub expr: Expr,
    /// `ASC` or `DESC`; `None` when not written.
    pub direction: Option<Direction>,
    /// Position in the input.
    pub span: Span,
}

/// `LIMIT` clause.
#[derive(Debug, Clone, PartialEq)]
pub struct Limit {
    /// Number of rows.
    pub count: Expr,
    /// Rows skipped first.
    pub offset: Option<Expr>,
    /// `LIMIT n OFFSET m` or `LIMIT m, n`.
    pub form: LimitForm,
    /// Position in the input.
    pub span: Span,
}

/// Expression.
#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    /// What the expression is.
    pub kind: ExprKind,
    /// Position in the input.
    pub span: Span,
}

/// Kind of expression.
#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    /// Column, possibly qualified.
    Column(ObjectName),
    /// Literal.
    Literal(Literal),
    /// Prefix operator.
    Unary {
        /// Operator.
        op: UnaryOp,
        /// Operand.
        operand: Box<Expr>,
    },
    /// Binary operator.
    Binary {
        /// Operator.
        op: BinaryOp,
        /// Left operand.
        left: Box<Expr>,
        /// Right operand.
        right: Box<Expr>,
    },
    /// `IS [NOT] NULL`.
    IsNull {
        /// Value tested.
        operand: Box<Expr>,
        /// `IS NOT NULL`.
        negated: bool,
    },
    /// `[NOT] BETWEEN low AND high`.
    Between {
        /// Value tested.
        operand: Box<Expr>,
        /// Lower bound.
        low: Box<Expr>,
        /// Upper bound.
        high: Box<Expr>,
        /// `NOT BETWEEN`.
        negated: bool,
    },
    /// `[NOT] IN (values)`.
    InList {
        /// Value tested.
        operand: Box<Expr>,
        /// Values of the list.
        list: Vec<Expr>,
        /// `NOT IN`.
        negated: bool,
    },
    /// `[NOT] LIKE pattern [ESCAPE escape]`.
    Like {
        /// Value tested.
        operand: Box<Expr>,
        /// Pattern.
        pattern: Box<Expr>,
        /// Escape character.
        escape: Option<Box<Expr>>,
        /// `NOT LIKE`.
        negated: bool,
    },
    /// Expression between parentheses.
    Nested(Box<Expr>),
}

/// Literal.
#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    /// Integer, as written.
    Integer(String),
    /// Exact decimal, as written.
    Decimal(String),
    /// Approximate number, as written.
    Float(String),
    /// String, adjacent strings joined (`'a' 'b'` is `'ab'`).
    String(String),
    /// `NULL`.
    Null,
    /// `TRUE`.
    True,
    /// `FALSE`.
    False,
}

/// Prefix operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UnaryOp {
    /// `-`.
    Minus,
    /// `+`.
    Plus,
    /// `NOT`, or `!`.
    Not,
    /// `~`.
    BitNot,
}

/// Binary operator. Synonyms (`<>` and `!=`, `AND` and `&&`, `%` and `MOD`)
/// are one operator: they are spellings, written again in the canonical one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BinaryOp {
    /// `+`.
    Add,
    /// `-`.
    Subtract,
    /// `*`.
    Multiply,
    /// `/`.
    Divide,
    /// `DIV`.
    IntegerDivide,
    /// `%`, `MOD`.
    Modulo,
    /// `=`.
    Eq,
    /// `<>`, `!=`.
    NotEq,
    /// `<`.
    Lt,
    /// `<=`.
    LtEq,
    /// `>`.
    Gt,
    /// `>=`.
    GtEq,
    /// `<=>`.
    NullSafeEq,
    /// `AND`, `&&`.
    And,
    /// `OR`, `||` without `PIPES_AS_CONCAT`.
    Or,
    /// `XOR`.
    Xor,
    /// `&`.
    BitAnd,
    /// `|`.
    BitOr,
    /// `^`.
    BitXor,
    /// `<<`.
    ShiftLeft,
    /// `>>`.
    ShiftRight,
}
