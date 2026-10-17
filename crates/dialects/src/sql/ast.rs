//! Syntax tree of the SQL dialects, as written: each node keeps its position
//! and, when several forms have the same meaning, the form written.
//!
//! The tree is common to MySQL and MariaDB; the nodes that only one dialect
//! has are refused by the parser of the other one. It stays close to the
//! text, and the plan builder gives it its meaning (names resolved, types,
//! explicit conversions).

use prisme_plan::{
    AliasForm, CastForm, CastTarget, DateArithmeticForm, Direction, FrameUnits, Ident, IndexHint,
    JoinForm, LimitForm, Span, TimeUnit, ValuesForm,
};

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
    /// `TABLE t`: the rows of a table.
    Table(ObjectName),
    /// Query between parentheses.
    Nested(Box<Query>),
    /// `UNION`, `EXCEPT`, `INTERSECT`.
    SetOperation(Box<SetOperation>),
}

/// Set operation of two query bodies.
#[derive(Debug, Clone, PartialEq)]
pub struct SetOperation {
    /// Operator.
    pub operator: SetOperator,
    /// `ALL`: duplicates kept.
    pub all: bool,
    /// Left operand, which names the columns.
    pub left: QueryBody,
    /// Right operand.
    pub right: QueryBody,
    /// Position in the input.
    pub span: Span,
}

/// Set operator, without its duplicate mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SetOperator {
    /// `UNION`.
    Union,
    /// `EXCEPT`.
    Except,
    /// `INTERSECT`.
    Intersect,
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
    /// `GROUP BY`.
    pub group_by: Vec<Expr>,
    /// `WITH ROLLUP` after the `GROUP BY`.
    pub rollup: bool,
    /// `HAVING`.
    pub having: Option<Expr>,
    /// Named windows of the `WINDOW` clause.
    pub windows: Vec<NamedWindow>,
    /// Position in the input.
    pub span: Span,
}

/// Window of the `WINDOW` clause: `w AS (PARTITION BY a)`.
#[derive(Debug, Clone, PartialEq)]
pub struct NamedWindow {
    /// Name.
    pub name: Ident,
    /// Definition.
    pub spec: WindowSpec,
}

/// Window of a call (`OVER (...)`, `OVER w`) or of the `WINDOW` clause.
#[derive(Debug, Clone, PartialEq)]
pub struct WindowSpec {
    /// Named window this one completes, or uses alone (`OVER w`).
    pub base: Option<Ident>,
    /// `PARTITION BY`.
    pub partition_by: Vec<Expr>,
    /// `ORDER BY`.
    pub order_by: Vec<OrderItem>,
    /// Frame.
    pub frame: Option<Frame>,
    /// Position in the input.
    pub span: Span,
}

/// Frame of a window: `ROWS BETWEEN 1 PRECEDING AND CURRENT ROW`.
#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    /// `ROWS` or `RANGE`.
    pub units: FrameUnits,
    /// Start.
    pub start: FrameBound,
    /// End, after `BETWEEN ... AND`.
    pub end: Option<FrameBound>,
}

/// Bound of a frame.
#[derive(Debug, Clone, PartialEq)]
pub enum FrameBound {
    /// `UNBOUNDED PRECEDING`.
    UnboundedPreceding,
    /// `n PRECEDING`.
    Preceding(Box<Expr>),
    /// `CURRENT ROW`.
    CurrentRow,
    /// `n FOLLOWING`.
    Following(Box<Expr>),
    /// `UNBOUNDED FOLLOWING`.
    UnboundedFollowing,
}

/// Aggregate functions of the MySQL family, whose name must touch `(`.
pub const AGGREGATES: [&str; 18] = [
    "AVG",
    "BIT_AND",
    "BIT_OR",
    "BIT_XOR",
    "COUNT",
    "GROUP_CONCAT",
    "JSON_ARRAYAGG",
    "JSON_OBJECTAGG",
    "MAX",
    "MIN",
    "STD",
    "STDDEV",
    "STDDEV_POP",
    "STDDEV_SAMP",
    "SUM",
    "VARIANCE",
    "VAR_POP",
    "VAR_SAMP",
];

/// Call to a function: scalar, aggregate or window.
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionCall {
    /// Name, qualified by its database for a stored function.
    pub name: ObjectName,
    /// Arguments; empty for `COUNT(*)`.
    pub args: Vec<Expr>,
    /// `COUNT(*)`.
    pub star: bool,
    /// `DISTINCT` before the arguments of an aggregate function.
    pub distinct: bool,
    /// `ORDER BY` of `GROUP_CONCAT`.
    pub order_by: Vec<OrderItem>,
    /// `SEPARATOR` of `GROUP_CONCAT`.
    pub separator: Option<String>,
    /// Call to a function stored in the database: a qualified name, a name
    /// the dialect does not know in its version, or the name of a built-in
    /// function written in a form that calls a stored one (`SUM (a)`,
    /// `` `LEFT`(a) ``).
    pub stored: bool,
    /// Window of a window function or of an aggregate used as one.
    pub over: Option<WindowSpec>,
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
        /// Partitions (`PARTITION (p0)`).
        partitions: Vec<Ident>,
        /// Alias.
        alias: Option<Alias>,
        /// Index hints.
        index_hints: Vec<IndexHint>,
        /// Position in the input.
        span: Span,
    },
    /// Subquery, with its alias.
    Derived {
        /// Subquery.
        query: Box<Query>,
        /// `LATERAL`.
        lateral: bool,
        /// Alias, required.
        alias: Alias,
        /// Column list after the alias; empty when not written.
        columns: Vec<Ident>,
        /// Position in the input.
        span: Span,
    },
    /// Join of two sources.
    Join(Box<Join>),
    /// Sources between parentheses.
    Nested {
        /// Sources.
        inner: Box<TableRef>,
        /// Position in the input, parentheses included.
        span: Span,
    },
}

/// Join of two sources.
#[derive(Debug, Clone, PartialEq)]
pub struct Join {
    /// Left source.
    pub left: TableRef,
    /// Right source.
    pub right: TableRef,
    /// Inner, left or right join.
    pub kind: JoinKind,
    /// Keywords written.
    pub form: JoinForm,
    /// `NATURAL`.
    pub natural: bool,
    /// `ON` or `USING`.
    pub constraint: Option<JoinConstraint>,
    /// Position in the input.
    pub span: Span,
}

/// Kind of a join as written: a comma or an inner join without condition
/// is a cross join.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JoinKind {
    /// Inner join (`JOIN`, `INNER JOIN`, `CROSS JOIN`, `STRAIGHT_JOIN`, `,`).
    Inner,
    /// `LEFT [OUTER] JOIN`.
    Left,
    /// `RIGHT [OUTER] JOIN`.
    Right,
}

/// Condition of a join.
#[derive(Debug, Clone, PartialEq)]
pub enum JoinConstraint {
    /// `ON predicate`.
    On(Expr),
    /// `USING (columns)`.
    Using(Vec<Ident>),
}

impl TableRef {
    /// Position in the input.
    pub fn span(&self) -> Span {
        match self {
            Self::Named { span, .. } | Self::Derived { span, .. } | Self::Nested { span, .. } => {
                *span
            }
            Self::Join(join) => join.span,
        }
    }
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
    /// Subquery giving one value.
    Subquery(Box<Query>),
    /// `EXISTS (subquery)`.
    Exists(Box<Query>),
    /// `[NOT] IN (subquery)`.
    InSubquery {
        /// Value tested.
        operand: Box<Expr>,
        /// Subquery.
        query: Box<Query>,
        /// `NOT IN`.
        negated: bool,
    },
    /// Comparison with the rows of a subquery: `a > ALL (subquery)`.
    Quantified {
        /// Comparison operator.
        op: BinaryOp,
        /// Value compared.
        operand: Box<Expr>,
        /// `ALL` (`true`), or `ANY` and `SOME`.
        all: bool,
        /// Subquery.
        query: Box<Query>,
    },
    /// Row of values: `(a, b)`, `ROW(a, b)`.
    Row(Vec<Expr>),
    /// Call to a function.
    Function(Box<FunctionCall>),
    /// `CASE [operand] WHEN ... THEN ... [ELSE ...] END`.
    Case {
        /// Value compared to each `WHEN` (`CASE x WHEN 1 ...`); `None` when
        /// each `WHEN` is a predicate.
        operand: Option<Box<Expr>>,
        /// `WHEN` and `THEN` of each branch, in order.
        branches: Vec<(Expr, Expr)>,
        /// `ELSE`.
        otherwise: Option<Box<Expr>>,
    },
    /// `CAST(x AS type)`, `CONVERT(x, type)`, `CONVERT(x USING charset)`.
    Cast(Box<CastCall>),
    /// Interval added to or subtracted from a date or a time:
    /// `d + INTERVAL 1 DAY`, `DATE_ADD(d, INTERVAL 1 DAY)`,
    /// `TIMESTAMPADD(DAY, 1, d)`...
    DateArithmetic(Box<DateArithmeticCall>),
    /// `TIMESTAMPDIFF(unit, start, end)`.
    TimestampDiff(Box<TimestampDiffCall>),
}

/// `CAST` or `CONVERT`.
#[derive(Debug, Clone, PartialEq)]
pub struct CastCall {
    /// Value converted.
    pub operand: Expr,
    /// Type, as written.
    pub target: CastTarget,
    /// Form written.
    pub form: CastForm,
}

/// Interval added to or subtracted from a date or a time.
#[derive(Debug, Clone, PartialEq)]
pub struct DateArithmeticCall {
    /// Date or time the interval moves.
    pub operand: Expr,
    /// Number of units, or the string of a compound unit.
    pub value: Expr,
    /// Unit of the interval.
    pub unit: TimeUnit,
    /// Form written.
    pub form: DateArithmeticForm,
}

/// `TIMESTAMPDIFF(unit, start, end)`.
#[derive(Debug, Clone, PartialEq)]
pub struct TimestampDiffCall {
    /// Unit counted, a simple one.
    pub unit: TimeUnit,
    /// Date or time the count starts from.
    pub start: Expr,
    /// Date or time the count ends at.
    pub end: Expr,
}

impl FunctionCall {
    /// Name of the function, in capitals.
    pub fn upper_name(&self) -> String {
        self.name.parts[self.name.parts.len() - 1]
            .value
            .to_ascii_uppercase()
    }

    /// Tells whether the call is to a built-in aggregate function.
    pub fn is_aggregate(&self) -> bool {
        !self.stored
            && self.name.parts.len() == 1
            && AGGREGATES.contains(&self.upper_name().as_str())
    }
}

impl Expr {
    /// Operands of the expression, the subqueries left out (their queries
    /// have their own scope): the values it is computed from, and for a call
    /// the values of its window.
    pub fn children(&self) -> Vec<&Self> {
        match &self.kind {
            ExprKind::Column(_)
            | ExprKind::Literal(_)
            | ExprKind::Subquery(_)
            | ExprKind::Exists(_) => Vec::new(),
            ExprKind::Unary { operand, .. }
            | ExprKind::IsNull { operand, .. }
            | ExprKind::Nested(operand)
            | ExprKind::InSubquery { operand, .. }
            | ExprKind::Quantified { operand, .. } => vec![operand],
            ExprKind::Cast(cast) => vec![&cast.operand],
            ExprKind::DateArithmetic(call) => vec![&call.operand, &call.value],
            ExprKind::TimestampDiff(call) => vec![&call.start, &call.end],
            ExprKind::Binary { left, right, .. } => vec![left, right],
            ExprKind::Between {
                operand, low, high, ..
            } => vec![operand, low, high],
            ExprKind::InList { operand, list, .. } => {
                std::iter::once(operand.as_ref()).chain(list).collect()
            }
            ExprKind::Like {
                operand,
                pattern,
                escape,
                ..
            } => {
                let mut children = vec![operand.as_ref(), pattern.as_ref()];
                children.extend(escape.as_deref());
                children
            }
            ExprKind::Row(values) => values.iter().collect(),
            ExprKind::Case {
                operand,
                branches,
                otherwise,
            } => {
                let mut children: Vec<&Self> = operand.iter().map(AsRef::as_ref).collect();
                for (when, then) in branches {
                    children.extend([when, then]);
                }
                children.extend(otherwise.as_deref());
                children
            }
            ExprKind::Function(call) => {
                let mut children: Vec<&Self> = call.args.iter().collect();
                children.extend(call.order_by.iter().map(|item| &item.expr));
                if let Some(over) = &call.over {
                    children.extend(over.children());
                }
                children
            }
        }
    }
}

impl WindowSpec {
    /// Values of the window: partition, order and frame bounds.
    pub fn children(&self) -> Vec<&Expr> {
        let mut children: Vec<&Expr> = self.partition_by.iter().collect();
        children.extend(self.order_by.iter().map(|item| &item.expr));
        if let Some(frame) = &self.frame {
            for bound in std::iter::once(&frame.start).chain(frame.end.as_ref()) {
                if let FrameBound::Preceding(value) | FrameBound::Following(value) = bound {
                    children.push(value);
                }
            }
        }
        children
    }
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
    /// `DATE '...'`, `TIME '...'`, `TIMESTAMP '...'`, or their ODBC form
    /// (`{d '...'}`) when the dialect reads a date or a time in it.
    Temporal {
        /// Type written.
        kind: TemporalKind,
        /// String, as written.
        value: String,
    },
}

/// Type of a typed literal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TemporalKind {
    /// `DATE`.
    Date,
    /// `TIME`.
    Time,
    /// `TIMESTAMP`, a `DATETIME`.
    Timestamp,
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
