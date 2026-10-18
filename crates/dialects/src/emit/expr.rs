//! Expressions written in the MySQL family, with the parentheses their
//! precedence needs, from the loosest level to the tightest: the assignment
//! of a user variable (0), `OR` (1),
//! `XOR` (2), `AND` (3), `NOT` and `IS TRUE` (4), comparisons and
//! predicates (5), `|`
//! (6), `&` (7), shifts (8), `+` and `-` (9), `*`, `/`, `DIV`, `%` (10),
//! `^` (11), prefix `-` and `~` (12), operands (14).
//!
//! The conversions between booleans and numbers are implicit in the MySQL
//! family: `ToBoolean` and `ToInteger` write their operand alone, but a
//! `JSON` read as a boolean, which each server reads its own way (`truth`
//! module). A `JSON` compared, sorted or converted to a number is read as
//! the input reads it (`json` module).

use prisme_plan::{
    Arithmetic, Bitwise, Comparison, Expr, ExprKind, Logical, NotDistinctForm, Plan, Quantifier,
};
use prisme_types::{DataType, TextType, Value};

use super::Emitter;
use super::case::Reading;
use super::hexadecimal::{bytes_canonical, reads_as_string};
use crate::Dialect;
use crate::build::is_internal_name;
use crate::report::StepKind;
use crate::sql::ast::TemporalKind;

/// Level of `OR`.
pub(super) const OR: u8 = 1;
/// Level of `AND`.
pub(super) const AND: u8 = 3;
/// Level of `NOT`.
pub(super) const NOT: u8 = 4;
/// Level of `+` and `-`.
pub(super) const ADDITIVE: u8 = 9;
/// Level of `*`, `/`, `DIV` and `%`.
pub(super) const MULTIPLY: u8 = 10;
/// Level of the comparisons and predicates.
pub(super) const PREDICATE: u8 = 5;
/// Level of the prefix operators.
pub(super) const PREFIX: u8 = 12;
/// Level of an operand.
pub(super) const OPERAND: u8 = 14;

/// Symbol of a comparison, between spaces.
const fn comparison(op: Comparison) -> &'static str {
    match op {
        Comparison::Eq => " = ",
        Comparison::NotEq => " <> ",
        Comparison::Lt => " < ",
        Comparison::LtEq => " <= ",
        Comparison::Gt => " > ",
        Comparison::GtEq => " >= ",
    }
}

/// Level of an expression, and the levels its left and right operands need.
fn levels(expr: &Expr) -> (u8, u8, u8) {
    let binary = |level: u8| (level, level, level + 1);
    match &expr.kind {
        // Its value extends as far as an expression goes.
        ExprKind::Assign { .. } => (0, 0, 0),
        ExprKind::Logical { op, .. } => match op {
            Logical::Or => binary(OR),
            Logical::Xor => binary(2),
            Logical::And => binary(AND),
        },
        ExprKind::Not(_) => (NOT, NOT, NOT),
        // MySQL reads no operator after `IS TRUE`: as an operand of a
        // comparison or of `IS`, it is put between parentheses.
        ExprKind::IsTruth { .. } => (NOT, PREDICATE, PREDICATE),
        ExprKind::NotDistinct { negated: true, .. } => (NOT, NOT, NOT),
        ExprKind::NotDistinct { .. } => binary(PREDICATE),
        ExprKind::Compare { .. } | ExprKind::Quantified { .. } => binary(PREDICATE),
        ExprKind::IsNull { .. }
        | ExprKind::Between { .. }
        | ExprKind::Like { .. }
        | ExprKind::Regexp { .. }
        | ExprKind::SoundsLike { .. }
        | ExprKind::MemberOf { .. }
        | ExprKind::InList { .. }
        | ExprKind::InSubquery { .. } => (PREDICATE, PREDICATE + 1, PREDICATE + 1),
        ExprKind::Exists { negated: true, .. } => (NOT, NOT, NOT),
        ExprKind::Bitwise { op, .. } => match op {
            Bitwise::Or => binary(6),
            Bitwise::And => binary(7),
            Bitwise::ShiftLeft | Bitwise::ShiftRight => binary(8),
            Bitwise::Xor => binary(11),
        },
        ExprKind::Arithmetic { op, .. } => match op {
            Arithmetic::Add | Arithmetic::Subtract => binary(ADDITIVE),
            _ => binary(MULTIPLY),
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
        if let ExprKind::Alias(alias) = &expr.kind {
            self.alias(expr, alias, min);
            return;
        }
        // Read as a value of its type, as a number, a `JSON` kept as is, as
        // bytes: this expression, never its operands.
        let column = std::mem::take(&mut self.column);
        let number = std::mem::take(&mut self.number);
        let json_as_is = std::mem::take(&mut self.json_as_is);
        let as_bytes = std::mem::take(&mut self.as_bytes) || column;
        self.hybrids(expr, as_bytes);
        if let ExprKind::ToBoolean(inner) | ExprKind::ToInteger(inner) = &expr.kind {
            if matches!(expr.kind, ExprKind::ToBoolean(_)) && inner.ty.data_type == DataType::Json {
                self.json_condition(inner, min, true);
            } else if !(matches!(expr.kind, ExprKind::ToInteger(_))
                && self.regexp_number(inner, min))
            {
                self.number_expr(inner, min);
            }
            return;
        }
        if let ExprKind::Column(column) = &expr.kind
            && column.qualifier.is_none()
            && is_internal_name(&column.name.value)
            && self.internal(expr, &column.name, min)
        {
            return;
        }
        if !json_as_is && self.json_text(expr) {
            return;
        }
        self.at(expr.span);
        // A rule may write a call as an expression of a lower level.
        if let ExprKind::Function(call) = &expr.kind {
            self.function_call(expr, call, min, column);
            return;
        }
        if let ExprKind::Case(case) = &expr.kind {
            self.case(expr, case, Reading { column, number });
            return;
        }
        if let ExprKind::Cast(cast) = &expr.kind {
            self.cast(expr, cast);
            return;
        }
        if let ExprKind::DateArithmetic(arithmetic) = &expr.kind {
            self.date_arithmetic(expr, arithmetic, min);
            return;
        }
        if let ExprKind::TimestampDiff(diff) = &expr.kind {
            self.timestamp_diff(expr, diff);
            return;
        }
        if let ExprKind::Extract(extract) = &expr.kind {
            self.extract(expr, extract, min);
            return;
        }
        if let ExprKind::Trim(trim) = &expr.kind {
            self.trim(expr, trim);
            return;
        }
        if let ExprKind::Position(position) = &expr.kind {
            self.position(expr, position);
            return;
        }
        if self.json_in(expr, min) {
            return;
        }
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
            ExprKind::Variable(variable) => self.variable(variable, expr.span),
            ExprKind::Assign { name, value } => self.assign(expr, name, value),
            ExprKind::Compare {
                op,
                left: a,
                right: b,
            } => self.compared(a, comparison(*op), b, left, right),
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
                    self.compared(a, " <=> ", b, PREDICATE, PREDICATE + 1);
                    self.write(")");
                } else {
                    self.compared(a, " <=> ", b, left, right);
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
                    Logical::Xor => return self.xor(a, b, left, right, false),
                };
                self.binary(a, word, b, left, right);
            }
            ExprKind::Not(operand) => {
                self.write("NOT ");
                // Some versions read `NOT (a XOR b)` as `(NOT a) XOR b`
                // (`truth` module).
                if let ExprKind::Logical {
                    op: Logical::Xor,
                    left: a,
                    right: b,
                } = &operand.kind
                {
                    let (_, left, right) = levels(operand);
                    self.write("(");
                    self.at(operand.span);
                    self.xor(a, b, left, right, true);
                    self.write(")");
                } else {
                    self.number_expr(operand, NOT);
                }
            }
            ExprKind::IsNull { operand, negated } => {
                self.json_as_is = true;
                self.number_expr(operand, left);
                self.write(if *negated { " IS NOT NULL" } else { " IS NULL" });
            }
            ExprKind::IsTruth {
                operand,
                truth,
                negated,
            } => {
                self.expr(operand, left);
                self.write(if *negated { " IS NOT " } else { " IS " });
                self.write(truth.keyword());
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
                self.number_expr(operand, right);
            }
            ExprKind::Bitwise {
                op,
                left: a,
                right: b,
            } => self.bitwise(expr, *op, (a, b), (left, right)),
            ExprKind::BitNot(operand) => self.bit_not(expr, operand, right),
            ExprKind::Between {
                operand,
                low,
                high,
                negated,
            } => {
                let strings = reads_as_string(low) || reads_as_string(high);
                self.compared_expr(operand, left, strings);
                self.write(if *negated {
                    " NOT BETWEEN "
                } else {
                    " BETWEEN "
                });
                self.compared_expr(low, right, reads_as_string(operand));
                self.write(" AND ");
                self.compared_expr(high, right, reads_as_string(operand));
            }
            ExprKind::Like {
                operand,
                pattern,
                escape,
                negated,
            } => {
                // MySQL reads a simple operand after `LIKE` and `ESCAPE`.
                let right = match self.output.dialect {
                    Dialect::MySql => PREFIX,
                    Dialect::MariaDb => right,
                };
                // Both dialects read the text of a `JSON` there, the bytes
                // of a hexadecimal literal.
                self.json_as_is = true;
                self.as_bytes = true;
                self.expr(operand, left);
                self.write(if *negated { " NOT LIKE " } else { " LIKE " });
                self.json_as_is = true;
                self.as_bytes = true;
                self.expr(pattern, right);
                if let Some(escape) = escape {
                    self.write(" ESCAPE ");
                    self.json_as_is = true;
                    self.as_bytes = true;
                    self.expr(escape, right);
                }
            }
            ExprKind::InList {
                operand,
                list,
                negated,
            } => {
                self.compared_expr(operand, left, list.iter().any(reads_as_string));
                self.write(if *negated { " NOT IN (" } else { " IN (" });
                for (index, item) in list.iter().enumerate() {
                    if index > 0 {
                        self.write(", ");
                    }
                    self.compared_expr(item, 0, reads_as_string(operand));
                }
                self.write(")");
            }
            ExprKind::ScalarSubquery(_)
            | ExprKind::Exists { .. }
            | ExprKind::InSubquery { .. }
            | ExprKind::Quantified { .. }
            | ExprKind::Row(_) => self.query_kind(expr, left),
            ExprKind::Window(call) => self.window_call(expr, call),
            ExprKind::Regexp { .. } => self.regexp(expr, left, right),
            ExprKind::SoundsLike { .. } => self.sounds_like(expr, left, right),
            ExprKind::MemberOf { .. } => self.member_of(expr, left),
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

    /// Content of a subquery or of a row, its operand at level `left` at
    /// least: out of `expr_kind`, whose frame the recursion of deep
    /// expressions repeats.
    fn query_kind(&mut self, expr: &Expr, left: u8) {
        match &expr.kind {
            ExprKind::ScalarSubquery(subquery) => self.subquery("", subquery),
            ExprKind::Exists { subquery, negated } => {
                self.subquery(if *negated { "NOT EXISTS " } else { "EXISTS " }, subquery);
            }
            ExprKind::InSubquery {
                operand,
                subquery,
                negated,
            } => {
                self.number_expr(operand, left);
                self.compared_rows = true;
                self.subquery(if *negated { " NOT IN " } else { " IN " }, subquery);
            }
            ExprKind::Quantified {
                op,
                operand,
                quantifier,
                subquery,
            } => {
                self.number_expr(operand, left);
                self.write(comparison(*op));
                self.compared_rows = true;
                self.subquery(
                    match quantifier {
                        Quantifier::All => "ALL ",
                        Quantifier::Any => "ANY ",
                    },
                    subquery,
                );
            }
            ExprKind::Row(values) => {
                self.write("(");
                self.expr_list(values);
                self.write(")");
            }
            _ => unreachable!("called on subqueries and rows"),
        }
    }

    /// `prefix (subquery)`.
    fn subquery(&mut self, prefix: &str, subquery: &Plan) {
        self.write(prefix);
        self.write("(");
        self.query(subquery, false);
        self.write(")");
    }

    /// `a op b`, the operands at levels `left` and `right` at least.
    fn binary(&mut self, a: &Expr, op: &str, b: &Expr, left: u8, right: u8) {
        self.number_expr(a, left);
        self.write(op);
        self.number_expr(b, right);
    }

    /// `a op b`, a comparison, the operands at levels `left` and `right` at
    /// least, each read as bytes when the other is a string.
    fn compared(&mut self, a: &Expr, op: &str, b: &Expr, left: u8, right: u8) {
        self.compared_expr(a, left, reads_as_string(b));
        self.write(op);
        self.compared_expr(b, right, reads_as_string(a));
    }

    /// Writes `expr`, an operand of a comparison, read as bytes when
    /// `as_bytes`, as a number otherwise.
    fn compared_expr(&mut self, expr: &Expr, min: u8, as_bytes: bool) {
        self.as_bytes = as_bytes;
        self.number_expr(expr, min);
    }

    /// Writes `expr` read as a number (see `Emitter::number`).
    pub(super) fn number_expr(&mut self, expr: &Expr, min: u8) {
        self.number = true;
        self.expr(expr, min);
    }

    /// Literal: its text in the input when the output reads it the same way,
    /// its canonical spelling otherwise.
    fn literal(&mut self, expr: &Expr, value: &Value) {
        let temporal = match value {
            Value::Date(text) => Some((TemporalKind::Date, text)),
            Value::Time(text) => Some((TemporalKind::Time, text)),
            Value::DateTime(text) => Some((TemporalKind::Timestamp, text)),
            _ => None,
        };
        if let Some((kind, text)) = temporal
            && let Some(rewritten) = self.temporal_literal(expr, kind, text)
        {
            self.write(&rewritten);
            return;
        }
        self.introducer(expr);
        if let Value::Binary(bytes) = value
            && let Some(text) = self.bytes_text(expr, bytes)
        {
            self.write(&text);
            return;
        }
        let same_reading = self.source.mode.ansi_quotes == self.output.mode.ansi_quotes
            && self.source.mode.no_backslash_escapes == self.output.mode.no_backslash_escapes;
        if same_reading
            && let Some(span) = expr.span
            && let Some(text) = self.source.text.get(span.start..span.end)
        {
            self.input_text(span, text);
            return;
        }
        let text = match value {
            Value::Null => "NULL".to_string(),
            Value::Boolean(true) => "TRUE".to_string(),
            Value::Boolean(false) => "FALSE".to_string(),
            Value::Integer(value) => value.to_string(),
            Value::Decimal(value) => value.as_str().to_string(),
            Value::Float(value) => format!("{value:e}"),
            Value::Text(value) => match &expr.ty.data_type {
                DataType::Text(TextType {
                    charset: Some(charset),
                    ..
                }) => self.introduced_string(expr, value, charset),
                _ => self.string(value),
            },
            Value::Binary(bytes) => bytes_canonical(expr, bytes),
            Value::Date(text) => format!("DATE {}", self.string(text)),
            Value::Time(text) => format!("TIME {}", self.string(text)),
            Value::DateTime(text) => format!("TIMESTAMP {}", self.string(text)),
        };
        self.write(&text);
    }

    /// String between single quotes, escaped for the SQL mode of the
    /// output.
    pub(super) fn string(&self, value: &str) -> String {
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
