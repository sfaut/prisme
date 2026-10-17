//! Rules of Prisme that write a built-in function of the input in the
//! output dialect, named by the reference files (`builtins` module): the
//! function of the output when the arguments allow it, an emulation that
//! keeps the result, or an explicit error. Each rule was measured on MySQL
//! 8.0.11 and 8.4, MariaDB 10.6 and 11.8.

use prisme_plan::{Expr, ExprKind, FunctionCall};
use prisme_types::{DataType, IntegerSize, IntegerType, MAX_FRACTION_DIGITS, Value};

use super::Emitter;
use super::builtin::NameForm;
use super::expr::MULTIPLY;
use crate::Dialect;
use crate::builtins::Rule;
use crate::report::StepKind;
use crate::versions::Construct;

/// Last second of the range of `UNIX_TIMESTAMP` and `FROM_UNIXTIME`,
/// 2038-01-19 03:14:07 UTC, before MySQL 8.0.28 and MariaDB 11.5.1.
const UNIXTIME_2038: i64 = 2_147_483_647;
/// Last second of the range in MariaDB from 11.5.1, 2106-02-07 06:28:15 UTC.
const UNIXTIME_2106: i64 = 4_294_967_295;
/// Last second of the range in MySQL from 8.0.28, 3001-01-18 23:59:59 UTC.
const UNIXTIME_3001: i64 = 32_536_771_199;

/// The epoch in the time zone of the session, in MariaDB from 11.7.1 where
/// `FROM_UNIXTIME(0)` gives NULL.
const EPOCH: &str = "FROM_UNIXTIME(1) - INTERVAL 1 SECOND";

/// Functions that may give another value on each call in a statement, or
/// that act: an argument that calls them is never written twice.
const VOLATILE: [&str; 17] = [
    "BENCHMARK",
    "GET_LOCK",
    "LASTVAL",
    "LAST_INSERT_ID",
    "NEXTVAL",
    "RAND",
    "RANDOM_BYTES",
    "RELEASE_ALL_LOCKS",
    "RELEASE_LOCK",
    "SETVAL",
    "SLEEP",
    "SYSDATE",
    "SYS_GUID",
    "UUID",
    "UUID_SHORT",
    "UUID_V4",
    "UUID_V7",
];

/// Range of `UNIX_TIMESTAMP` and `FROM_UNIXTIME` in a version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Unixtime {
    /// Last second of the range, its fraction included.
    max: i64,
    /// The epoch is in the range.
    epoch: bool,
    /// Out of the range, `UNIX_TIMESTAMP` gives 0 (MySQL); NULL otherwise.
    zero: bool,
}

impl Unixtime {
    /// Range of `dialect`, which has the constructs `has` accepts.
    fn of(dialect: Dialect, has: impl Fn(Construct) -> bool) -> Self {
        let max = if has(Construct::Unixtime3001) {
            UNIXTIME_3001
        } else if has(Construct::Unixtime2106) {
            UNIXTIME_2106
        } else {
            UNIXTIME_2038
        };
        Self {
            max,
            epoch: !has(Construct::UnixtimeEpochNull),
            zero: dialect == Dialect::MySql,
        }
    }

    /// Bounds of the range, for the messages.
    fn bounds(self) -> String {
        let from = if self.epoch { "from" } else { "after" };
        format!("{from} the epoch to {} UTC", date(self.max))
    }

    /// Value of `UNIX_TIMESTAMP` out of the range, for the messages.
    const fn outside(self) -> &'static str {
        if self.zero { "0" } else { "NULL" }
    }
}

impl Emitter<'_> {
    /// Writes the call `expr` of the built-in function `call` by `rule`, at
    /// level `min` at least, read as a value of its type when `column`
    /// (see `Emitter::column`).
    pub(super) fn builtin_rule(
        &mut self,
        rule: Rule,
        expr: &Expr,
        call: &FunctionCall,
        min: u8,
        column: bool,
    ) {
        match rule {
            Rule::Aes => self.aes(rule, expr, call),
            Rule::BitCount => self.bit_count(rule, expr, call),
            Rule::FromUnixtime => self.date_from_unixtime(rule, expr, call),
            Rule::Greatest => self.greatest(rule, expr, call, column),
            Rule::Now => self.now(rule, expr, call),
            Rule::Pad => self.pad(rule, expr, call),
            Rule::TimeToSec => self.time_to_sec(rule, expr, call, min),
            Rule::UnixTimestamp => self.unix_timestamp(rule, expr, call),
            Rule::Uuid => self.uuid(rule, expr, call),
            _ => {
                self.rule_not_written(rule, &call.name, expr.span);
                self.input_call(expr, call);
            }
        }
    }

    /// `TIME_TO_SEC`: MySQL drops the fraction of the seconds, MariaDB keeps
    /// it (`TIME_TO_SEC('00:39:38.5')` gives 2378, 2378.5 in MariaDB).
    /// Towards MariaDB, `DIV 1` drops it; towards MySQL, a value with a
    /// fraction is an error.
    fn time_to_sec(&mut self, rule: Rule, expr: &Expr, call: &FunctionCall, min: u8) {
        let whole = match call.args.as_slice() {
            [arg] => whole_seconds(&arg.ty.data_type),
            // The server reports the number of arguments.
            _ => true,
        };
        if whole {
            self.input_call(expr, call);
            return;
        }
        match self.output.dialect {
            Dialect::MariaDb => {
                let message = format!(
                    "{} keeps the fraction of the seconds in `TIME_TO_SEC`: `DIV 1` drops it, as MySQL does",
                    self.target()
                );
                self.step(rule.id(), StepKind::Emulation, message, expr.span);
                let parenthesized = MULTIPLY < min;
                if parenthesized {
                    self.write("(");
                }
                self.input_call(expr, call);
                self.write(" DIV 1");
                if parenthesized {
                    self.write(")");
                }
            }
            Dialect::MySql => {
                let message = format!(
                    "`TIME_TO_SEC` of a value that may have a fraction of seconds: {} drops it, MariaDB keeps it",
                    self.target()
                );
                self.error(rule.id(), &message, expr.span);
                self.input_call(expr, call);
            }
        }
    }

    /// `NOW`, `CURRENT_TIMESTAMP`, `SYSDATE` and `LOCALTIME`: the date and
    /// time of a `DATETIME` in MySQL. In MariaDB, the first three give a
    /// `TIMESTAMP` from 11.7.1, of the same value in the time zone of the
    /// session, and `LOCALTIME` the time of a `TIME` from 11.6.2. Towards
    /// those versions, `NOW`, `CURRENT_TIMESTAMP` and `LOCALTIME` become
    /// `LOCALTIMESTAMP`, the same instant of the statement as a `DATETIME`,
    /// and `SYSDATE` a `DATETIME` by `CAST`. From MariaDB, `LOCALTIME` of
    /// those versions becomes `CURRENT_TIME`.
    fn now(&mut self, rule: Rule, expr: &Expr, call: &FunctionCall) {
        let upper = call.name.value.to_ascii_uppercase();
        let localtime = upper == "LOCALTIME";
        let target = self.target();
        // A call written without parentheses keeps that form.
        let form = match self.form(&call.name, expr.span) {
            NameForm::Bare => NameForm::Bare,
            _ => NameForm::Plain,
        };
        if self.output.dialect == Dialect::MySql {
            if localtime && self.input_has(Construct::TimeLocaltime) {
                let message = format!(
                    "`LOCALTIME` of MariaDB is the time of day: `CURRENT_TIME` in {target}"
                );
                self.step(rule.id(), StepKind::Rewrite, message, expr.span);
                self.call_text("CURRENT_TIME", form, &call.args);
            } else {
                self.input_call(expr, call);
            }
            return;
        }
        let construct = if localtime {
            Construct::TimeLocaltime
        } else {
            Construct::TimestampNow
        };
        if !self.has(construct) {
            self.input_call(expr, call);
            return;
        }
        if upper != "SYSDATE" {
            let message = format!(
                "{target} gives `{upper}` another type: `LOCALTIMESTAMP` gives the same instant as a `DATETIME`"
            );
            self.step(rule.id(), StepKind::Rewrite, message, expr.span);
            self.call_text("LOCALTIMESTAMP", form, &call.args);
            return;
        }
        let digits = match call.args.as_slice() {
            [] => Some(String::new()),
            [arg] => match &arg.kind {
                ExprKind::Literal(Value::Integer(digits)) => Some(format!("({digits})")),
                _ => None,
            },
            _ => None,
        };
        let Some(digits) = digits else {
            let message = format!(
                "`SYSDATE` with digits that are not an integer: {target} gives a `TIMESTAMP`, cast to a `DATETIME` of these digits"
            );
            self.error(rule.id(), &message, expr.span);
            self.input_call(expr, call);
            return;
        };
        let message = format!(
            "{target} gives `SYSDATE` the type `TIMESTAMP`: cast to `DATETIME`, of the same value"
        );
        self.step(rule.id(), StepKind::Emulation, message, expr.span);
        self.write("CAST(");
        self.input_call(expr, call);
        self.write(&format!(" AS DATETIME{digits})"));
    }

    /// `LPAD` and `RPAD`: MariaDB pads with spaces without a pad, MySQL
    /// needs it. With an empty pad, beyond the length of the string, they
    /// give NULL in MariaDB and MySQL before 8.0.13, an empty string in MySQL
    /// from 8.0.13, the string unchanged from 9.3.0. A literal pad other than
    /// the empty string gives the same result everywhere; another pad is an
    /// error when the two servers differ.
    fn pad(&mut self, rule: Rule, expr: &Expr, call: &FunctionCall) {
        let upper = call.name.value.to_ascii_uppercase();
        let target = self.target();
        let pad = match call.args.as_slice() {
            [_, _] if self.output.dialect == Dialect::MySql => {
                let message = format!(
                    "`{upper}` without a pad pads with spaces: {target} needs the pad `' '`"
                );
                self.step(rule.id(), StepKind::Rewrite, message, expr.span);
                let form = self.form(&call.name, expr.span);
                self.call_name(&call.name.value, form);
                self.expr_list(&call.args);
                self.write(", ' ')");
                return;
            }
            [_, _, pad] => pad,
            // The server reports the number of arguments.
            _ => {
                self.input_call(expr, call);
                return;
            }
        };
        let input = empty_pad(
            self.input_has(Construct::PadEmptyString),
            self.input_has(Construct::PadUnchanged),
        );
        let output = empty_pad(
            self.has(Construct::PadEmptyString),
            self.has(Construct::PadUnchanged),
        );
        let literal = matches!(&pad.kind, ExprKind::Literal(value) if !empty(value));
        if input != output && !literal {
            let message = format!(
                "`{upper}` with a pad that may be empty: beyond the length of the string, {} {} gives {input}, {target} gives {output}",
                self.source.dialect.label(),
                self.source.version
            );
            self.error(rule.id(), &message, expr.span);
        }
        self.input_call(expr, call);
    }

    /// `UUID()`: a `VARCHAR(36)` in MySQL, a value of type `UUID` in MariaDB
    /// from 10.7.1, of the same text, which compares only with UUIDs. Towards
    /// those versions, a `CAST` to `CHAR(36)` keeps the string of MySQL;
    /// towards MySQL, the string of the same text.
    fn uuid(&mut self, rule: Rule, expr: &Expr, call: &FunctionCall) {
        if self.output.dialect == Dialect::MySql || !self.has(Construct::UuidType) {
            self.input_call(expr, call);
            return;
        }
        let message = format!(
            "{} gives `UUID()` the type `UUID`: cast to `CHAR(36)`, the string of the same text",
            self.target()
        );
        self.step(rule.id(), StepKind::Emulation, message, expr.span);
        self.write("CAST(");
        self.input_call(expr, call);
        self.write(" AS CHAR(36))");
    }

    /// `UNIX_TIMESTAMP(x)`: the seconds from the epoch to `x` within a range,
    /// until 2038-01-19 03:14:07 UTC, 3001-01-18 23:59:59 in MySQL from
    /// 8.0.28, 2106-02-07 06:28:15 in MariaDB from 11.5.1, the epoch left out
    /// in MariaDB from 11.7.1; out of it, 0 in MySQL, NULL in MariaDB. A
    /// `CASE` on the result of the output keeps that of the input; beyond the
    /// range of the output, an error, unless the type or the value of `x`
    /// keep it within (a `TIMESTAMP`, a literal date from 1971 to 2037).
    fn unix_timestamp(&mut self, rule: Rule, expr: &Expr, call: &FunctionCall) {
        let (input, output) = self.unixtime_ranges();
        // Without argument, the current time, within every range; the
        // server reports another number of arguments.
        let [arg] = call.args.as_slice() else {
            self.input_call(expr, call);
            return;
        };
        let timestamp_max = match self.output.dialect {
            Dialect::MySql => UNIXTIME_2038,
            Dialect::MariaDb => output.max,
        };
        let bound = date_bound(arg, timestamp_max);
        if input == output || bound.is_some_and(|max| max <= input.max.min(output.max)) {
            self.input_call(expr, call);
            return;
        }
        let top = bound.unwrap_or(i64::MAX);
        let (source, target) = (self.source_name(), self.target());
        if top > output.max && input.max > output.max {
            let message = format!(
                "`UNIX_TIMESTAMP` of a date that may be after {} UTC: {source} gives its seconds until {} UTC, {target} gives {}",
                date(output.max),
                date(input.max),
                output.outside()
            );
            self.error(rule.id(), &message, expr.span);
            self.input_call(expr, call);
            return;
        }
        let cap = (top > input.max && output.max > input.max).then_some(input.max + 1);
        // MariaDB gives NULL out of its range; MySQL 0, as at the epoch and
        // in the second after it, its fraction dropped.
        let towards_mysql = output.zero;
        // A literal string gives its fractional digits to the result.
        let digits = match &arg.kind {
            ExprKind::Literal(Value::Text(text)) => Some(literal_digits(text)),
            _ => None,
        };
        let whole = whole_seconds(&arg.ty.data_type) || digits == Some(0);
        let if_null = !towards_mysql && cap.is_none() && whole;
        let ranges = format!(
            "`UNIX_TIMESTAMP` gives the seconds {} in {source}, {} otherwise, {} in {target}, {} otherwise",
            input.bounds(),
            input.outside(),
            output.bounds(),
            output.outside()
        );
        let repeats = !if_null || arg.ty.nullable;
        if repeats && !repeatable(arg) {
            self.error_repeated(rule, &ranges, expr);
            self.input_call(expr, call);
            return;
        }
        let message = format!("{ranges}: emulated to keep the result of {source}");
        self.step(rule.id(), StepKind::Emulation, message, expr.span);
        if if_null {
            // Towards MariaDB, of the same range: 0 for a date out of it.
            if arg.ty.nullable {
                self.write("IF(");
                self.expr(arg, 6);
                self.write(" IS NULL, NULL, ");
            }
            self.write("IFNULL(");
            self.input_call(expr, call);
            self.write(", 0)");
            if arg.ty.nullable {
                self.write(")");
            }
            return;
        }
        // The result of the output when it is in the range of the input.
        let mut conditions = Vec::new();
        if towards_mysql {
            conditions.push("> 0".to_string());
        } else if !whole {
            conditions.push(">= 1".to_string());
        }
        if let Some(cap) = cap {
            conditions.push(format!("< {cap}"));
        }
        self.write("CASE WHEN ");
        for (index, condition) in conditions.iter().enumerate() {
            if index > 0 {
                self.write(" AND ");
            }
            self.input_call(expr, call);
            self.write(&format!(" {condition}"));
        }
        self.write(" THEN ");
        self.input_call(expr, call);
        if towards_mysql && !whole {
            // The second after the epoch, which MariaDB gives with its
            // fraction, one second later.
            self.write(" WHEN ");
            self.expr(arg, 6);
            self.write(if input.epoch { " >= " } else { " > " });
            self.write("FROM_UNIXTIME(0) AND ");
            self.expr(arg, 6);
            self.write(" < FROM_UNIXTIME(1) THEN ");
            // The sum gives 6 fractional digits to a string: the digits of
            // a literal come back by `TRUNCATE`.
            let truncate = digits.filter(|digits| *digits < MAX_FRACTION_DIGITS);
            if truncate.is_some() {
                self.write("TRUNCATE(");
            }
            let form = self.form(&call.name, expr.span);
            self.call_name(&call.name.value, form);
            self.expr(arg, 9);
            self.write(" + INTERVAL 1 SECOND) - 1");
            if let Some(digits) = truncate {
                self.write(&format!(", {digits})"));
            }
        } else if towards_mysql && input.epoch {
            // 0 at the epoch, NULL out of the range of MariaDB.
            self.write(" WHEN ");
            self.expr(arg, 6);
            self.write(" = FROM_UNIXTIME(0) THEN 0");
        } else if !towards_mysql && arg.ty.nullable {
            // 0 out of the range of MySQL.
            self.write(" WHEN ");
            self.expr(arg, 6);
            self.write(" IS NOT NULL THEN 0");
        } else if !towards_mysql {
            self.write(" ELSE 0");
        }
        self.write(" END");
    }

    /// `FROM_UNIXTIME(n)` and `FROM_UNIXTIME(n, format)`: the date and time
    /// `n` seconds after the epoch, within the range of `UNIX_TIMESTAMP`,
    /// NULL out of it; a `TIMESTAMP` in MariaDB from 11.7.1, a `DATETIME`
    /// otherwise. Towards a wider range, `IF(n < cap, n, NULL)`; towards a
    /// range without the epoch, the epoch rebuilt from the next second;
    /// towards a range with it, `NULLIF(n, 0)`; towards a `TIMESTAMP`, a
    /// `CAST` to the `DATETIME` of as many fractional digits. Beyond the
    /// range of the output, an error, unless the type or the value of `n`
    /// keep it within.
    fn date_from_unixtime(&mut self, rule: Rule, expr: &Expr, call: &FunctionCall) {
        let (input, output) = self.unixtime_ranges();
        let (n, format) = match call.args.as_slice() {
            [n] => (n, None),
            [n, format] => (n, Some(format)),
            // The server reports the number of arguments.
            _ => {
                self.input_call(expr, call);
                return;
            }
        };
        let literal = match &n.kind {
            ExprKind::Literal(value) => number(value),
            _ => None,
        };
        let top = literal.map_or_else(
            || seconds_bound(&n.ty.data_type),
            |value| Some(value.floor() as i64),
        );
        let may_be_zero = literal.is_none_or(|value| value == 0.0);
        let cast = format.is_none()
            && self.has(Construct::TimestampNow)
            && !self.input_has(Construct::TimestampNow);
        let (source, target) = (self.source_name(), self.target());
        if input.max > output.max && top.is_none_or(|top| top > output.max) {
            let message = format!(
                "`FROM_UNIXTIME` of a number that may exceed {}: {source} gives a date until {} UTC, {target} gives NULL",
                output.max,
                date(input.max)
            );
            self.error(rule.id(), &message, expr.span);
            self.input_call(expr, call);
            return;
        }
        let cap = (output.max > input.max && top.is_none_or(|top| top > input.max))
            .then_some(input.max + 1);
        let null_zero = may_be_zero && !input.epoch && output.epoch;
        let epoch = may_be_zero && input.epoch && !output.epoch;
        if cap.is_none() && !null_zero && !epoch && !cast {
            self.input_call(expr, call);
            return;
        }
        let mut reasons = Vec::new();
        if cap.is_some() || null_zero || epoch {
            reasons.push(format!(
                "`FROM_UNIXTIME` gives a date {} in {source}, {} in {target}, NULL otherwise",
                input.bounds(),
                output.bounds()
            ));
        }
        let digits = match cast.then(|| fraction_digits(&n.ty.data_type)) {
            Some(None) => {
                let message = format!(
                    "{target} gives `FROM_UNIXTIME` the type `TIMESTAMP`: the cast to a `DATETIME` needs the fractional digits of the argument, whose type is unknown"
                );
                self.error(rule.id(), &message, expr.span);
                self.input_call(expr, call);
                return;
            }
            Some(Some(digits)) => {
                reasons.push(format!(
                    "{target} gives `FROM_UNIXTIME` the type `TIMESTAMP`: cast to a `DATETIME`, of the same value"
                ));
                Some(digits)
            }
            None => None,
        };
        let reasons = reasons.join("; ");
        // Only the epoch: written alone.
        let only_epoch = epoch && literal == Some(0.0);
        // `NULLIF` evaluates its first argument twice when it differs from
        // the second.
        let repeated = (cap.is_some() || epoch || null_zero) && !only_epoch;
        if (repeated && !repeatable(n))
            || (epoch && !only_epoch && format.is_some_and(|format| !repeatable(format)))
        {
            self.error_repeated(rule, &reasons, expr);
            self.input_call(expr, call);
            return;
        }
        self.step(rule.id(), StepKind::Emulation, reasons, expr.span);
        if cast {
            self.write("CAST(");
        }
        if epoch {
            if !only_epoch {
                self.write("IF(");
                self.expr(n, 6);
                self.write(" = 0, ");
            }
            match format {
                Some(format) => {
                    self.write(&format!("DATE_FORMAT({EPOCH}, "));
                    self.expr(format, 0);
                    self.write(")");
                }
                None => self.write(EPOCH),
            }
            if !only_epoch {
                self.write(", ");
            }
        }
        if !only_epoch {
            let form = self.form(&call.name, expr.span);
            self.call_name(&call.name.value, form);
            if null_zero {
                self.write("NULLIF(");
            }
            match cap {
                Some(cap) => {
                    self.write("IF(");
                    self.expr(n, 6);
                    self.write(&format!(" < {cap}, "));
                    self.expr(n, 0);
                    self.write(", NULL)");
                }
                None => self.expr(n, 0),
            }
            if null_zero {
                self.write(", 0)");
            }
            if let Some(format) = format {
                self.write(", ");
                self.expr(format, 0);
            }
            self.write(")");
        }
        if epoch && !only_epoch {
            self.write(")");
        }
        if let Some(digits) = digits {
            match digits {
                0 => self.write(" AS DATETIME)"),
                _ => self.write(&format!(" AS DATETIME({digits}))")),
            }
        }
    }

    /// Ranges of `UNIX_TIMESTAMP` and `FROM_UNIXTIME` in the input and the
    /// output.
    fn unixtime_ranges(&self) -> (Unixtime, Unixtime) {
        (
            Unixtime::of(self.source.dialect, |construct| self.input_has(construct)),
            Unixtime::of(self.output.dialect, |construct| self.has(construct)),
        )
    }

    /// Records that the emulation `ranges` explains would repeat an argument
    /// that may change from one evaluation to the next.
    fn error_repeated(&mut self, rule: Rule, ranges: &str, expr: &Expr) {
        let message = format!(
            "{ranges}: the emulation repeats the argument, which may give another value on each evaluation (a subquery, a stored or nondeterministic function)"
        );
        self.error(rule.id(), &message, expr.span);
    }

    /// Name of the input dialect with its version, for the messages.
    pub(super) fn source_name(&self) -> String {
        format!("{} {}", self.source.dialect.label(), self.source.version)
    }

    /// Writes the call `call` with its name and form in the input.
    pub(super) fn input_call(&mut self, expr: &Expr, call: &FunctionCall) {
        let form = self.form(&call.name, expr.span);
        self.call_text(&call.name.value, form, &call.args);
    }
}

/// Result of `LPAD` and `RPAD` with an empty pad, beyond the length of the
/// string, from the behaviours of the server.
fn empty_pad(empty_string: bool, unchanged: bool) -> &'static str {
    if unchanged {
        "the string unchanged"
    } else if empty_string {
        "an empty string"
    } else {
        "NULL"
    }
}

/// Tells whether `value` is an empty string.
fn empty(value: &Value) -> bool {
    match value {
        Value::Text(text) => text.is_empty(),
        Value::Binary(bytes) => bytes.is_empty(),
        _ => false,
    }
}

/// Date of the last second of a range of `UNIX_TIMESTAMP`.
fn date(max: i64) -> &'static str {
    match max {
        UNIXTIME_3001 => "3001-01-18 23:59:59",
        UNIXTIME_2106 => "2106-02-07 06:28:15",
        _ => "2038-01-19 03:14:07",
    }
}

/// Last second from the epoch that the date `arg` can reach, when it stays
/// after the epoch (a zero `TIMESTAMP` aside, which gives 0 everywhere): a
/// `TIMESTAMP` until `timestamp_max`, a valid literal date from 1971 to
/// 2037 whatever the time zone; `None` when unknown. NULL gives NULL.
fn date_bound(arg: &Expr, timestamp_max: i64) -> Option<i64> {
    match &arg.kind {
        ExprKind::Literal(Value::Null) => Some(0),
        ExprKind::Literal(Value::Text(text) | Value::Date(text) | Value::DateTime(text)) => {
            literal_year(text)
                .filter(|year| (1971..=2037).contains(year))
                .map(|_| UNIXTIME_2038)
        }
        _ if matches!(arg.ty.data_type, DataType::Timestamp { .. }) => Some(timestamp_max),
        _ => None,
    }
}

/// Year of a valid date literal, `YYYY-MM-DD` followed or not by
/// `hh:mm:ss` and up to 6 fractional digits; `None` for another form,
/// which the servers may read differently.
pub(super) fn literal_year(text: &str) -> Option<u32> {
    let (date, time) = text.split_once(' ').unwrap_or((text, ""));
    let field = |part: &str, len: usize, max: u32| {
        (part.len() == len && part.bytes().all(|byte| byte.is_ascii_digit()))
            .then(|| part.parse::<u32>().ok())
            .flatten()
            .filter(|value| *value <= max)
    };
    let mut parts = date.split('-');
    let year = field(parts.next()?, 4, 9999)?;
    let month = field(parts.next()?, 2, 12).filter(|month| *month > 0)?;
    let day = field(parts.next()?, 2, 31).filter(|day| *day > 0)?;
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = match month {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    if parts.next().is_some() || day > days {
        return None;
    }
    if !time.is_empty() {
        let (clock, fraction) = time.split_once('.').unwrap_or((time, "0"));
        let mut parts = clock.split(':');
        field(parts.next()?, 2, 23)?;
        field(parts.next()?, 2, 59)?;
        field(parts.next()?, 2, 59)?;
        let digits = fraction.len() <= usize::from(MAX_FRACTION_DIGITS)
            && fraction.bytes().all(|byte| byte.is_ascii_digit());
        if parts.next().is_some() || fraction.is_empty() || !digits {
            return None;
        }
    }
    Some(year)
}

/// Fractional digits of a date written as a string, those after its last
/// point, up to `MAX_FRACTION_DIGITS`.
pub(super) fn literal_digits(text: &str) -> u8 {
    let fraction = text.rsplit_once('.').map_or("", |(_, fraction)| fraction);
    let count = fraction.bytes().take_while(u8::is_ascii_digit).count();
    u8::try_from(count).map_or(MAX_FRACTION_DIGITS, |count| count.min(MAX_FRACTION_DIGITS))
}

/// Largest whole number of seconds a value of `data_type` can hold: an
/// integer up to 4 bytes, a decimal of few integer digits; `None` when
/// unknown.
fn seconds_bound(data_type: &DataType) -> Option<i64> {
    match data_type {
        DataType::Boolean => Some(1),
        DataType::Integer(IntegerType {
            size: IntegerSize::Big,
            ..
        }) => None,
        DataType::Integer(IntegerType {
            size: IntegerSize::Int,
            unsigned: true,
        }) => Some(i64::from(u32::MAX)),
        DataType::Integer(_) => Some(i64::from(i32::MAX)),
        DataType::Decimal(decimal) => {
            let digits = u32::from(decimal.precision() - decimal.scale());
            10_i64.checked_pow(digits).map(|power| power - 1)
        }
        _ => None,
    }
}

/// Number of a numeric literal; `None` for another value.
pub(super) fn number(value: &Value) -> Option<f64> {
    match value {
        Value::Boolean(value) => Some(f64::from(u8::from(*value))),
        Value::Integer(value) => Some(*value as f64),
        Value::Decimal(value) => value.as_str().parse().ok(),
        Value::Float(value) => Some(*value),
        _ => None,
    }
}

/// Fractional digits of `FROM_UNIXTIME` of a value of `data_type`, measured
/// on MySQL 8.0.11 and 8.4, MariaDB 10.6 and 11.8; `None` when the type is
/// unknown.
fn fraction_digits(data_type: &DataType) -> Option<u8> {
    match data_type {
        DataType::Null
        | DataType::Boolean
        | DataType::Integer(_)
        | DataType::Date
        | DataType::Year
        | DataType::Bit { .. } => Some(0),
        DataType::Decimal(decimal) => Some(decimal.scale().min(MAX_FRACTION_DIGITS)),
        DataType::Time { fraction }
        | DataType::DateTime { fraction }
        | DataType::Timestamp { fraction } => Some(*fraction),
        DataType::Float(_) | DataType::Text(_) | DataType::Binary(_) | DataType::Json => {
            Some(MAX_FRACTION_DIGITS)
        }
        _ => None,
    }
}

/// Tells whether `expr` gives the same value each time it is written in a
/// statement: without subquery, stored function nor function of
/// `VOLATILE`.
fn repeatable(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Column(_) | ExprKind::Literal(_) | ExprKind::Variable(_) => true,
        ExprKind::Not(operand)
        | ExprKind::Negate(operand)
        | ExprKind::BitNot(operand)
        | ExprKind::Cast(operand)
        | ExprKind::ToBoolean(operand)
        | ExprKind::ToInteger(operand)
        | ExprKind::IsNull { operand, .. } => repeatable(operand),
        ExprKind::Compare { left, right, .. }
        | ExprKind::NotDistinct { left, right, .. }
        | ExprKind::Logical { left, right, .. }
        | ExprKind::Arithmetic { left, right, .. }
        | ExprKind::Bitwise { left, right, .. } => repeatable(left) && repeatable(right),
        ExprKind::Between {
            operand, low, high, ..
        } => repeatable(operand) && repeatable(low) && repeatable(high),
        ExprKind::Like {
            operand,
            pattern,
            escape,
            ..
        } => repeatable(operand) && repeatable(pattern) && escape.as_deref().is_none_or(repeatable),
        ExprKind::InList { operand, list, .. } => {
            repeatable(operand) && list.iter().all(repeatable)
        }
        ExprKind::Row(values) => values.iter().all(repeatable),
        ExprKind::Case(case) => {
            case.operand.as_deref().is_none_or(repeatable)
                && case
                    .branches
                    .iter()
                    .all(|branch| repeatable(&branch.when) && repeatable(&branch.then))
                && case.otherwise.as_deref().is_none_or(repeatable)
        }
        ExprKind::Function(call) => {
            !call.stored
                && !VOLATILE.contains(&call.name.value.to_ascii_uppercase().as_str())
                && call.args.iter().all(repeatable)
        }
        _ => false,
    }
}

/// Tells whether a value of `data_type` is a whole number of seconds.
fn whole_seconds(data_type: &DataType) -> bool {
    matches!(
        data_type,
        DataType::Null
            | DataType::Integer(_)
            | DataType::Date
            | DataType::Year
            | DataType::Time { fraction: 0 }
            | DataType::DateTime { fraction: 0 }
            | DataType::Timestamp { fraction: 0 }
    )
}
