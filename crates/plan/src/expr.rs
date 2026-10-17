//! Expressions of the plan, each with its type and its nullability.
//!
//! The constructors apply the rules that hold for every dialect
//! (`docs/FEDERATED.md`, section 7):
//!
//! - comparison equality (`Eq`, three-valued) and identity equality
//!   (`NotDistinct`, never unknown) are two distinct expressions (point 1);
//! - a predicate is a three-valued `Boolean`, unknown when one of its
//!   operands is NULL; `IS NULL`, identity and `EXISTS` are never unknown
//!   (point 2);
//! - `NOT IN` stays a three-valued predicate, as written, unknown when its
//!   operand or its subquery holds a NULL (point 3);
//! - the MySQL family mixes booleans and numbers: the plan converts between
//!   them explicitly (`to_boolean`, `to_integer`, point 2).
//!
//! The type of the other expressions depends on the dialect (type of a
//! literal, of an arithmetic operation, of a function, of a `CAST` that can
//! give NULL): the plan builder gives it.

use prisme_types::{DataType, IntegerType, Type, Value};

use crate::{Cast, DateArithmetic, Extract, Ident, Plan, PlanError, SortKey, Span, TimestampDiff};

/// Expression of the plan, with its type.
#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    /// What the expression computes.
    pub kind: ExprKind,
    /// Type of its result, with its nullability.
    pub ty: Type,
    /// Position in the input; `None` for an expression written by Prisme.
    pub span: Option<Span>,
}

/// What an expression computes.
#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    /// Column of the input.
    Column(ColumnRef),
    /// Literal.
    Literal(Value),
    /// User or system variable (`@x`, `@@sql_mode`).
    Variable(Variable),
    /// Comparison, three-valued.
    Compare {
        /// Comparison operator.
        op: Comparison,
        /// Left operand.
        left: Box<Expr>,
        /// Right operand.
        right: Box<Expr>,
    },
    /// Identity equality (`<=>`, `IS NOT DISTINCT FROM`): two NULLs are
    /// equal, the result is never unknown.
    NotDistinct {
        /// Left operand.
        left: Box<Expr>,
        /// Right operand.
        right: Box<Expr>,
        /// `true` for `IS DISTINCT FROM`.
        negated: bool,
        /// Form written.
        form: NotDistinctForm,
    },
    /// `AND`, `OR`, `XOR`, with the truth tables of Kleene.
    Logical {
        /// Logical operator.
        op: Logical,
        /// Left operand.
        left: Box<Expr>,
        /// Right operand.
        right: Box<Expr>,
    },
    /// `NOT`.
    Not(Box<Expr>),
    /// `IS NULL`, `IS NOT NULL`.
    IsNull {
        /// Value tested.
        operand: Box<Expr>,
        /// `true` for `IS NOT NULL`.
        negated: bool,
    },
    /// Arithmetic operation.
    Arithmetic {
        /// Arithmetic operator.
        op: Arithmetic,
        /// Left operand.
        left: Box<Expr>,
        /// Right operand.
        right: Box<Expr>,
    },
    /// Opposite of a number (`-x`).
    Negate(Box<Expr>),
    /// Bitwise operation.
    Bitwise {
        /// Bitwise operator.
        op: Bitwise,
        /// Left operand.
        left: Box<Expr>,
        /// Right operand.
        right: Box<Expr>,
    },
    /// Bitwise inversion (`~x`).
    BitNot(Box<Expr>),
    /// `BETWEEN`, bounds included.
    Between {
        /// Value tested.
        operand: Box<Expr>,
        /// Lower bound.
        low: Box<Expr>,
        /// Upper bound.
        high: Box<Expr>,
        /// `true` for `NOT BETWEEN`.
        negated: bool,
    },
    /// `LIKE`.
    Like {
        /// Value tested.
        operand: Box<Expr>,
        /// Pattern.
        pattern: Box<Expr>,
        /// Escape character (`ESCAPE`).
        escape: Option<Box<Expr>>,
        /// `true` for `NOT LIKE`.
        negated: bool,
    },
    /// `IN` a list of values.
    InList {
        /// Value tested.
        operand: Box<Expr>,
        /// Values of the list.
        list: Vec<Expr>,
        /// `true` for `NOT IN`.
        negated: bool,
    },
    /// `IN` a subquery of as many columns as the value tested (one, or the
    /// values of a row), three-valued: `NOT IN` a subquery that holds a NULL
    /// is never true.
    InSubquery {
        /// Value tested.
        operand: Box<Expr>,
        /// Subquery.
        subquery: Box<Plan>,
        /// `true` for `NOT IN`.
        negated: bool,
    },
    /// `EXISTS`, never unknown.
    Exists {
        /// Subquery.
        subquery: Box<Plan>,
        /// `true` for `NOT EXISTS`.
        negated: bool,
    },
    /// Subquery of one column and at most one row; NULL without row.
    ScalarSubquery(Box<Plan>),
    /// Comparison with the rows of a subquery of as many columns as the
    /// value compared: `a > ALL (subquery)`, `(a, b) = ANY (subquery)`.
    Quantified {
        /// Comparison.
        op: Comparison,
        /// Value compared.
        operand: Box<Expr>,
        /// `ALL` or `ANY`.
        quantifier: Quantifier,
        /// Subquery.
        subquery: Box<Plan>,
    },
    /// Row of values, compared whole (`(a, b) = (1, 2)`,
    /// `(a, b) IN (SELECT ...)`): equal when each value is.
    Row(Vec<Expr>),
    /// Call to a scalar function.
    Function(Box<FunctionCall>),
    /// Call to a window function.
    Window(Box<WindowCall>),
    /// `CASE`.
    Case(Case),
    /// `CAST` or `CONVERT` to the data type of the expression.
    Cast(Box<Cast>),
    /// Interval added to or subtracted from a date or a time
    /// (`d + INTERVAL 1 DAY`, `DATE_ADD`, `TIMESTAMPADD`).
    DateArithmetic(Box<DateArithmetic>),
    /// Whole units from a date or a time to another (`TIMESTAMPDIFF`).
    TimestampDiff(Box<TimestampDiff>),
    /// Part of a date or a time (`EXTRACT`).
    Extract(Box<Extract>),
    /// Number to boolean, true when non-zero (MySQL family: `WHERE a`,
    /// `2 AND 3`).
    ToBoolean(Box<Expr>),
    /// Boolean to integer, 1 or 0 (MySQL family: `SUM(a = 1)`,
    /// `(a = 1) + 1`).
    ToInteger(Box<Expr>),
}

/// Reference to a column of the input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnRef {
    /// Table or alias written before the name (`t.a`).
    pub qualifier: Option<Ident>,
    /// Name of the column.
    pub name: Ident,
}

/// User or system variable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Variable {
    /// Scope of the variable.
    pub scope: VariableScope,
    /// Name, without `@` or `@@`.
    pub name: Ident,
}

/// Scope of a variable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VariableScope {
    /// User variable of the connection (`@x`); its type is read on the server
    /// before running (`docs/QUESTIONS.md`, point 44).
    User,
    /// System variable of the session (`@@x`, `@@session.x`).
    Session,
    /// System variable of the server (`@@global.x`).
    Global,
}

/// Quantifier of a comparison with a subquery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Quantifier {
    /// `ALL`: true for every row, true without row.
    All,
    /// `ANY`, `SOME`: true for at least one row, false without row.
    Any,
}

/// Comparison operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Comparison {
    /// `=`, comparison equality.
    Eq,
    /// `<>`.
    NotEq,
    /// `<`.
    Lt,
    /// `<=`.
    LtEq,
    /// `>`.
    Gt,
    /// `>=`.
    GtEq,
}

/// Form written for an identity equality.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NotDistinctForm {
    /// `a <=> b` (MySQL family).
    Symbol,
    /// `a IS NOT DISTINCT FROM b` (standard SQL).
    Keywords,
}

/// Logical operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Logical {
    /// `AND`.
    And,
    /// `OR`.
    Or,
    /// `XOR` (MySQL family).
    Xor,
}

/// Arithmetic operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Arithmetic {
    /// `+`.
    Add,
    /// `-`.
    Subtract,
    /// `*`.
    Multiply,
    /// `/`.
    Divide,
    /// Integer division (`DIV` of the MySQL family).
    IntegerDivide,
    /// Remainder (`%`, `MOD`).
    Modulo,
}

/// Bitwise operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Bitwise {
    /// `&`.
    And,
    /// `|`.
    Or,
    /// `^`.
    Xor,
    /// `<<`.
    ShiftLeft,
    /// `>>`.
    ShiftRight,
}

/// Call to a scalar function.
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionCall {
    /// Database of a stored function (`db.f()`).
    pub qualifier: Option<Ident>,
    /// Name of the function, as written.
    pub name: Ident,
    /// Arguments.
    pub args: Vec<Expr>,
    /// Function stored in the database, the built-in functions of the
    /// dialect aside.
    pub stored: bool,
}

/// Call to a window function.
#[derive(Debug, Clone, PartialEq)]
pub struct WindowCall {
    /// Name of the function, as written.
    pub function: Ident,
    /// Arguments; empty for `COUNT(*)`.
    pub args: Vec<Expr>,
    /// `PARTITION BY`, that of the named window included.
    pub partition_by: Vec<Expr>,
    /// `ORDER BY` of the window, that of the named window included.
    pub order_by: Vec<SortKey>,
    /// Frame of the window, when written.
    pub frame: Option<Frame>,
    /// Named window the call refers to (`OVER w`, `OVER (w ORDER BY a)`),
    /// whose definition the window node holds.
    pub window: Option<Ident>,
}

/// Frame of a window.
#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    /// Unit of the bounds.
    pub units: FrameUnits,
    /// Start of the frame.
    pub start: FrameBound,
    /// End of the frame; `None` for `ROWS start` alone, ending at the
    /// current row.
    pub end: Option<FrameBound>,
}

/// Unit of the bounds of a frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FrameUnits {
    /// `ROWS`.
    Rows,
    /// `RANGE`.
    Range,
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

/// `CASE` expression.
#[derive(Debug, Clone, PartialEq)]
pub struct Case {
    /// Value compared to each branch (`CASE x WHEN 1 ...`); `None` when each
    /// branch is a predicate (`CASE WHEN x = 1 ...`).
    pub operand: Option<Box<Expr>>,
    /// Branches, in order; a branch is taken only when true.
    pub branches: Vec<CaseBranch>,
    /// `ELSE`; NULL when absent.
    pub otherwise: Option<Box<Expr>>,
}

/// Branch of a `CASE`.
#[derive(Debug, Clone, PartialEq)]
pub struct CaseBranch {
    /// Condition, or value compared to the operand.
    pub when: Expr,
    /// Result of the branch.
    pub then: Expr,
}

impl Expr {
    /// Reference to a column, of the type found in the input.
    pub fn column(column: ColumnRef, ty: Type) -> Self {
        Self::new(ExprKind::Column(column), ty)
    }

    /// Literal of the type the dialect gives it; nullable only for NULL.
    pub fn literal(value: Value, data_type: DataType) -> Self {
        let nullable = value == Value::Null;
        Self::new(
            ExprKind::Literal(value),
            Type {
                data_type,
                nullable,
            },
        )
    }

    /// Literal of the type `ty`, nullable for another value than NULL when
    /// the server types it so (a zero date, `date NULL` in MariaDB).
    pub fn literal_typed(value: Value, ty: Type) -> Self {
        Self::new(ExprKind::Literal(value), ty)
    }

    /// Variable, of the type read on the server.
    pub fn variable(variable: Variable, ty: Type) -> Self {
        Self::new(ExprKind::Variable(variable), ty)
    }

    /// Comparison: unknown when an operand is NULL.
    pub fn compare(op: Comparison, left: Expr, right: Expr) -> Self {
        let ty = boolean(left.ty.nullable || right.ty.nullable);
        Self::new(
            ExprKind::Compare {
                op,
                left: Box::new(left),
                right: Box::new(right),
            },
            ty,
        )
    }

    /// Identity equality: never unknown.
    pub fn not_distinct(left: Expr, right: Expr, negated: bool, form: NotDistinctForm) -> Self {
        Self::new(
            ExprKind::NotDistinct {
                left: Box::new(left),
                right: Box::new(right),
                negated,
                form,
            },
            boolean(false),
        )
    }

    /// `AND`, `OR`, `XOR`: can be unknown when an operand can.
    pub fn logical(op: Logical, left: Expr, right: Expr) -> Self {
        let ty = boolean(left.ty.nullable || right.ty.nullable);
        Self::new(
            ExprKind::Logical {
                op,
                left: Box::new(left),
                right: Box::new(right),
            },
            ty,
        )
    }

    /// `NOT`: unknown when its operand is.
    pub fn logical_not(operand: Expr) -> Self {
        let ty = boolean(operand.ty.nullable);
        Self::new(ExprKind::Not(Box::new(operand)), ty)
    }

    /// `IS NULL`, `IS NOT NULL`: never unknown.
    pub fn is_null(operand: Expr, negated: bool) -> Self {
        Self::new(
            ExprKind::IsNull {
                operand: Box::new(operand),
                negated,
            },
            boolean(false),
        )
    }

    /// Arithmetic operation, of the type the dialect gives it (a division by
    /// zero gives NULL in the MySQL family).
    pub fn arithmetic(op: Arithmetic, left: Expr, right: Expr, ty: Type) -> Self {
        Self::new(
            ExprKind::Arithmetic {
                op,
                left: Box::new(left),
                right: Box::new(right),
            },
            ty,
        )
    }

    /// Opposite of a number, of the type the dialect gives it.
    pub fn negate(operand: Expr, ty: Type) -> Self {
        Self::new(ExprKind::Negate(Box::new(operand)), ty)
    }

    /// Bitwise operation, of the type the dialect gives it.
    pub fn bitwise(op: Bitwise, left: Expr, right: Expr, ty: Type) -> Self {
        Self::new(
            ExprKind::Bitwise {
                op,
                left: Box::new(left),
                right: Box::new(right),
            },
            ty,
        )
    }

    /// Bitwise inversion, of the type the dialect gives it.
    pub fn bit_not(operand: Expr, ty: Type) -> Self {
        Self::new(ExprKind::BitNot(Box::new(operand)), ty)
    }

    /// `BETWEEN`: unknown when the value or a bound is NULL.
    pub fn between(operand: Expr, low: Expr, high: Expr, negated: bool) -> Self {
        let ty = boolean(operand.ty.nullable || low.ty.nullable || high.ty.nullable);
        Self::new(
            ExprKind::Between {
                operand: Box::new(operand),
                low: Box::new(low),
                high: Box::new(high),
                negated,
            },
            ty,
        )
    }

    /// `LIKE`: unknown when the value, the pattern or the escape is NULL.
    pub fn like(operand: Expr, pattern: Expr, escape: Option<Expr>, negated: bool) -> Self {
        let nullable = operand.ty.nullable
            || pattern.ty.nullable
            || escape.as_ref().is_some_and(|escape| escape.ty.nullable);
        Self::new(
            ExprKind::Like {
                operand: Box::new(operand),
                pattern: Box::new(pattern),
                escape: escape.map(Box::new),
                negated,
            },
            boolean(nullable),
        )
    }

    /// `IN` a list: unknown when the value or a value of the list is NULL.
    pub fn in_list(operand: Expr, list: Vec<Expr>, negated: bool) -> Self {
        let nullable = operand.ty.nullable || list.iter().any(|value| value.ty.nullable);
        Self::new(
            ExprKind::InList {
                operand: Box::new(operand),
                list,
                negated,
            },
            boolean(nullable),
        )
    }

    /// `IN` a subquery: unknown when the value or a column of the subquery
    /// can be NULL.
    ///
    /// # Errors
    ///
    /// `PlanError::ColumnCount` when the subquery has not as many columns as
    /// the value tested (one, or the values of a row).
    pub fn in_subquery(operand: Expr, subquery: Plan, negated: bool) -> Result<Self, PlanError> {
        let expected = match &operand.kind {
            ExprKind::Row(values) => values.len(),
            _ => 1,
        };
        let schema = subquery.schema();
        if schema.len() != expected {
            return Err(PlanError::ColumnCount {
                expected,
                found: schema.len(),
            });
        }
        let nullable = schema.columns.iter().any(|column| column.ty.nullable);
        let ty = boolean(operand.ty.nullable || nullable);
        Ok(Self::new(
            ExprKind::InSubquery {
                operand: Box::new(operand),
                subquery: Box::new(subquery),
                negated,
            },
            ty,
        ))
    }

    /// Comparison with the rows of a subquery: unknown when the value or a
    /// column of the subquery can be NULL; never unknown without row.
    ///
    /// # Errors
    ///
    /// `PlanError::ColumnCount` when the subquery has not as many columns as
    /// the value compared (one, or the values of a row).
    pub fn quantified(
        op: Comparison,
        operand: Expr,
        quantifier: Quantifier,
        subquery: Plan,
    ) -> Result<Self, PlanError> {
        let expected = match &operand.kind {
            ExprKind::Row(values) => values.len(),
            _ => 1,
        };
        let schema = subquery.schema();
        if schema.len() != expected {
            return Err(PlanError::ColumnCount {
                expected,
                found: schema.len(),
            });
        }
        let nullable = schema.columns.iter().any(|column| column.ty.nullable);
        let ty = boolean(operand.ty.nullable || nullable);
        Ok(Self::new(
            ExprKind::Quantified {
                op,
                operand: Box::new(operand),
                quantifier,
                subquery: Box::new(subquery),
            },
            ty,
        ))
    }

    /// Row of values: of no data type of its own, NULL in part when one of
    /// its values can be.
    pub fn row(values: Vec<Expr>) -> Self {
        let nullable = values.iter().any(|value| value.ty.nullable);
        Self::new(
            ExprKind::Row(values),
            Type {
                data_type: DataType::Unknown,
                nullable,
            },
        )
    }

    /// `EXISTS`: never unknown.
    pub fn exists(subquery: Plan, negated: bool) -> Self {
        Self::new(
            ExprKind::Exists {
                subquery: Box::new(subquery),
                negated,
            },
            boolean(false),
        )
    }

    /// Scalar subquery: the type of its column, always nullable, as a
    /// subquery without row gives NULL.
    ///
    /// # Errors
    ///
    /// `PlanError::ColumnCount` when the subquery has more than one column.
    pub fn scalar_subquery(subquery: Plan) -> Result<Self, PlanError> {
        let ty = single_column(&subquery)?.with_nullable(true);
        Ok(Self::new(ExprKind::ScalarSubquery(Box::new(subquery)), ty))
    }

    /// Call to a scalar function, of the type the function catalog gives it.
    pub fn function(call: FunctionCall, ty: Type) -> Self {
        Self::new(ExprKind::Function(Box::new(call)), ty)
    }

    /// Call to a window function, of the type the function catalog gives it.
    pub fn window(call: WindowCall, ty: Type) -> Self {
        Self::new(ExprKind::Window(Box::new(call)), ty)
    }

    /// `CASE`, of the type the dialect gives it.
    pub fn case(case: Case, ty: Type) -> Self {
        Self::new(ExprKind::Case(case), ty)
    }

    /// `CAST` or `CONVERT` to the data type of `ty`, whose nullability the
    /// dialect gives (MySQL gives NULL for `CAST('0000-00-00' AS DATE)`).
    pub fn cast(cast: Cast, ty: Type) -> Self {
        Self::new(ExprKind::Cast(Box::new(cast)), ty)
    }

    /// Interval added to or subtracted from a date or a time, of the type the
    /// dialect gives it.
    pub fn date_arithmetic(arithmetic: DateArithmetic, ty: Type) -> Self {
        Self::new(ExprKind::DateArithmetic(Box::new(arithmetic)), ty)
    }

    /// Whole units from a date or a time to another, of the type the dialect
    /// gives it.
    pub fn timestamp_diff(diff: TimestampDiff, ty: Type) -> Self {
        Self::new(ExprKind::TimestampDiff(Box::new(diff)), ty)
    }

    /// Part of a date or a time, of the type the dialect gives it.
    pub fn extract(extract: Extract, ty: Type) -> Self {
        Self::new(ExprKind::Extract(Box::new(extract)), ty)
    }

    /// Number to boolean, true when non-zero; unknown when the number is NULL.
    pub fn to_boolean(operand: Expr) -> Self {
        let ty = boolean(operand.ty.nullable);
        Self::new(ExprKind::ToBoolean(Box::new(operand)), ty)
    }

    /// Boolean to integer of the type the dialect gives (`BIGINT` on MySQL,
    /// `INT` on MariaDB); NULL when the boolean is unknown.
    pub fn to_integer(operand: Expr, integer: IntegerType) -> Self {
        let ty = Type {
            data_type: DataType::Integer(integer),
            nullable: operand.ty.nullable,
        };
        Self::new(ExprKind::ToInteger(Box::new(operand)), ty)
    }

    /// Operands of the expression, to rewrite them: the values it is
    /// computed from, the subqueries left out.
    pub fn operands_mut(&mut self) -> Vec<&mut Expr> {
        match &mut self.kind {
            ExprKind::Column(_)
            | ExprKind::Literal(_)
            | ExprKind::Variable(_)
            | ExprKind::Exists { .. }
            | ExprKind::ScalarSubquery(_) => Vec::new(),
            ExprKind::Compare { left, right, .. }
            | ExprKind::NotDistinct { left, right, .. }
            | ExprKind::Logical { left, right, .. }
            | ExprKind::Arithmetic { left, right, .. }
            | ExprKind::Bitwise { left, right, .. } => vec![left.as_mut(), right.as_mut()],
            ExprKind::Not(operand)
            | ExprKind::Negate(operand)
            | ExprKind::BitNot(operand)
            | ExprKind::ToBoolean(operand)
            | ExprKind::ToInteger(operand)
            | ExprKind::IsNull { operand, .. }
            | ExprKind::InSubquery { operand, .. }
            | ExprKind::Quantified { operand, .. } => vec![operand.as_mut()],
            ExprKind::Cast(cast) => vec![&mut cast.operand],
            ExprKind::DateArithmetic(arithmetic) => {
                vec![&mut arithmetic.operand, &mut arithmetic.value]
            }
            ExprKind::TimestampDiff(diff) => vec![&mut diff.start, &mut diff.end],
            ExprKind::Extract(extract) => vec![&mut extract.operand],
            ExprKind::Between {
                operand, low, high, ..
            } => vec![operand.as_mut(), low.as_mut(), high.as_mut()],
            ExprKind::Like {
                operand,
                pattern,
                escape,
                ..
            } => {
                let mut operands = vec![operand.as_mut(), pattern.as_mut()];
                operands.extend(escape.as_deref_mut());
                operands
            }
            ExprKind::InList { operand, list, .. } => {
                let mut operands = vec![operand.as_mut()];
                operands.extend(list.iter_mut());
                operands
            }
            ExprKind::Row(values) => values.iter_mut().collect(),
            ExprKind::Function(call) => call.args.iter_mut().collect(),
            ExprKind::Window(call) => {
                let WindowCall {
                    args,
                    partition_by,
                    order_by,
                    frame,
                    ..
                } = call.as_mut();
                let mut operands: Vec<&mut Expr> = args.iter_mut().collect();
                operands.extend(partition_by.iter_mut());
                operands.extend(order_by.iter_mut().map(|key| &mut key.expr));
                if let Some(frame) = frame {
                    operands.extend(frame.bounds_mut());
                }
                operands
            }
            ExprKind::Case(case) => {
                let mut operands: Vec<&mut Expr> =
                    case.operand.as_deref_mut().into_iter().collect();
                for branch in &mut case.branches {
                    operands.push(&mut branch.when);
                    operands.push(&mut branch.then);
                }
                operands.extend(case.otherwise.as_deref_mut());
                operands
            }
        }
    }

    /// Same expression, without the positions read in the input: two
    /// expressions written at different places compare equal.
    pub fn without_spans(mut self) -> Self {
        self.clear_spans();
        self
    }

    /// Removes the positions of the expression and of its operands.
    fn clear_spans(&mut self) {
        self.span = None;
        for operand in self.operands_mut() {
            operand.clear_spans();
        }
    }

    /// Same expression, read at `span` in the input.
    pub fn with_span(self, span: Span) -> Self {
        Self {
            span: Some(span),
            ..self
        }
    }

    /// Expression written by Prisme, without position.
    fn new(kind: ExprKind, ty: Type) -> Self {
        Self {
            kind,
            ty,
            span: None,
        }
    }
}

impl Frame {
    /// Values of the bounds, to rewrite them.
    fn bounds_mut(&mut self) -> Vec<&mut Expr> {
        let mut bounds = Vec::new();
        for bound in std::iter::once(&mut self.start).chain(self.end.as_mut()) {
            if let FrameBound::Preceding(value) | FrameBound::Following(value) = bound {
                bounds.push(value.as_mut());
            }
        }
        bounds
    }
}

/// Boolean type, nullable or not.
fn boolean(nullable: bool) -> Type {
    Type {
        data_type: DataType::Boolean,
        nullable,
    }
}

/// Type of the single column of a subquery.
fn single_column(subquery: &Plan) -> Result<Type, PlanError> {
    let schema = subquery.schema();
    match schema.columns.as_slice() {
        [column] => Ok(column.ty.clone()),
        columns => Err(PlanError::ColumnCount {
            expected: 1,
            found: columns.len(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use prisme_types::{IntegerSize, IntegerType};

    use super::*;
    use crate::{Values, ValuesForm};

    fn int() -> DataType {
        DataType::Integer(IntegerType::signed(IntegerSize::Int))
    }

    fn column(name: &str, nullable: bool) -> Expr {
        let column = ColumnRef {
            qualifier: None,
            name: Ident::new(name),
        };
        Expr::column(column, Type::nullable(int()).with_nullable(nullable))
    }

    /// Subquery `VALUES (1), (NULL)` or `VALUES (1), (2)`, one column.
    fn subquery(with_null: bool) -> Plan {
        let second = if with_null {
            Expr::literal(Value::Null, DataType::Null)
        } else {
            Expr::literal(Value::Integer(2), int())
        };
        let rows = vec![vec![Expr::literal(Value::Integer(1), int())], vec![second]];
        Plan::Values(
            Values::new(rows, vec![Ident::new("k")], vec![int()], ValuesForm::Bare).unwrap(),
        )
    }

    #[test]
    fn comparison_is_unknown_with_a_null_operand() {
        let eq = Expr::compare(Comparison::Eq, column("a", true), column("b", false));
        assert_eq!(eq.ty, Type::nullable(DataType::Boolean));
        let eq = Expr::compare(Comparison::Eq, column("a", false), column("b", false));
        assert_eq!(eq.ty, Type::not_null(DataType::Boolean));
    }

    #[test]
    fn identity_and_is_null_are_never_unknown() {
        let identity = Expr::not_distinct(
            column("a", true),
            column("b", true),
            false,
            NotDistinctForm::Symbol,
        );
        assert_eq!(identity.ty, Type::not_null(DataType::Boolean));
        assert!(!Expr::is_null(column("a", true), false).ty.nullable);
        assert!(!Expr::exists(subquery(true), true).ty.nullable);
    }

    #[test]
    fn logic_is_unknown_when_an_operand_can_be() {
        let known = Expr::compare(Comparison::Gt, column("a", false), column("b", false));
        let unknown = Expr::compare(Comparison::Gt, column("a", true), column("b", false));
        assert!(
            !Expr::logical(Logical::And, known.clone(), known.clone())
                .ty
                .nullable
        );
        assert!(
            Expr::logical(Logical::Or, known, unknown.clone())
                .ty
                .nullable
        );
        assert!(Expr::logical_not(unknown).ty.nullable);
    }

    #[test]
    fn not_in_is_unknown_when_the_subquery_holds_a_null() {
        // Measured: `2 NOT IN (SELECT k FROM r)` is NULL when `r` holds 1 and NULL.
        let not_in = Expr::in_subquery(column("k", false), subquery(true), true).unwrap();
        assert_eq!(not_in.ty, Type::nullable(DataType::Boolean));
        let not_in = Expr::in_subquery(column("k", false), subquery(false), true).unwrap();
        assert_eq!(not_in.ty, Type::not_null(DataType::Boolean));
        let not_in = Expr::in_subquery(column("k", true), subquery(false), true).unwrap();
        assert!(not_in.ty.nullable);
    }

    #[test]
    fn in_list_is_unknown_with_a_null_in_the_list() {
        let list = vec![
            Expr::literal(Value::Integer(1), int()),
            Expr::literal(Value::Null, DataType::Null),
        ];
        assert!(Expr::in_list(column("a", false), list, true).ty.nullable);
    }

    #[test]
    fn scalar_subquery_is_nullable_and_has_one_column() {
        let scalar = Expr::scalar_subquery(subquery(false)).unwrap();
        assert_eq!(scalar.ty, Type::nullable(int()));
        let two_columns = Plan::Values(
            Values::new(
                vec![vec![
                    Expr::literal(Value::Integer(1), int()),
                    Expr::literal(Value::Integer(2), int()),
                ]],
                vec![Ident::new("a"), Ident::new("b")],
                vec![int(), int()],
                ValuesForm::Bare,
            )
            .unwrap(),
        );
        assert_eq!(
            Expr::scalar_subquery(two_columns.clone()),
            Err(PlanError::ColumnCount {
                expected: 1,
                found: 2
            })
        );
        assert!(Expr::in_subquery(column("a", false), two_columns.clone(), false).is_err());
        // A row tests the columns of the subquery one by one.
        let row = Expr::row(vec![column("a", false), column("b", true)]);
        assert!(row.ty.nullable);
        let in_row = Expr::in_subquery(row, two_columns, false).unwrap();
        assert_eq!(in_row.ty, Type::nullable(DataType::Boolean));
    }

    #[test]
    fn booleans_and_numbers_convert_explicitly() {
        // `SUM(a = 1)` in MySQL: the predicate becomes an integer.
        let predicate = Expr::compare(
            Comparison::Eq,
            column("a", true),
            Expr::literal(Value::Integer(1), int()),
        );
        let integer = Expr::to_integer(predicate, IntegerType::signed(IntegerSize::Big));
        assert_eq!(
            integer.ty,
            Type::nullable(DataType::Integer(IntegerType::signed(IntegerSize::Big)))
        );
        // `WHERE a` in MySQL: the number becomes a predicate.
        assert_eq!(
            Expr::to_boolean(column("a", false)).ty,
            Type::not_null(DataType::Boolean)
        );
    }

    #[test]
    fn literal_is_nullable_only_for_null() {
        assert!(!Expr::literal(Value::Integer(1), int()).ty.nullable);
        assert!(Expr::literal(Value::Null, DataType::Null).ty.nullable);
    }
}
