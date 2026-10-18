//! A `JSON`, which MySQL compares, sorts and converts by the rules of JSON,
//! and MariaDB as its text, its `JSON` being a `LONGTEXT` of collation
//! `utf8mb4_bin` (`crates/probe/cases/json-text.txt`, measured on MySQL
//! 8.0.11 to 8.0.17 and 8.4.11, MariaDB 10.6.28 and 11.8.9).
//!
//! MySQL compares a `JSON` by the rules of JSON with `=`, `<`, `IN`, `CASE`
//! (the string `"b"` equals `'b'`, the number `1` equals `1.0` and differs
//! from `"1"` and from `true`), sorts and groups it by them (`ORDER BY`,
//! `GROUP BY`, `DISTINCT`, `UNION`, `PARTITION BY`, `MIN`, `MAX`,
//! `COUNT(DISTINCT)`), and converts its value to a number (`true` gives 1,
//! `"1"` 1). MariaDB compares and sorts its text (`'"b"'`), and converts its
//! text to a number (`'true'` gives 0). Both read the same text in a column
//! of a result, an operand of `LIKE` or of `IS NULL`, an argument of
//! `CONCAT`, `COUNT` or `GROUP_CONCAT`, a `CAST` to a string; a `JSON_`
//! function reads its value in both.
//!
//! Towards MySQL from MariaDB, `CONCAT(j)` gives the reading of MariaDB: the
//! text of the `JSON`, of collation `utf8mb4_bin` and of coercibility 2,
//! those of a `JSON` column of MariaDB (`CAST(j AS CHAR)` takes the collation
//! of the connection). It is written around a column or a scalar subquery
//! of type `JSON` that the parent reads otherwise than its text, and around
//! the `JSON` columns of a query whose rows are compared: `DISTINCT`, an
//! operand of a set operation other than `UNION ALL`, a subquery of `IN`,
//! `ANY` or `ALL` (rule `json.text`). An expression over them (`MIN(j)`,
//! `COALESCE(j, k)`) reads their text. A `JSON` is never an index of its
//! own: the text loses none. In a block with `HAVING`, a key of `GROUP BY`
//! read as its text is followed by the key as is (`GROUP BY CONCAT(j), j`),
//! which groups alike, a text giving one `JSON`: `HAVING` of MySQL names
//! the columns of `GROUP BY`, not those of its expressions. Towards MariaDB
//! from MySQL, an error. A join `USING` a `JSON` column, a `TABLE` whose
//! rows are compared, a key of `GROUP BY ... WITH ROLLUP` with `HAVING`: an
//! error.
//!
//! MySQL stores the text of a `JSON` normalized (`{"a":1}` stored `{"a": 1}`,
//! `2.50` stored `2.5`), MariaDB as written: a difference of the data, which
//! the reading of the query does not reach.
//!
//! Between versions of MySQL, `IN` of a list compares all its values by the
//! rules of JSON when one is a `JSON`, from 8.0.17 (construct
//! `json_in_compared`); before, a `JSON` as its text against a string, as a
//! number against a number. From 8.0.17, `j IN (a, b)` is written `j = a OR
//! j = b`, which compares by the rules of JSON in every version, when the
//! operand is the `JSON`, of the same value at each evaluation (neither a
//! subquery nor a function of `VOLATILE`). Otherwise (`'b' IN (j, 'x')`
//! compares `'b'` with `'x'` by the rules of JSON too), and towards 8.0.17,
//! an error (rule `json.in`).
//!
//! Only a value the plan types `JSON` is concerned: the results of the
//! `JSON_` functions are of unknown type yet, their translation between the
//! dialects refused.

use prisme_plan::{Expr, ExprKind, Join, Plan};
use prisme_types::DataType;

use super::Emitter;
use super::expr::{AND, OR, PREDICATE};
use super::rules::repeatable;
use crate::Dialect;
use crate::build::is_internal_name;
use crate::report::StepKind;
use crate::versions::Construct;

impl Emitter<'_> {
    /// Writes `expr`, a column or a scalar subquery of type `JSON` that the
    /// parent compares, sorts or converts, as the input reads it: as its
    /// text towards MySQL from MariaDB. True when written; towards MariaDB
    /// from MySQL, an error, the value left to the caller.
    pub(super) fn json_text(&mut self, expr: &Expr) -> bool {
        if !self.json_differs(expr) {
            return false;
        }
        let message = self.json_message("compares, sorts and converts a `JSON`");
        if self.source.dialect == Dialect::MySql {
            self.error("json.text", &message, expr.span);
            return false;
        }
        let message = format!("{message}: read as its text, by `CONCAT`");
        self.step("json.text", StepKind::Emulation, message, expr.span);
        self.at(expr.span);
        self.write("CONCAT(");
        self.json_as_is = true;
        self.expr(expr, 0);
        self.write(")");
        true
    }

    /// Tells whether `expr` is a column or a scalar subquery of type `JSON`,
    /// which the input and the output compare otherwise.
    pub(super) fn json_differs(&self, expr: &Expr) -> bool {
        let value = match &expr.kind {
            ExprKind::Column(column) => {
                column.qualifier.is_some() || !is_internal_name(&column.name.value)
            }
            ExprKind::ScalarSubquery(_) => true,
            _ => false,
        };
        value && expr.ty.data_type == DataType::Json && self.source.dialect != self.output.dialect
    }

    /// After `key`, a key of `GROUP BY` of a block with `HAVING`, read as its
    /// text: the key as is, which groups alike (a text gives one `JSON`) and
    /// that `HAVING` can name, MySQL naming there the columns of `GROUP BY`,
    /// not those of its expressions. With `WITH ROLLUP`, which would add a
    /// level of totals, an error.
    pub(super) fn json_having_key(&mut self, key: &Expr, rollup: bool) {
        if self.source.dialect != Dialect::MariaDb || !self.json_differs(key) {
            return;
        }
        if rollup {
            let message = self.json_message("compares, sorts and converts a `JSON`");
            let message = format!(
                "{message}; `HAVING` names a key of `GROUP BY ... WITH ROLLUP` read as its text: not supported yet by Prisme"
            );
            self.error("json.text", &message, key.span);
            return;
        }
        self.write(", ");
        self.json_as_is = true;
        self.expr(key, 0);
    }

    /// Records the error of a join `USING` or `NATURAL` on a `JSON` column,
    /// which the input and the output compare otherwise.
    pub(super) fn json_using(&mut self, join: &Join) {
        let Some(using) = &join.using else {
            return;
        };
        if self.source.dialect == self.output.dialect {
            return;
        }
        let json = [join.left.schema(), join.right.schema()]
            .iter()
            .flat_map(|schema| &schema.columns)
            .any(|column| {
                column.ty.data_type == DataType::Json
                    && using
                        .columns
                        .iter()
                        .any(|name| name.value.eq_ignore_ascii_case(&column.name.value))
            });
        if json {
            let message = self.json_message("compares a `JSON` of `USING`");
            let message = format!("{message}: not supported yet by Prisme");
            self.error("json.text", &message, join.span);
        }
    }

    /// Records the error of a `TABLE` statement whose rows the parent
    /// compares, when a column is a `JSON`.
    pub(super) fn json_rows(&mut self, plan: &Plan) {
        let json = plan
            .schema()
            .columns
            .iter()
            .any(|column| column.ty.data_type == DataType::Json);
        if json && self.source.dialect != self.output.dialect {
            let message = self.json_message("compares a `JSON` of the rows of `TABLE`");
            let message = format!("{message}: not supported yet by Prisme");
            self.error("json.text", &message, plan.span());
        }
    }

    /// `IN` of a list at the level `min`, between versions of MySQL that
    /// compare a `JSON` of it otherwise (rule `json.in`); true when written.
    pub(super) fn json_in(&mut self, expr: &Expr, min: u8) -> bool {
        let ExprKind::InList {
            operand,
            list,
            negated,
        } = &expr.kind
        else {
            return false;
        };
        let json = |value: &Expr| value.ty.data_type == DataType::Json;
        if self.source.dialect != Dialect::MySql
            || self.output.dialect != Dialect::MySql
            || list.len() < 2
            || !(json(operand) || list.iter().any(json))
        {
            return false;
        }
        let compared = self.input_has(Construct::JsonInCompared);
        if compared == self.has(Construct::JsonInCompared) {
            return false;
        }
        let reading = |compared: bool| {
            if compared {
                "by the rules of JSON"
            } else {
                "as its text or as a number"
            }
        };
        let message = format!(
            "{} compares a `JSON` of `IN` {}, {} {}",
            self.source_name(),
            reading(compared),
            self.target(),
            reading(!compared)
        );
        if !compared {
            self.error("json.in", &message, expr.span);
            return false;
        }
        if !json(operand) {
            let message = format!(
                "{message}, and its other values by the rules of JSON too: not supported yet by Prisme"
            );
            self.error("json.in", &message, expr.span);
            return false;
        }
        if !repeatable(operand) {
            let message = format!(
                "{message}, and the emulation writes the operand once per value, which may give several values"
            );
            self.error("json.in", &message, expr.span);
            return false;
        }
        let (level, separator, comparison) = if *negated {
            (AND, " AND ", " <> ")
        } else {
            (OR, " OR ", " = ")
        };
        let message = format!("{message}: emulated by comparisons `{}`", comparison.trim());
        self.step("json.in", StepKind::Emulation, message, expr.span);
        let parenthesized = level < min;
        if parenthesized {
            self.write("(");
        }
        for (index, value) in list.iter().enumerate() {
            if index > 0 {
                self.write(separator);
            }
            self.number_expr(operand, PREDICATE);
            self.write(comparison);
            self.number_expr(value, PREDICATE + 1);
        }
        if parenthesized {
            self.write(")");
        }
        true
    }

    /// Message telling how the input and the output read a `JSON` in `what`
    /// (`compares a JSON of USING`).
    fn json_message(&self, what: &str) -> String {
        let (input, output) = match self.source.dialect {
            Dialect::MariaDb => ("as its text", "by the rules of JSON"),
            Dialect::MySql => ("by the rules of JSON", "as its text"),
        };
        format!(
            "{} {what} {input}, {} {output}",
            self.source_name(),
            self.target()
        )
    }
}

/// Tells whether the function `name`, in capitals, reads a `JSON` alike in
/// both dialects: `CONCAT`, `CONCAT_WS`, `COUNT` and `GROUP_CONCAT` its
/// text, a `JSON_` function its value.
pub(super) fn reads_json_as_is(name: &str) -> bool {
    matches!(name, "CONCAT" | "CONCAT_WS" | "COUNT" | "GROUP_CONCAT") || name.starts_with("JSON_")
}
