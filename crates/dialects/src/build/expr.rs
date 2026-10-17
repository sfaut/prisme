//! Expressions of the MySQL family, typed, with the explicit conversions
//! between booleans and numbers (`docs/FEDERATED.md`, section 7, point 2):
//! a predicate used as a value becomes an integer (`ToInteger`), a value
//! used as a predicate becomes a boolean (`ToBoolean`).

use prisme_plan::{Arithmetic, Bitwise, Comparison, Expr, Logical, NotDistinctForm};
use prisme_types::{DataType, Decimal, IntegerSize, IntegerType, Type, Value};

use super::scope::{ResolveError, Scope};
use super::{BuildError, Builder, as_predicate, types};
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

impl Builder<'_> {
    /// Expression used as a value: a predicate becomes an integer.
    pub(super) fn value(&mut self, expr: &ast::Expr, scope: &Scope) -> Result<Expr, BuildError> {
        let built = self.expr(expr, scope)?;
        Ok(self.as_value(built))
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
                let (column, ty) = scope.resolve(name).map_err(|error| {
                    let written = &self.text[name.span.start..name.span.end];
                    let message = match error {
                        ResolveError::Unknown => format!("unknown column `{written}`"),
                        ResolveError::Ambiguous => format!("column `{written}` is ambiguous"),
                    };
                    self.error(message, name.span)
                })?;
                Expr::column(column, ty)
            }
            ExprKind::Literal(literal) => self.literal(literal, expr)?,
            ExprKind::Unary { op, operand } => {
                let operand = self.expr(operand, scope)?;
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
                let left = self.expr(left, scope)?;
                let right = self.expr(right, scope)?;
                match operator(*op) {
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
                Expr::is_null(self.expr(operand, scope)?, *negated)
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
                let operand = self.value(operand, scope)?;
                let list = list
                    .iter()
                    .map(|value| self.value(value, scope))
                    .collect::<Result<Vec<_>, _>>()?;
                Expr::in_list(operand, list, *negated)
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
        };
        Ok(built.with_span(expr.span))
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
