//! `CASE` between MySQL and MariaDB, measured on MySQL 8.0.11 and 8.4.11,
//! MariaDB 10.6.28 and 11.8.9 (`crates/probe/cases/case*`). Both merge the
//! families of the results alike, but:
//!
//! - MariaDB gives a `DECIMAL` to a signed integer with an unsigned one, or
//!   to an unsigned integer with a `NULL` literal, where MySQL gives an
//!   integer: the values are the same, a cast keeps the family of the input,
//!   `SIGNED` or `UNSIGNED` towards MariaDB, `DECIMAL(n, 0)` towards MySQL
//!   (rule `case.type`);
//! - a `CASE` of type `DECIMAL` gives in MySQL each value at the scale of its
//!   result when it is written as a string (`42` for an integer, `42.00` in
//!   MariaDB), by the protocol or by a string function, `LIKE`...; read as a
//!   number, it is the same: towards MySQL, the results of a smaller scale
//!   take the scale of the type; towards MariaDB, a column of the result
//!   keeps its values, an expression that may read them as strings is an
//!   error (rule `case.scale`).
//!
//! A result of unknown type (a function, a system variable) hides these
//! differences: an error.

use prisme_plan::{Case, Expr, ExprKind};
use prisme_types::{DataType, DecimalType, Value};

use super::Emitter;
use crate::Dialect;
use crate::build::types;
use crate::report::StepKind;

/// How the parent of an expression reads it.
#[derive(Debug, Clone, Copy)]
pub(super) struct Reading {
    /// As a value of its type: a column of the result, a key of `ORDER BY`
    /// or `GROUP BY` (see `Emitter::column`).
    pub(super) column: bool,
    /// As a column of the result whose rows are not compared, of its text
    /// and its type (`Emitter::column` and `Emitter::json_as_is`).
    pub(super) result: bool,
    /// As a number (see `Emitter::number`).
    pub(super) number: bool,
    /// As an operand of a comparison (see `Emitter::compared`).
    pub(super) compared: bool,
}

impl Emitter<'_> {
    /// Writes `CASE ... END`, cast to the family of its type in the input
    /// when the output gives another one, its results at the scale of its
    /// type when the output needs it.
    pub(super) fn case(&mut self, expr: &Expr, case: &Case, reading: Reading) {
        let results: Vec<&Expr> = case
            .branches
            .iter()
            .map(|branch| &branch.then)
            .chain(case.otherwise.as_deref())
            .collect();
        let across = self.source.dialect != self.output.dialect;
        if across {
            self.case_unknown(expr, &results, reading);
        }
        let output = types::case(
            self.output.dialect,
            results.iter().map(|result| &result.ty.data_type),
        );
        let target = cast_target(&expr.ty.data_type, &output);
        if let Some(target) = &target {
            let message = format!(
                "{} gives this `CASE` a {}, {} {}: cast to `{target}`",
                self.target(),
                family(&output),
                self.source_name(),
                family(&expr.ty.data_type)
            );
            self.step("case.type", StepKind::Emulation, message, expr.span);
            self.write("CAST(");
        }
        let scale = if across && !reading.number {
            self.case_scale(expr, &results, reading)
        } else {
            None
        };
        self.write("CASE");
        if let Some(operand) = &case.operand {
            self.write(" ");
            self.number_expr(operand, 0);
        }
        // The results are read as the `CASE` is, when it is a number.
        let numeric = matches!(
            expr.ty.data_type,
            DataType::Integer(_) | DataType::Decimal(_) | DataType::Float(_)
        );
        let inherited = numeric.then_some(reading);
        for branch in &case.branches {
            self.write(" WHEN ");
            match &branch.when.kind {
                // A `JSON` is read as by `IF` (`truth` module).
                ExprKind::ToBoolean(json) if json.ty.data_type == DataType::Json => {
                    self.json_condition(json, 0, false);
                }
                _ => self.number_expr(&branch.when, 0),
            }
            self.write(" THEN ");
            self.case_result(&branch.then, scale, inherited);
        }
        if let Some(otherwise) = &case.otherwise {
            self.write(" ELSE ");
            self.case_result(otherwise, scale, inherited);
        }
        self.write(" END");
        if let Some(target) = target {
            self.write(&format!(" AS {target})"));
        }
    }

    /// Errors of a `CASE` whose results include a value of unknown type
    /// (a function, a system variable): among integers and `NULL`, the family of the type may
    /// differ; with a decimal that has a scale, read otherwise than as a
    /// number, the scale of its values.
    fn case_unknown(&mut self, expr: &Expr, results: &[&Expr], reading: Reading) {
        let types: Vec<&DataType> = results.iter().map(|result| &result.ty.data_type).collect();
        if !types.contains(&&DataType::Unknown) {
            return;
        }
        let integral = types.iter().all(|data_type| {
            matches!(
                data_type,
                DataType::Integer(_) | DataType::Null | DataType::Unknown
            )
        });
        let scaled = types.iter().any(
            |data_type| matches!(data_type, DataType::Decimal(decimal) if decimal.scale() > 0),
        );
        if integral {
            self.error(
                "case.type",
                "`CASE` of a result whose type is unknown (a function, a system variable) with integers or `NULL`: an unsigned integer with a signed one or with `NULL` gives an integer in MySQL, a `DECIMAL` in MariaDB",
                expr.span,
            );
        } else if scaled && !reading.number {
            self.error(
                "case.scale",
                "`CASE` of a result whose type is unknown (a function, a system variable) with a decimal: MySQL writes each value of a `DECIMAL` `CASE` at the scale of its result (`42` for an integer), MariaDB at the scale of the type (`42.00`)",
                expr.span,
            );
        }
    }

    /// Type to which the results of a smaller scale than the `CASE` are
    /// written towards MySQL; towards MariaDB, `None`, with an error when
    /// the parent may read the values as strings.
    fn case_scale(
        &mut self,
        expr: &Expr,
        results: &[&Expr],
        reading: Reading,
    ) -> Option<DecimalType> {
        let DataType::Decimal(decimal) = expr.ty.data_type else {
            return None;
        };
        if decimal.scale() == 0 {
            return None;
        }
        let example = results
            .iter()
            .find(|result| smaller_scale(&result.ty.data_type, decimal))?;
        let written = format!("DECIMAL({}, {})", decimal.precision(), decimal.scale());
        if self.output.dialect == Dialect::MySql {
            let message = format!(
                "{} writes each value of a `DECIMAL` `CASE` at the scale of its result, {} at the scale of the type: the results of a smaller scale written as `{written}`",
                self.target(),
                self.source_name()
            );
            self.step("case.scale", StepKind::Emulation, message, example.span);
            return Some(decimal);
        }
        if !reading.column {
            let message = format!(
                "`CASE` of type `{written}` with a result of a smaller scale, read by an expression that may read it as a string: {} writes each value at the scale of its result (`42` for an integer), {} at the scale of the type (`42.00`); the same in both when each result has the scale of the type (`CAST(x AS {written})`)",
                self.source_name(),
                self.target()
            );
            self.error("case.scale", &message, expr.span);
        }
        None
    }

    /// Writes a result of a `CASE`, read as `reading` when it has one (the
    /// parent of a non-numeric `CASE` reads its results as strings), at the
    /// scale of `scale` when it has a smaller one: a literal with its
    /// digits, another value cast.
    fn case_result(&mut self, result: &Expr, scale: Option<DecimalType>, reading: Option<Reading>) {
        let Some(decimal) = scale.filter(|decimal| smaller_scale(&result.ty.data_type, *decimal))
        else {
            if let Some(reading) = reading {
                self.column = reading.column;
                self.number = reading.number;
            }
            self.expr(result, 0);
            return;
        };
        self.at(result.span);
        let zeros = |digits: usize| "0".repeat(usize::from(decimal.scale()).saturating_sub(digits));
        match &result.kind {
            ExprKind::Literal(Value::Integer(value)) => {
                self.write(&format!("{value}.{}", zeros(0)));
            }
            ExprKind::Literal(Value::Decimal(value)) => {
                let text = value.as_str();
                match text.split_once('.') {
                    Some((_, fraction)) => self.write(&format!("{text}{}", zeros(fraction.len()))),
                    None => self.write(&format!("{text}.{}", zeros(0))),
                }
            }
            _ => {
                self.write("CAST(");
                self.number_expr(result, 0);
                self.write(&format!(
                    " AS DECIMAL({}, {}))",
                    decimal.precision(),
                    decimal.scale()
                ));
            }
        }
    }
}

/// Tells whether a result of type `data_type` has a smaller scale than the
/// `DECIMAL` of its `CASE`.
fn smaller_scale(data_type: &DataType, decimal: DecimalType) -> bool {
    match data_type {
        DataType::Integer(_) => true,
        DataType::Decimal(other) => other.scale() < decimal.scale(),
        _ => false,
    }
}

/// Type of the `CAST` that gives the family of `input` to a `CASE` of type
/// `output`; `None` when both are of the same family.
fn cast_target(input: &DataType, output: &DataType) -> Option<String> {
    match (input, output) {
        (DataType::Integer(integer), DataType::Decimal(_)) => Some(
            if integer.unsigned {
                "UNSIGNED"
            } else {
                "SIGNED"
            }
            .to_string(),
        ),
        (DataType::Decimal(decimal), DataType::Integer(_)) => Some(format!(
            "DECIMAL({}, {})",
            decimal.precision(),
            decimal.scale()
        )),
        _ => None,
    }
}

/// Family of a numeric type, for the messages.
const fn family(data_type: &DataType) -> &'static str {
    match data_type {
        DataType::Decimal(_) => "`DECIMAL`",
        _ => "integer",
    }
}
