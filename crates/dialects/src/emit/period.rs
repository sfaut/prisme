//! `PERIOD_ADD` and `PERIOD_DIFF` between MySQL and MariaDB (rule
//! `function.period`), from their sources (`docs/sources/mysql/period.md`,
//! `docs/sources/mariadb/period.md`) and measured on each local image
//! (`crates/probe/cases/period.txt`, `period-arguments.txt`,
//! `period-forms.sql`). Both read their arguments as integers (a decimal
//! rounded, a string of digits) and a period as `YYMM` or `YYYYMM`, a year
//! below 70 in the 2000s, below 100 in the 1900s. They agree on a period from
//! 1 to 999912 of a month from 1 to 12 and, for `PERIOD_ADD`, on a sum of
//! months from the year 0 to 2^31 - 1 months. Out of it:
//!
//! - MySQL refuses a period not positive or of a month other than 1 to 12
//!   (error 1210), where MariaDB gives a value (`PERIOD_DIFF(202413, 202401)`
//!   gives 12): left as is, as the other errors of a dialect against a value
//!   of the other (reference files);
//! - beyond 999912, a year after 9999, MySQL computes the period, MariaDB
//!   reads 0 months (`PERIOD_ADD(1000001, 1)` gives 1000002, 200002 in
//!   MariaDB). Towards MariaDB, the months of MySQL written out,
//!   `p DIV 100 * 12 + p MOD 100 - 1`, a signed integer by `CAST` for an
//!   unsigned argument, which MariaDB would merge with `PERIOD_ADD` into a
//!   `DECIMAL`. Towards MySQL, `PERIOD_DIFF` takes the period 7001 and
//!   takes off its 23640 months; `PERIOD_ADD` takes the period 0,
//!   which MySQL refuses as the periods MariaDB reads otherwise (decided
//!   autonomously: MariaDB adds the months to 0 in 32 bits, which MySQL
//!   cannot compute before 8.0.16);
//! - MariaDB reads the months in 32 bits and adds them in 32 bits, MySQL in
//!   64 bits from 8.0.16 and reads them in 32 bits before: months beyond
//!   2^31 - 1 or a sum before the year 0 give values of their own. Towards
//!   MariaDB, an error; towards MySQL, the period 0;
//! - MySQL before 8.0.16 reads a negative period as unsigned
//!   (`PERIOD_ADD(-202404, 1)` gives -202315, an error from 8.0.16): an
//!   error towards MariaDB, the period 0 or 7001 towards those versions.
//!
//! An argument is read by the values of its type: an integer, a `YEAR`, a
//! `BIT`, a `DECIMAL` without fraction; a literal by its value. The rule
//! writes only what these values need, and computes the literals as each
//! server does: written as is when the results agree, an error otherwise.
//! A date (`20240506`, beyond 999912), a time (MySQL rounds its fraction,
//! MariaDB drops it), a floating point number, a decimal with a fraction, a
//! string other than a literal integer, a byte string, a `JSON` and a value
//! of a type unknown to Prisme are errors.

use prisme_plan::{Expr, ExprKind, FunctionCall};
use prisme_types::{DataType, IntegerSize, IntegerType, Value};

use super::Emitter;
use super::expr::{ADDITIVE, MULTIPLY, PREDICATE};
use super::interval::round;
use super::rules::repeatable;
use crate::Dialect;
use crate::builtins::Rule;
use crate::report::StepKind;
use crate::versions::Construct;

/// Largest period MariaDB reads, of the year 9999: 0 months beyond
/// (`convert_period_to_month`).
const MARIADB_MAX: i128 = 999_912;
/// A year of two digits below it is of the 2000s, of the 1900s otherwise
/// (`YY_PART_YEAR`).
const YY_PART_YEAR: u64 = 70;
/// Period whose months stand for the 0 months of MariaDB in `PERIOD_DIFF`
/// towards MySQL.
const BASE_PERIOD: i128 = 7001;
/// Months of `BASE_PERIOD`, 1970 years.
const BASE_MONTHS: i128 = 23_640;
/// Fewest months of a period beyond 999912 (1000001, the year 10000) that
/// keep a year of three digits once added, as the months of MySQL written
/// out need: 1200 months, the year 100.
const FEWEST_BEYOND: i128 = 120_000 - 1200;

/// How both servers read an argument.
enum Argument {
    /// The `NULL` literal: the result is NULL.
    Null,
    /// The integers they read.
    Values(Values),
    /// Read otherwise by the dialects, or of a type unknown to Prisme: an
    /// error, for this reason.
    Refused(&'static str),
}

/// Integers the servers read from an argument (`val_int`), those of a
/// literal or of its type.
#[derive(Debug, Clone, Copy)]
struct Values {
    /// Smallest one.
    low: i128,
    /// Largest one.
    high: i128,
    /// A literal, whose value is `low`.
    literal: bool,
    /// A signed integer, or a literal: its arithmetic keeps a signed
    /// integer, which `CAST(x AS SIGNED)` gives to the others (MariaDB
    /// merges an unsigned integer with `PERIOD_ADD` into a `DECIMAL`).
    signed: bool,
}

impl Values {
    /// Tells whether MariaDB may read 0 months of the period, a value
    /// beyond 999912.
    fn beyond(self) -> bool {
        self.high > MARIADB_MAX
    }

    /// Tells whether MySQL before 8.0.16 may read the period as unsigned: a
    /// negative value, or a `BIGINT UNSIGNED` from 2^63.
    fn negative(self) -> bool {
        self.low < 0 || self.high > i128::from(i64::MAX)
    }

    /// The value of a literal.
    fn value(self) -> Option<i64> {
        self.literal.then(|| reading(self.low))
    }
}

impl Emitter<'_> {
    /// Writes the call `expr` of `PERIOD_ADD` or `PERIOD_DIFF`, `call`, at
    /// level `min` at least.
    pub(super) fn period(&mut self, rule: Rule, expr: &Expr, call: &FunctionCall, min: u8) {
        // The server reports the number of arguments.
        let [first, second] = call.args.as_slice() else {
            self.input_call(expr, call);
            return;
        };
        let add = call.name.value.eq_ignore_ascii_case("PERIOD_ADD");
        let name = if add { "PERIOD_ADD" } else { "PERIOD_DIFF" };
        let (a, b) = match (argument(first), argument(second)) {
            (Argument::Null, _) | (_, Argument::Null) => {
                self.input_call(expr, call);
                return;
            }
            (Argument::Refused(reason), _) | (_, Argument::Refused(reason)) => {
                let message = format!("`{name}` of {reason}");
                self.error(rule.id(), &message, expr.span);
                self.input_call(expr, call);
                return;
            }
            (Argument::Values(a), Argument::Values(b)) => (a, b),
        };
        if let (Some(x), Some(y)) = (a.value(), b.value()) {
            self.period_literals(rule, expr, call, add, (x, y));
            return;
        }
        match self.output.dialect {
            Dialect::MariaDb if add => self.period_add_mariadb(rule, expr, call, (a, b)),
            Dialect::MariaDb => self.period_diff_mariadb(rule, expr, call, (a, b), min),
            Dialect::MySql if add => self.period_add_mysql(rule, expr, call, (a, b)),
            Dialect::MySql => self.period_diff_mysql(rule, expr, call, (a, b), min),
        }
    }

    /// Writes `call` of two literals, `values`, as is when both servers
    /// give the same result, an error otherwise.
    fn period_literals(
        &mut self,
        rule: Rule,
        expr: &Expr,
        call: &FunctionCall,
        add: bool,
        values: (i64, i64),
    ) {
        let result = |dialect: Dialect, wide: bool| match dialect {
            Dialect::MySql => mysql_period(add, wide, values),
            Dialect::MariaDb => Some(mariadb_period(add, values)),
        };
        let input = result(
            self.source.dialect,
            self.input_has(Construct::PeriodLongLong),
        );
        let output = result(self.output.dialect, self.has(Construct::PeriodLongLong));
        if input != output {
            let text = |result: Option<i64>| {
                result.map_or_else(|| "an error (1210)".to_string(), |value| value.to_string())
            };
            let message = format!(
                "`{}({}, {})`: {} gives {}, {} {}",
                call.name.value.to_ascii_uppercase(),
                values.0,
                values.1,
                self.source_name(),
                text(input),
                self.target(),
                text(output)
            );
            self.error(rule.id(), &message, expr.span);
        }
        self.input_call(expr, call);
    }

    /// `PERIOD_ADD(p, m)` towards MariaDB: the months of a period beyond
    /// 999912 written out, `IF(p > 999912, (s) DIV 12 * 100 + (s) MOD 12 +
    /// 1, PERIOD_ADD(p, m))` where `s`, `p DIV 100 * 12 + p MOD 100 - 1 +
    /// m`, holds the months of MySQL; within `CAST(... AS SIGNED)` for an
    /// unsigned argument.
    fn period_add_mariadb(
        &mut self,
        rule: Rule,
        expr: &Expr,
        call: &FunctionCall,
        (p, m): (Values, Values),
    ) {
        if !self.input_has(Construct::PeriodLongLong) && p.negative() {
            self.period_unsigned(rule, expr, call);
            return;
        }
        let mut lowest = fewest_months(p.low, p.high).map_or(i128::MIN, |fewest| -fewest);
        if p.beyond() {
            lowest = lowest.max(-FEWEST_BEYOND);
        }
        if m.low < lowest || m.high > i128::from(i32::MAX) {
            self.period_months(rule, expr, call);
            return;
        }
        if !p.beyond() {
            self.input_call(expr, call);
            return;
        }
        let [period, months] = call.args.as_slice() else {
            unreachable!("two arguments")
        };
        if !repeatable(period) || !repeatable(months) {
            self.period_repeated(rule, expr, call);
            return;
        }
        let message = format!(
            "{} reads a period beyond 999912 as 0 months, {} as a year after 9999 (`PERIOD_ADD(1000001, 1)` gives 200002, 1000002 in MySQL): its months written out",
            self.target(),
            self.source_name()
        );
        self.step(rule.id(), StepKind::Emulation, message, expr.span);
        let cast = !p.signed || !m.signed;
        self.write("IF(");
        self.integer(period, p, PREDICATE + 1);
        self.write(" > 999912, ");
        if cast {
            self.write("CAST(");
        }
        for suffix in [" DIV 12 * 100 + ", " MOD 12 + 1"] {
            self.write("(");
            self.months_of(period, p);
            self.write(" + ");
            self.integer(months, m, ADDITIVE + 1);
            self.write(")");
            self.write(suffix);
        }
        if cast {
            self.write(" AS SIGNED)");
        }
        self.write(", ");
        self.input_call(expr, call);
        self.write(")");
    }

    /// `PERIOD_DIFF(p, q)` towards MariaDB: the months of MySQL of a period
    /// beyond 999912, which MariaDB reads as 0, added, `PERIOD_DIFF(p, q) +
    /// IF(p > 999912, p DIV 100 * 12 + p MOD 100 - 1, 0)`, and taken off for
    /// `q`; within `CAST(... AS SIGNED)` for an unsigned argument.
    fn period_diff_mariadb(
        &mut self,
        rule: Rule,
        expr: &Expr,
        call: &FunctionCall,
        values: (Values, Values),
        min: u8,
    ) {
        let both = [values.0, values.1];
        if !self.input_has(Construct::PeriodLongLong) && both.iter().any(|v| v.negative()) {
            self.period_unsigned(rule, expr, call);
            return;
        }
        if !both.iter().any(|v| v.beyond()) {
            self.input_call(expr, call);
            return;
        }
        if !call.args.iter().all(repeatable) {
            self.period_repeated(rule, expr, call);
            return;
        }
        let message = format!(
            "{} reads a period beyond 999912 as 0 months, {} as a year after 9999 (`PERIOD_DIFF(1000001, 202401)` gives -24288, 95712 in MySQL): its months written out",
            self.target(),
            self.source_name()
        );
        self.step(rule.id(), StepKind::Emulation, message, expr.span);
        let parenthesized = ADDITIVE < min;
        if parenthesized {
            self.write("(");
        }
        self.input_call(expr, call);
        for ((arg, values), sign) in call.args.iter().zip(both).zip([" + ", " - "]) {
            if values.beyond() {
                self.write(sign);
                self.write("IF(");
                self.integer(arg, values, PREDICATE + 1);
                self.write(" > 999912, ");
                if values.signed {
                    self.months_of(arg, values);
                } else {
                    self.write("CAST(");
                    self.months_of(arg, values);
                    self.write(" AS SIGNED)");
                }
                self.write(", 0)");
            }
        }
        if parenthesized {
            self.write(")");
        }
    }

    /// `PERIOD_ADD(p, m)` towards MySQL: the period 0, which MySQL refuses,
    /// where MariaDB reads the period or the months otherwise,
    /// `PERIOD_ADD(IF(p NOT BETWEEN 1 AND 999912, p * 0, p), m)`; `p * 0`
    /// keeps NULL.
    fn period_add_mysql(
        &mut self,
        rule: Rule,
        expr: &Expr,
        call: &FunctionCall,
        (p, m): (Values, Values),
    ) {
        let [period, months] = call.args.as_slice() else {
            unreachable!("two arguments")
        };
        let outside = p.beyond() || (!self.has(Construct::PeriodLongLong) && p.negative());
        let wide = m.low < i128::from(i32::MIN) || m.high > i128::from(i32::MAX);
        let fewest = fewest_months(p.low.max(1), p.high.min(MARIADB_MAX));
        let before = fewest.is_some_and(|fewest| m.low < -fewest);
        if (p.literal && outside) || (m.literal && wide) {
            self.period_months(rule, expr, call);
            return;
        }
        if !outside && !wide && !before {
            self.input_call(expr, call);
            return;
        }
        if !repeatable(period) || ((wide || before) && !repeatable(months)) {
            self.period_repeated(rule, expr, call);
            return;
        }
        let message = format!(
            "{} reads a period beyond 999912 as 0 months and adds the months in 32 bits (`PERIOD_ADD(1000001, 1)` gives 200002, 1000002 in MySQL): where {} reads them otherwise, the period 0, which it refuses (error 1210)",
            self.source_name(),
            self.target()
        );
        self.step(rule.id(), StepKind::Emulation, message, expr.span);
        let mut conditions = Vec::new();
        if outside {
            conditions.push(Condition::Outside);
        }
        if wide {
            conditions.push(Condition::Wide);
        }
        if before {
            conditions.push(Condition::Before);
        }
        let form = self.form(&call.name, expr.span);
        self.call_name(&call.name.value, form);
        self.write("IF(");
        for (index, condition) in conditions.into_iter().enumerate() {
            if index > 0 {
                self.write(" OR ");
            }
            match condition {
                Condition::Outside => {
                    self.integer(period, p, PREDICATE + 1);
                    self.write(" NOT BETWEEN 1 AND 999912");
                }
                Condition::Wide => {
                    self.integer(months, m, PREDICATE + 1);
                    self.write(" NOT BETWEEN -2147483648 AND 2147483647");
                }
                Condition::Before => {
                    // The months first: NULL, they leave `PERIOD_DIFF`, which
                    // refuses an invalid period, unread.
                    self.integer(months, m, PREDICATE + 1);
                    match p.value() {
                        Some(value) => {
                            self.write(&format!(" < {}", -months_of_period(i128::from(value))));
                        }
                        None => {
                            self.write(&format!(" < -{BASE_MONTHS} - PERIOD_DIFF("));
                            self.integer(period, p, 0);
                            self.write(&format!(", {BASE_PERIOD})"));
                        }
                    }
                }
            }
        }
        self.write(", ");
        self.integer(period, p, MULTIPLY);
        self.write(" * 0, ");
        self.expr(period, 0);
        self.write("), ");
        self.expr(months, 0);
        self.write(")");
    }

    /// `PERIOD_DIFF(p, q)` towards MySQL: the period 7001 for a period that
    /// MariaDB reads as 0 months, its months taken off,
    /// `PERIOD_DIFF(IF(p NOT BETWEEN 1 AND 999912, 7001, p), q) - IF(p NOT
    /// BETWEEN 1 AND 999912, 23640, 0)`, and added for `q`.
    fn period_diff_mysql(
        &mut self,
        rule: Rule,
        expr: &Expr,
        call: &FunctionCall,
        values: (Values, Values),
        min: u8,
    ) {
        let wide = self.has(Construct::PeriodLongLong);
        let outside = |v: Values| v.beyond() || (!wide && v.negative());
        let both = [values.0, values.1];
        if !both.into_iter().any(outside) {
            self.input_call(expr, call);
            return;
        }
        let substituted: Vec<bool> = both.into_iter().map(outside).collect();
        if call
            .args
            .iter()
            .zip(&substituted)
            .any(|(arg, &substituted)| substituted && !repeatable(arg))
        {
            self.period_repeated(rule, expr, call);
            return;
        }
        let message = format!(
            "{} reads a period beyond 999912 or not positive as 0 months (`PERIOD_DIFF(1000001, 202401)` gives -24288, 95712 in MySQL): the period {BASE_PERIOD}, whose {BASE_MONTHS} months are taken off",
            self.source_name()
        );
        self.step(rule.id(), StepKind::Emulation, message, expr.span);
        let parenthesized = ADDITIVE < min;
        if parenthesized {
            self.write("(");
        }
        let form = self.form(&call.name, expr.span);
        self.call_name(&call.name.value, form);
        for (index, arg) in call.args.iter().enumerate() {
            if index > 0 {
                self.write(", ");
            }
            if substituted[index] {
                self.write("IF(");
                self.integer(arg, both[index], PREDICATE + 1);
                self.write(&format!(" NOT BETWEEN 1 AND 999912, {BASE_PERIOD}, "));
                self.expr(arg, 0);
                self.write(")");
            } else {
                self.expr(arg, 0);
            }
        }
        self.write(")");
        for (index, (arg, sign)) in call.args.iter().zip([" - ", " + "]).enumerate() {
            if substituted[index] {
                self.write(sign);
                self.write("IF(");
                self.integer(arg, both[index], PREDICATE + 1);
                self.write(&format!(" NOT BETWEEN 1 AND 999912, {BASE_MONTHS}, 0)"));
            }
        }
        if parenthesized {
            self.write(")");
        }
    }

    /// Writes `p DIV 100 * 12 + p MOD 100 - 1`, the months of MySQL of the
    /// period `arg`, of the values `values`, beyond 999912.
    fn months_of(&mut self, arg: &Expr, values: Values) {
        self.integer(arg, values, MULTIPLY);
        self.write(" DIV 100 * 12 + ");
        self.integer(arg, values, MULTIPLY);
        self.write(" MOD 100 - 1");
    }

    /// Writes `arg`, of the values `values`, at level `min` at least: a
    /// literal by the integer the servers read from it.
    fn integer(&mut self, arg: &Expr, values: Values, min: u8) {
        match values.value() {
            Some(value) if value < 0 && min > ADDITIVE => self.write(&format!("({value})")),
            Some(value) => self.write(&value.to_string()),
            None => self.expr(arg, min),
        }
    }

    /// Records that MySQL before 8.0.16 reads a negative period as
    /// unsigned.
    fn period_unsigned(&mut self, rule: Rule, expr: &Expr, call: &FunctionCall) {
        let message = format!(
            "{} reads a negative period as an unsigned number (`PERIOD_ADD(-202404, 1)` gives -202315, an error from 8.0.16), {} as 0 months",
            self.source_name(),
            self.target()
        );
        self.error(rule.id(), &message, expr.span);
        self.input_call(expr, call);
    }

    /// Records that the months may go beyond the range both servers read
    /// alike.
    fn period_months(&mut self, rule: Rule, expr: &Expr, call: &FunctionCall) {
        let message = format!(
            "`{}` of months that may take the period before the year 0 or beyond 2^31 - 1 months (`PERIOD_ADD(10001, -1201)`), or of a period beyond 999912: MySQL and MariaDB give values of their own",
            call.name.value.to_ascii_uppercase()
        );
        self.error(rule.id(), &message, expr.span);
        self.input_call(expr, call);
    }

    /// Records that the emulation would repeat an argument that may change
    /// from one evaluation to the next.
    fn period_repeated(&mut self, rule: Rule, expr: &Expr, call: &FunctionCall) {
        let message = format!(
            "`{}` of a period beyond 999912 or of months MariaDB reads otherwise: the emulation repeats the arguments, which may give another value on each evaluation (a subquery, a stored or nondeterministic function)",
            call.name.value.to_ascii_uppercase()
        );
        self.error(rule.id(), &message, expr.span);
        self.input_call(expr, call);
    }
}

/// Condition of `PERIOD_ADD` towards MySQL under which the period becomes
/// 0.
enum Condition {
    /// MariaDB reads 0 months of the period.
    Outside,
    /// The months go beyond 32 bits.
    Wide,
    /// The sum may go before the year 0.
    Before,
}

/// How both servers read `expr`, an argument of `PERIOD_ADD` or
/// `PERIOD_DIFF`.
fn argument(expr: &Expr) -> Argument {
    if let Some(value) = literal(expr) {
        return match value {
            Ok(None) => Argument::Null,
            Ok(Some(value)) if value < i128::from(i64::MIN) || value > i128::from(u64::MAX) => {
                Argument::Refused("a number beyond `BIGINT`")
            }
            Ok(Some(value)) => Argument::Values(Values {
                low: value,
                high: value,
                literal: true,
                signed: true,
            }),
            Err(reason) => Argument::Refused(reason),
        };
    }
    let (low, high, signed) = match &expr.ty.data_type {
        DataType::Null => return Argument::Null,
        DataType::Boolean => (0, 1, true),
        DataType::Integer(integer) => {
            let (low, high) = integer_range(*integer);
            (low, high, !integer.unsigned)
        }
        DataType::Year => (0, 2155, false),
        DataType::Bit { width } => (0, (1_i128 << width) - 1, false),
        DataType::Decimal(decimal) if decimal.scale() == 0 && decimal.precision() <= 18 => {
            let high = 10_i128.pow(u32::from(decimal.precision())) - 1;
            (-high, high, false)
        }
        data_type => return Argument::Refused(refusal(data_type)),
    };
    Argument::Values(Values {
        low,
        high,
        literal: false,
        signed,
    })
}

/// Integer the servers read from a literal `expr`: `Ok(None)` for NULL, an
/// error for a literal they read otherwise; `None` for another expression.
fn literal(expr: &Expr) -> Option<Result<Option<i128>, &'static str>> {
    match &expr.kind {
        ExprKind::Negate(inner) => {
            literal(inner).map(|value| value.map(|value| value.map(|value| -value)))
        }
        ExprKind::Literal(Value::Null) => Some(Ok(None)),
        ExprKind::Literal(Value::Boolean(value)) => Some(Ok(Some(i128::from(*value)))),
        ExprKind::Literal(Value::Integer(value)) => Some(Ok(Some(*value))),
        ExprKind::Literal(Value::Decimal(value)) => {
            Some(round(value.as_str()).map(Some).ok_or("a decimal"))
        }
        ExprKind::Literal(Value::Text(text)) => Some(text.trim().parse().map(Some).map_err(
            |_| "a string other than an integer, which the servers read otherwise (`'2024.01e2'`)",
        )),
        ExprKind::Literal(_) => Some(Err(refusal(&expr.ty.data_type))),
        _ => None,
    }
}

/// Smallest and largest values of `integer`.
fn integer_range(integer: IntegerType) -> (i128, i128) {
    let bits = match integer.size {
        IntegerSize::Tiny => 8,
        IntegerSize::Small => 16,
        IntegerSize::Medium => 24,
        IntegerSize::Int => 32,
        IntegerSize::Big => 64,
    };
    if integer.unsigned {
        (0, (1_i128 << bits) - 1)
    } else {
        (-(1_i128 << (bits - 1)), (1_i128 << (bits - 1)) - 1)
    }
}

/// Why an argument of `data_type` is an error.
fn refusal(data_type: &DataType) -> &'static str {
    match data_type {
        DataType::Date | DataType::DateTime { .. } | DataType::Timestamp { .. } => {
            "a date read as a number (20240506), beyond 999912: a year after 9999 in MySQL, 0 months in MariaDB"
        }
        DataType::Time { .. } => {
            "a time read as a number: MySQL rounds its fraction of a second, MariaDB drops it"
        }
        DataType::Float(_) => {
            "a floating point number, whose fraction the emulation would read unrounded"
        }
        DataType::Decimal(_) => {
            "a decimal with a fraction or of more than 18 digits, which the emulation would read unrounded"
        }
        DataType::Text(_) => "a string read as a number, which the servers read otherwise",
        DataType::Binary(_) | DataType::Hybrid { .. } => "a byte string read as a number",
        DataType::Json => "a `JSON` value, a string in MariaDB",
        _ => "an argument of a type unknown to Prisme",
    }
}

/// Integer a server reads from the literal `value` (`val_int`): a
/// `BIGINT UNSIGNED` from 2^63 as the negative integer of the same bits.
fn reading(value: i128) -> i64 {
    i64::try_from(value).unwrap_or(value as u64 as i64)
}

/// Months of the period `period` (from 1), as MySQL counts them.
fn months_of_period(period: i128) -> i128 {
    let mut year = period / 100;
    if year < i128::from(YY_PART_YEAR) {
        year += 2000;
    } else if year < 100 {
        year += 1900;
    }
    year * 12 + period % 100 - 1
}

/// Fewest months of a period of MySQL from `low` to `high`, of a month
/// from 1 to 12; `None` without one.
fn fewest_months(low: i128, high: i128) -> Option<i128> {
    // The months grow with the period within each run of years: 2000 to
    // 2069, 1970 to 1999, then from the year 100.
    [(1, 6912), (7001, 9912), (10001, i128::MAX)]
        .into_iter()
        .filter_map(|(start, end)| {
            let first = first_valid(low.max(start));
            (first <= high.min(end)).then(|| months_of_period(first))
        })
        .min()
}

/// First period from `period`, positive, of a month from 1 to 12.
fn first_valid(period: i128) -> i128 {
    match period % 100 {
        1..=12 => period,
        0 => period + 1,
        _ => (period / 100 + 1) * 100 + 1,
    }
}

/// `convert_period_to_month` of MySQL and MariaDB, of an unsigned period.
fn period_to_month(period: u64) -> u64 {
    if period == 0 {
        return 0;
    }
    let mut year = period / 100;
    if year < YY_PART_YEAR {
        year += 2000;
    } else if year < 100 {
        year += 1900;
    }
    year.wrapping_mul(12)
        .wrapping_add(period % 100)
        .wrapping_sub(1)
}

/// `convert_month_to_period` of MySQL and MariaDB.
fn month_to_period(month: u64) -> u64 {
    if month == 0 {
        return 0;
    }
    let mut year = month / 12;
    if year < 100 {
        year += if year < YY_PART_YEAR { 2000 } else { 1900 };
    }
    year.wrapping_mul(100)
        .wrapping_add(month % 12)
        .wrapping_add(1)
}

/// `PERIOD_ADD` (`add`) or `PERIOD_DIFF` of `values` in MySQL, from 8.0.16
/// when `wide`; `None` for the error 1210.
fn mysql_period(add: bool, wide: bool, (first, second): (i64, i64)) -> Option<i64> {
    // A period read as signed from 8.0.16, as unsigned before.
    let valid = |period: i64| {
        let month = if wide {
            (period > 0).then_some(period % 100)
        } else {
            (period != 0).then_some((period as u64 % 100) as i64)
        };
        month.is_some_and(|month| (1..=12).contains(&month))
    };
    if !valid(first) || (!add && !valid(second)) {
        return None;
    }
    let months = period_to_month(first as u64);
    if add {
        // The months of 64 bits from 8.0.16, of 32 bits before.
        let added = if wide {
            second
        } else {
            i64::from(second as i32)
        };
        return Some(month_to_period(months.wrapping_add(added as u64)) as i64);
    }
    Some((months as i64).wrapping_sub(period_to_month(second as u64) as i64))
}

/// `PERIOD_ADD` (`add`) or `PERIOD_DIFF` of `values` in MariaDB: a period
/// beyond 999912 counts 0 months, the months are added in 32 bits.
fn mariadb_period(add: bool, (first, second): (i64, i64)) -> i64 {
    let months = |period: i64| match period as u64 {
        period if period > MARIADB_MAX as u64 => 0,
        period => period_to_month(period),
    };
    if !add {
        return (months(first) as i64).wrapping_sub(months(second) as i64);
    }
    if first == 0 {
        return 0;
    }
    let sum = (months(first) as i32).wrapping_add(second as i32) as u32;
    month_to_period(u64::from(sum)) as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn computes_the_periods_as_the_servers() {
        // Measured on MySQL 8.0.11, 8.0.15, 8.0.16 and 8.4, MariaDB 10.6
        // and 11.8 (`crates/probe/cases/period.txt`, `period-forms.sql`).
        // `PERIOD_ADD` or `PERIOD_DIFF`, its arguments, MySQL before and
        // from 8.0.16 (`None` for an error), MariaDB.
        type Case = (bool, (i64, i64), Option<i64>, Option<i64>, i64);
        let cases: [Case; 14] = [
            (true, (202401, 1), Some(202402), Some(202402), 202402),
            (true, (1000001, 1), Some(1000002), Some(1000002), 200002),
            (true, (202413, 1), None, None, 202502),
            (true, (0, 1), None, None, 0),
            (true, (-202404, 1), Some(-202315), None, 200002),
            (
                true,
                (202401, 4294967296),
                Some(202401),
                Some(35791596505),
                202401,
            ),
            (
                true,
                (202401, 2147483648),
                Some(6148914673341022481),
                Some(17895899409),
                17895899409,
            ),
            (
                true,
                (202401, -30000),
                Some(6148914691236469577),
                Some(6148914691236469577),
                35791346505,
            ),
            (true, (10001, -1), Some(199912), Some(199912), 199912),
            (false, (202401, 202301), Some(12), Some(12), 12),
            (false, (1000001, 202401), Some(95712), Some(95712), -24288),
            (false, (202413, 202401), None, None, 12),
            (
                false,
                (-202404, 202401),
                Some(2213609288845097627),
                None,
                -24288,
            ),
            (false, (999912, 1), Some(95999), Some(95999), 95999),
        ];
        for (add, values, before, from, mariadb) in cases {
            assert_eq!(mysql_period(add, false, values), before, "{add} {values:?}");
            assert_eq!(mysql_period(add, true, values), from, "{add} {values:?}");
            assert_eq!(mariadb_period(add, values), mariadb, "{add} {values:?}");
        }
    }

    #[test]
    fn counts_the_fewest_months_of_the_periods() {
        assert_eq!(fewest_months(202401, 202401), Some(24288));
        assert_eq!(
            fewest_months(i128::from(i32::MIN), i128::from(i32::MAX)),
            Some(1200)
        );
        assert_eq!(fewest_months(1, 9999), Some(23640));
        assert_eq!(fewest_months(1, 255), Some(24000));
        assert_eq!(fewest_months(13, 99), None);
        assert_eq!(fewest_months(-5, 0), None);
    }
}
