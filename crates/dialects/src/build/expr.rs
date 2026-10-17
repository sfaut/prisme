//! Expressions of the MySQL family, typed, with the explicit conversions
//! between booleans and numbers (`docs/FEDERATED.md`, section 7, point 2):
//! a predicate used as a value becomes an integer (`ToInteger`), a value
//! used as a predicate becomes a boolean (`ToBoolean`).
//!
//! A row (`(a, b)`, `ROW(a, b)`) is only an operand of a comparison or of
//! `IN`, compared with a row of the same shape; elsewhere, a single value is
//! expected.

use prisme_plan::{
    Arithmetic, Bitwise, Comparison, Expr, Logical, NotDistinctForm, Quantifier, Span,
};
use prisme_types::{DataType, Decimal, IntegerSize, IntegerType, Type, Value};

use super::scope::Scope;
use super::{BuildError, Builder, as_predicate, plan_error, types};
use crate::sql::ast::{self, BinaryOp, ExprKind, Literal, UnaryOp};

/// Operator of the plan for a binary operator of the tree.
enum Operator {
    /// Arithmetic.
    Arithmetic(Arithmetic),
    /// Comparison.
    Comparison(Comparison),
    /// `<=>`.
    NotDistinct,
    /// `AND`, `OR`, `XOR`.
    Logical(Logical),
    /// Bitwise.
    Bitwise(Bitwise),
}

/// Operator of the plan of `op`.
fn operator(op: BinaryOp) -> Operator {
    match op {
        BinaryOp::Add => Operator::Arithmetic(Arithmetic::Add),
        BinaryOp::Subtract => Operator::Arithmetic(Arithmetic::Subtract),
        BinaryOp::Multiply => Operator::Arithmetic(Arithmetic::Multiply),
        BinaryOp::Divide => Operator::Arithmetic(Arithmetic::Divide),
        BinaryOp::IntegerDivide => Operator::Arithmetic(Arithmetic::IntegerDivide),
        BinaryOp::Modulo => Operator::Arithmetic(Arithmetic::Modulo),
        BinaryOp::Eq => Operator::Comparison(Comparison::Eq),
        BinaryOp::NotEq => Operator::Comparison(Comparison::NotEq),
        BinaryOp::Lt => Operator::Comparison(Comparison::Lt),
        BinaryOp::LtEq => Operator::Comparison(Comparison::LtEq),
        BinaryOp::Gt => Operator::Comparison(Comparison::Gt),
        BinaryOp::GtEq => Operator::Comparison(Comparison::GtEq),
        BinaryOp::NullSafeEq => Operator::NotDistinct,
        BinaryOp::And => Operator::Logical(Logical::And),
        BinaryOp::Or => Operator::Logical(Logical::Or),
        BinaryOp::Xor => Operator::Logical(Logical::Xor),
        BinaryOp::BitAnd => Operator::Bitwise(Bitwise::And),
        BinaryOp::BitOr => Operator::Bitwise(Bitwise::Or),
        BinaryOp::BitXor => Operator::Bitwise(Bitwise::Xor),
        BinaryOp::ShiftLeft => Operator::Bitwise(Bitwise::ShiftLeft),
        BinaryOp::ShiftRight => Operator::Bitwise(Bitwise::ShiftRight),
    }
}

/// `BIGINT UNSIGNED`, the type of the bitwise operations of the MySQL
/// family.
fn bigint_unsigned(nullable: bool) -> Type {
    Type {
        data_type: DataType::Integer(IntegerType::unsigned(IntegerSize::Big)),
        nullable,
    }
}

/// Number of values of an expression: those of a row, one otherwise.
fn arity(expr: &Expr) -> usize {
    match &expr.kind {
        prisme_plan::ExprKind::Row(values) => values.len(),
        _ => 1,
    }
}

/// Tells whether two operands have the same shape: two rows of the same
/// shapes, or two single values.
fn same_shape(left: &Expr, right: &Expr) -> bool {
    match (&left.kind, &right.kind) {
        (prisme_plan::ExprKind::Row(left), prisme_plan::ExprKind::Row(right)) => {
            left.len() == right.len()
                && left
                    .iter()
                    .zip(right)
                    .all(|(left, right)| same_shape(left, right))
        }
        (prisme_plan::ExprKind::Row(_), _) | (_, prisme_plan::ExprKind::Row(_)) => false,
        _ => true,
    }
}

impl Builder<'_> {
    /// Expression used as a single value: a predicate becomes an integer.
    pub(super) fn value(&mut self, expr: &ast::Expr, scope: &Scope) -> Result<Expr, BuildError> {
        let built = self.operand(expr, scope)?;
        self.single(&built, expr.span)?;
        Ok(built)
    }

    /// Expression used as a predicate: a value becomes a boolean.
    pub(super) fn condition(
        &mut self,
        expr: &ast::Expr,
        scope: &Scope,
    ) -> Result<Expr, BuildError> {
        let built = self.expr(expr, scope)?;
        self.single(&built, expr.span)?;
        Ok(as_predicate(built))
    }

    /// Operand of a comparison, a row or a value: a predicate becomes an
    /// integer.
    fn operand(&mut self, expr: &ast::Expr, scope: &Scope) -> Result<Expr, BuildError> {
        let built = self.expr(expr, scope)?;
        Ok(self.as_value(built))
    }

    /// Refuses a row where a single value is expected.
    fn single(&self, expr: &Expr, span: Span) -> Result<(), BuildError> {
        match &expr.kind {
            prisme_plan::ExprKind::Row(values) => Err(self.error(
                format!(
                    "a row of {} values where a single value is expected",
                    values.len()
                ),
                span,
            )),
            _ => Ok(()),
        }
    }

    /// Refuses operands of different shapes: `(a, b) = (1, 2, 3)`.
    fn same_shape(&self, left: &Expr, right: &Expr, span: Span) -> Result<(), BuildError> {
        if same_shape(left, right) {
            return Ok(());
        }
        Err(self.error(
            format!(
                "operands of {} and {} values are compared",
                arity(left),
                arity(right)
            ),
            span,
        ))
    }

    /// Integer from a predicate (`(a = 1) + 1`), other values unchanged.
    fn as_value(&self, expr: Expr) -> Expr {
        if expr.ty.data_type != DataType::Boolean {
            return expr;
        }
        let span = expr.span;
        let converted = Expr::to_integer(expr, types::boolean_integer(self.dialect));
        match span {
            Some(span) => converted.with_span(span),
            None => converted,
        }
    }

    /// Expression, typed.
    pub(super) fn expr(&mut self, expr: &ast::Expr, scope: &Scope) -> Result<Expr, BuildError> {
        let built = match &expr.kind {
            ExprKind::Nested(inner) => return self.expr(inner, scope),
            ExprKind::Column(name) => {
                let (column, ty) = self.resolve(name, scope)?;
                Expr::column(column, ty)
            }
            ExprKind::Literal(literal) => self.literal(literal, expr)?,
            ExprKind::Unary { op, operand } => {
                let operand_span = operand.span;
                let operand = self.expr(operand, scope)?;
                self.single(&operand, operand_span)?;
                match op {
                    UnaryOp::Minus => {
                        let operand = self.as_value(operand);
                        let ty = types::negate(&operand.ty);
                        Expr::negate(operand, ty)
                    }
                    UnaryOp::Plus => return Ok(self.as_value(operand)),
                    UnaryOp::Not => Expr::logical_not(as_predicate(operand)),
                    UnaryOp::BitNot => {
                        let operand = self.as_value(operand);
                        let ty = bigint_unsigned(operand.ty.nullable);
                        Expr::bit_not(operand, ty)
                    }
                }
            }
            ExprKind::Binary { op, left, right } => {
                let (left_span, right_span) = (left.span, right.span);
                let left = self.expr(left, scope)?;
                let right = self.expr(right, scope)?;
                let operator = operator(*op);
                if matches!(operator, Operator::Comparison(_) | Operator::NotDistinct) {
                    self.same_shape(&left, &right, expr.span)?;
                } else {
                    self.single(&left, left_span)?;
                    self.single(&right, right_span)?;
                }
                match operator {
                    Operator::Arithmetic(op) => {
                        let (left, right) = (self.as_value(left), self.as_value(right));
                        let ty = types::arithmetic(op, &left.ty, &right.ty);
                        Expr::arithmetic(op, left, right, ty)
                    }
                    Operator::Comparison(op) => {
                        Expr::compare(op, self.as_value(left), self.as_value(right))
                    }
                    Operator::NotDistinct => Expr::not_distinct(
                        self.as_value(left),
                        self.as_value(right),
                        false,
                        NotDistinctForm::Symbol,
                    ),
                    Operator::Logical(op) => {
                        Expr::logical(op, as_predicate(left), as_predicate(right))
                    }
                    Operator::Bitwise(op) => {
                        let (left, right) = (self.as_value(left), self.as_value(right));
                        let ty = bigint_unsigned(left.ty.nullable || right.ty.nullable);
                        Expr::bitwise(op, left, right, ty)
                    }
                }
            }
            ExprKind::IsNull { operand, negated } => {
                let built = self.expr(operand, scope)?;
                self.single(&built, operand.span)?;
                Expr::is_null(built, *negated)
            }
            ExprKind::Between {
                operand,
                low,
                high,
                negated,
            } => Expr::between(
                self.value(operand, scope)?,
                self.value(low, scope)?,
                self.value(high, scope)?,
                *negated,
            ),
            ExprKind::InList {
                operand,
                list,
                negated,
            } => {
                let operand = self.operand(operand, scope)?;
                let mut built = Vec::new();
                for value in list {
                    let value_built = self.operand(value, scope)?;
                    self.same_shape(&operand, &value_built, value.span)?;
                    built.push(value_built);
                }
                Expr::in_list(operand, built, *negated)
            }
            ExprKind::Like {
                operand,
                pattern,
                escape,
                negated,
            } => {
                let escape = match escape {
                    Some(escape) => Some(self.value(escape, scope)?),
                    None => None,
                };
                Expr::like(
                    self.value(operand, scope)?,
                    self.value(pattern, scope)?,
                    escape,
                    *negated,
                )
            }
            ExprKind::Subquery(_)
            | ExprKind::Exists(_)
            | ExprKind::InSubquery { .. }
            | ExprKind::Quantified { .. }
            | ExprKind::Row(_) => self.query_expr(expr, scope)?,
        };
        Ok(built.with_span(expr.span))
    }

    /// Subquery or row: out of `expr`, whose frame the recursion of deep
    /// expressions repeats.
    fn query_expr(&mut self, expr: &ast::Expr, scope: &Scope) -> Result<Expr, BuildError> {
        Ok(match &expr.kind {
            ExprKind::Subquery(query) => {
                let plan = self.subquery(query, scope)?;
                let columns = plan.schema().len();
                if columns > 1 {
                    return Err(self.error(
                        format!(
                            "subquery of {columns} columns: row subqueries are not supported yet by Prisme"
                        ),
                        expr.span,
                    ));
                }
                Expr::scalar_subquery(plan).map_err(|error| plan_error(&error, expr.span))?
            }
            ExprKind::Exists(query) => Expr::exists(self.subquery(query, scope)?, false),
            ExprKind::InSubquery {
                operand,
                query,
                negated,
            } => {
                let operand = self.operand(operand, scope)?;
                let plan = self.subquery(query, scope)?;
                Expr::in_subquery(operand, plan, *negated)
                    .map_err(|error| plan_error(&error, expr.span))?
            }
            ExprKind::Quantified {
                op,
                operand,
                all,
                query,
            } => {
                let Operator::Comparison(op) = operator(*op) else {
                    return Err(self.error("`<=>` has no `ANY` or `ALL`", expr.span));
                };
                let quantifier = if *all {
                    Quantifier::All
                } else {
                    Quantifier::Any
                };
                let operand_span = operand.span;
                let operand = self.operand(operand, scope)?;
                // A row compares with `= ANY` and `<> ALL` only, the forms of
                // `IN` and `NOT IN`.
                let membership = matches!(
                    (op, quantifier),
                    (Comparison::Eq, Quantifier::Any) | (Comparison::NotEq, Quantifier::All)
                );
                if !membership {
                    self.single(&operand, operand_span)?;
                }
                let plan = self.subquery(query, scope)?;
                Expr::quantified(op, operand, quantifier, plan)
                    .map_err(|error| plan_error(&error, expr.span))?
            }
            ExprKind::Row(values) => {
                let built = values
                    .iter()
                    .map(|value| self.operand(value, scope))
                    .collect::<Result<Vec<_>, _>>()?;
                Expr::row(built)
            }
            _ => unreachable!("called on subqueries and rows"),
        })
    }

    /// Literal, of the type the dialect gives it.
    fn literal(&self, literal: &Literal, expr: &ast::Expr) -> Result<Expr, BuildError> {
        let too_large = || self.error("number too large", expr.span);
        Ok(match literal {
            Literal::Integer(text) => match text.parse::<i128>() {
                Ok(value) if value <= i128::from(u64::MAX) => Expr::literal(
                    Value::Integer(value),
                    types::integer_literal(value, text.len(), self.dialect),
                ),
                _ => {
                    let decimal = Decimal::parse(text).map_err(|_| too_large())?;
                    let data_type = types::decimal(text.len(), 0);
                    if !matches!(data_type, DataType::Decimal(_)) || text.len() > 65 {
                        return Err(too_large());
                    }
                    Expr::literal(Value::Decimal(decimal), data_type)
                }
            },
            Literal::Decimal(text) => {
                let decimal = Decimal::parse(text).map_err(|_| too_large())?;
                let (precision, scale) = (decimal.precision(), decimal.scale());
                if precision > 65 {
                    return Err(too_large());
                }
                Expr::literal(Value::Decimal(decimal), types::decimal(precision, scale))
            }
            Literal::Float(text) => {
                let value = text.parse::<f64>().map_err(|_| too_large())?;
                if value.is_infinite() {
                    return Err(too_large());
                }
                Expr::literal(Value::Float(value), types::double())
            }
            Literal::String(value) => {
                Expr::literal(Value::Text(value.clone()), types::string_literal(value))
            }
            Literal::Null => Expr::literal(Value::Null, DataType::Null),
            Literal::True => Expr::literal(Value::Boolean(true), DataType::Boolean),
            Literal::False => Expr::literal(Value::Boolean(false), DataType::Boolean),
        })
    }
}
