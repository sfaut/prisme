//! A `JSON` read as a boolean, which each server reads its own way
//! (`crates/probe/cases/json-truth.txt`, measured on MySQL 8.0.11 to 8.0.17
//! and 8.4.11, MariaDB 10.6.28 and 11.8.9):
//!
//! - MySQL from 8.0.17 reads a value `x` of `WHERE`, `HAVING`, `ON` and of
//!   the operands of `NOT`, `AND`, `OR`, `XOR` and `IS TRUE` as `x <> 0`
//!   (construct `json_truth`): a `JSON` compared with 0 by the rules of
//!   JSON, true unless it is the number 0 (`true`, `false`, `null`, an
//!   object, an array, a string);
//! - MySQL before 8.0.17 reads it there as a number, as `IF` and `CASE WHEN`
//!   do in every version: `true` and `"1"` are true, `false`, an object or
//!   an array false;
//! - MariaDB reads its `JSON`, a `LONGTEXT`, as a string: `1` and `0.5` are
//!   true, `true` and `"1"` false.
//!
//! Towards MySQL from MariaDB, the text of the `JSON` (`CAST(j AS CHAR)`)
//! gives the reading of MariaDB, in every context. A `JSON` is never an
//! index of its own: the text loses none. The other directions are an error
//! (rule `boolean.json`); towards MariaDB, only an expression is a `JSON`,
//! its columns being `LONGTEXT`.

use prisme_plan::{Expr, FunctionCall};
use prisme_types::DataType;

use super::Emitter;
use crate::Dialect;
use crate::report::StepKind;
use crate::versions::Construct;

impl Emitter<'_> {
    /// Writes `json`, a `JSON` read as a boolean at the level `min`: by a
    /// clause or an operator when `versioned`, otherwise by `IF` or `CASE
    /// WHEN`. As is where the input and the output read it alike, as its
    /// text towards MySQL from MariaDB, an error otherwise.
    pub(super) fn json_condition(&mut self, json: &Expr, min: u8, versioned: bool) {
        let reading = |dialect: Dialect, compared: bool| match dialect {
            Dialect::MySql if versioned && compared => "as true unless it is the number 0",
            Dialect::MySql => "as a number",
            Dialect::MariaDb => "as a string",
        };
        let input = reading(self.source.dialect, self.input_has(Construct::JsonTruth));
        let output = reading(self.output.dialect, self.has(Construct::JsonTruth));
        if input == output {
            self.number_expr(json, min);
            return;
        }
        let message = format!(
            "{} reads a `JSON` read as a boolean {input}, {} {output}",
            self.source_name(),
            self.target()
        );
        if self.source.dialect == Dialect::MariaDb {
            let message = format!("{message}: read as its text");
            self.step("boolean.json", StepKind::Emulation, message, json.span);
            self.write("CAST(");
            self.expr(json, 0);
            self.write(" AS CHAR)");
        } else {
            self.error("boolean.json", &message, json.span);
            self.number_expr(json, min);
        }
    }

    /// `IF`: its condition, a `JSON`, read as by `CASE WHEN`.
    pub(super) fn if_call(&mut self, expr: &Expr, call: &FunctionCall) {
        if call
            .args
            .first()
            .is_none_or(|arg| arg.ty.data_type != DataType::Json)
        {
            self.input_call(expr, call);
            return;
        }
        let form = self.form(&call.name, expr.span);
        self.call_name(&call.name.value, form);
        for (index, arg) in call.args.iter().enumerate() {
            if index == 0 {
                self.json_condition(arg, 0, false);
            } else {
                self.write(", ");
                self.expr(arg, 0);
            }
        }
        self.write(")");
    }
}
