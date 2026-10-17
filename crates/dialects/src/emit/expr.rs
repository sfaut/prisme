//! Expressions written in the MySQL family, with the parentheses their
//! precedence needs, from the loosest level to the tightest: `OR` (1),
//! `XOR` (2), `AND` (3), `NOT` (4), comparisons and predicates (5), `|`
//! (6), `&` (7), shifts (8), `+` and `-` (9), `*`, `/`, `DIV`, `%` (10),
//! `^` (11), prefix `-` and `~` (12), operands (14).
//!
//! The conversions between booleans and numbers are implicit in the MySQL
//! family: `ToBoolean` and `ToInteger` write their operand alone.

use prisme_plan::{
    Arithmetic, Bitwise, Comparison, Expr, ExprKind, Logical, NotDistinctForm, VariableScope,
};
use prisme_types::Value;

use super::Emitter;
use crate::report::StepKind;

/// Level of `NOT`.
const NOT: u8 = 4;
/// Level of the comparisons and predicates.
const PREDICATE: u8 = 5;
/// Level of the prefix operators.
const PREFIX: u8 = 12;
/// Level of an operand.
const OPERAND: u8 = 14;

/// Level of an expression, and the levels its left and right operands need.
fn levels(expr: &Expr) -> (u8, u8, u8) {
    let binary = |level: u8| (level, level, level + 1);
    match &expr.kind {
        ExprKind::Logical { op, .. } => match op {
            Logical::Or => binary(1),
            Logical::Xor => binary(2),
            Logical::And => binary(3),
        },
        ExprKind::Not(_) => (NOT, NOT, NOT),
        ExprKind::NotDistinct { negated: true, .. } => (NOT, NOT, NOT),
        ExprKind::Compare { .. } | ExprKind::NotDistinct { .. } => binary(PREDICATE),
        ExprKind::IsNull { .. }
        | ExprKind::Between { .. }
        | ExprKind::Like { .. }
        | ExprKind::InList { .. } => (PREDICATE, PREDICATE + 1, PREDICATE + 1),
        ExprKind::Bitwise { op, .. } => match op {
            Bitwise::Or => binary(6),
            Bitwise::And => binary(7),
            Bitwise::ShiftLeft | Bitwise::ShiftRight => binary(8),
            Bitwise::Xor => binary(11),
        },
        ExprKind::Arithmetic { op, .. } => match op {
            Arithmetic::Add | Arithmetic::Subtract => binary(9),
            _ => binary(10),
        },
        // An operand that is itself negative is put between parentheses:
        // `--` could open a comment.
        ExprKind::Negate(_) => (PREFIX, PREFIX + 1, PREFIX + 1),
        ExprKind::BitNot(_) => (PREFIX, PREFIX, PREFIX),
        ExprKind::Literal(Value::Integer(value)) if *value < 0 => (PREFIX, 0, 0),
        ExprKind::Literal(Value::Float(value)) if *value < 0.0 => (PREFIX, 0, 0),
        ExprKind::Literal(Value::Decimal(value)) if value.as_str().starts_with('-') => {
            (PREFIX, 0, 0)
        }
        _ => (OPERAND, 0, 0),
    }
}

impl Emitter<'_> {
    /// Writes `expr`, between parentheses when its level is below `min`.
    pub(super) fn expr(&mut self, expr: &Expr, min: u8) {
        if let ExprKind::ToBoolean(inner) | ExprKind::ToInteger(inner) = &expr.kind {
            self.expr(inner, min);
            return;
        }
        self.at(expr.span);
        let (level, left, right) = levels(expr);
        let parenthesized = level < min;
        if parenthesized {
            self.write("(");
        }
        self.expr_kind(expr, left, right);
        if parenthesized {
            self.write(")");
        }
    }

    /// Writes the content of `expr`, its operands at levels `left` and
    /// `right` at least.
    fn expr_kind(&mut self, expr: &Expr, left: u8, right: u8) {
        match &expr.kind {
            ExprKind::Column(column) => {
                if let Some(qualifier) = &column.qualifier {
                    self.ident(qualifier, expr.span);
                    self.write(".");
                }
                self.ident(&column.name, expr.span);
            }
            ExprKind::Literal(value) => self.literal(expr, value),
            ExprKind::Variable(variable) => {
                self.write(match variable.scope {
                    VariableScope::User => "@",
                    VariableScope::Session => "@@",
                    VariableScope::Global => "@@global.",
                });
                self.ident(&variable.name, expr.span);
            }
            ExprKind::Compare {
                op,
                left: a,
                right: b,
            } => {
                let symbol = match op {
                    Comparison::Eq => " = ",
                    Comparison::NotEq => " <> ",
                    Comparison::Lt => " < ",
                    Comparison::LtEq => " <= ",
                    Comparison::Gt => " > ",
                    Comparison::GtEq => " >= ",
                };
                self.binary(a, symbol, b, left, right);
            }
            ExprKind::NotDistinct {
                left: a,
                right: b,
                negated,
                form,
            } => {
                if *form == NotDistinctForm::Keywords {
                    let message = format!(
                        "{} writes the identity equality `<=>`",
                        self.output.dialect.label()
                    );
                    self.step("not-distinct.symbol", StepKind::Rewrite, message, expr.span);
                }
                if *negated {
                    self.write("NOT (");
                    self.binary(a, " <=> ", b, PREDICATE, PREDICATE + 1);
                    self.write(")");
                } else {
                    self.binary(a, " <=> ", b, left, right);
                }
            }
            ExprKind::Logical {
                op,
                left: a,
                right: b,
            } => {
                let word = match op {
                    Logical::And => " AND ",
                    Logical::Or => " OR ",
                    Logical::Xor => " XOR ",
                };
                self.binary(a, word, b, left, right);
            }
            ExprKind::Not(operand) => {
                self.write("NOT ");
                self.expr(operand, NOT);
            }
            ExprKind::IsNull { operand, negated } => {
                self.expr(operand, left);
                self.write(if *negated { " IS NOT NULL" } else { " IS NULL" });
            }
            ExprKind::Arithmetic {
                op,
                left: a,
                right: b,
            } => {
                let symbol = match op {
                    Arithmetic::Add => " + ",
                    Arithmetic::Subtract => " - ",
                    Arithmetic::Multiply => " * ",
                    Arithmetic::Divide => " / ",
                    Arithmetic::IntegerDivide => " DIV ",
                    Arithmetic::Modulo => " % ",
                };
                self.binary(a, symbol, b, left, right);
            }
            ExprKind::Negate(operand) => {
                self.write("-");
                self.expr(operand, right);
            }
            ExprKind::Bitwise {
                op,
                left: a,
                right: b,
            } => {
                let symbol = match op {
                    Bitwise::And => " & ",
                    Bitwise::Or => " | ",
                    Bitwise::Xor => " ^ ",
                    Bitwise::ShiftLeft => " << ",
                    Bitwise::ShiftRight => " >> ",
                };
                self.binary(a, symbol, b, left, right);
            }
            ExprKind::BitNot(operand) => {
                self.write("~");
                self.expr(operand, right);
            }
            ExprKind::Between {
                operand,
                low,
                high,
                negated,
            } => {
                self.expr(operand, left);
                self.write(if *negated {
                    " NOT BETWEEN "
                } else {
                    " BETWEEN "
                });
                self.expr(low, right);
                self.write(" AND ");
                self.expr(high, right);
            }
            ExprKind::Like {
                operand,
                pattern,
                escape,
                negated,
            } => {
                self.expr(operand, left);
                self.write(if *negated { " NOT LIKE " } else { " LIKE " });
                self.expr(pattern, right);
                if let Some(escape) = escape {
                    self.write(" ESCAPE ");
                    self.expr(escape, right);
                }
            }
            ExprKind::InList {
                operand,
                list,
                negated,
            } => {
                self.expr(operand, left);
                self.write(if *negated { " NOT IN (" } else { " IN (" });
                self.expr_list(list);
                self.write(")");
            }
            _ => {
                self.error(
                    "expr.kind",
                    "expression not supported yet by the emitter of Prisme",
                    expr.span,
                );
                self.write("NULL");
            }
        }
    }

    /// `a op b`, the operands at levels `left` and `right` at least.
    fn binary(&mut self, a: &Expr, op: &str, b: &Expr, left: u8, right: u8) {
        self.expr(a, left);
        self.write(op);
        self.expr(b, right);
    }

    /// Literal: its text in the input when the output reads it the same way,
    /// its canonical spelling otherwise.
    fn literal(&mut self, expr: &Expr, value: &Value) {
        let same_reading = self.source.mode.ansi_quotes == self.output.mode.ansi_quotes
            && self.source.mode.no_backslash_escapes == self.output.mode.no_backslash_escapes;
        if same_reading
            && let Some(span) = expr.span
            && let Some(text) = self.source.text.get(span.start..span.end)
        {
            self.write(text);
            return;
        }
        let text = match value {
            Value::Null => "NULL".to_string(),
            Value::Boolean(true) => "TRUE".to_string(),
            Value::Boolean(false) => "FALSE".to_string(),
            Value::Integer(value) => value.to_string(),
            Value::Decimal(value) => value.as_str().to_string(),
            Value::Float(value) => format!("{value:e}"),
            Value::Text(value) => self.string(value),
            Value::Binary(bytes) => {
                let digits: String = bytes.iter().map(|byte| format!("{byte:02X}")).collect();
                format!("X'{digits}'")
            }
            Value::Date(text) => format!("DATE {}", self.string(text)),
            Value::Time(text) => format!("TIME {}", self.string(text)),
            Value::DateTime(text) => format!("TIMESTAMP {}", self.string(text)),
        };
        self.write(&text);
    }

    /// String between single quotes, escaped for the SQL mode of the
    /// output.
    fn string(&self, value: &str) -> String {
        let mut quoted = String::with_capacity(value.len() + 2);
        quoted.push('\'');
        for character in value.chars() {
            match character {
                '\'' => quoted.push_str("''"),
                '\\' if !self.output.mode.no_backslash_escapes => quoted.push_str("\\\\"),
                '\0' if !self.output.mode.no_backslash_escapes => quoted.push_str("\\0"),
                _ => quoted.push(character),
            }
        }
        quoted.push('\'');
        quoted
    }
}
