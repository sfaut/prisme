//! Recursive common table expressions, written as the output reads them
//! (`crates/probe/cases/with-recursive.sql`, measured on each local image).
//!
//! Both dialects store the rows of each iteration in columns of the type of
//! the anchors, but type an integer literal otherwise: a `BIGINT` in MySQL,
//! an `INT` up to 9 digits in MariaDB, where a larger value of a recursive
//! part is then an error (1264). Towards MariaDB, an integer column of the
//! anchors is written `IF(FALSE, -9223372036854775807, x)`, a `BIGINT`
//! there. MySQL reads the anchors before the recursive parts, which MariaDB
//! writes in any order: towards MySQL, they are written first, when MariaDB
//! reads their order as the standard does (`order_parts`). Each dialect
//! refuses parts that the other reads (a read inside a derived table, on
//! the inner side of an outer join, `GROUP BY`, `DISTINCT`, `LIMIT`), and
//! stores another value than NULL in a column of NULL otherwise: an error.

use prisme_plan::{
    CallForm, Cte, CtePart, Expr, ExprKind, FunctionCall, Ident, Limit, Plan, ProjectItem, Schema,
    SetOperation, SetOperator, Sort,
};
use prisme_types::{DataType, IntegerSize, IntegerType, Value};

use super::{Block, Emitter};
use crate::Dialect;
use crate::report::StepKind;
use crate::versions::Construct;

/// Part of the `UNION` of a recursive common table expression, with the
/// operator that joins it to the parts before it.
struct Part {
    /// Plan of the part.
    plan: Plan,
    /// How it reads the expression.
    reading: CtePart,
    /// Operator before it; `None` for the first part.
    operator: Option<SetOperator>,
}

impl Emitter<'_> {
    /// Query of the recursive common table expression `cte`, as the output
    /// reads it: its parts in the order the output reads them, the integer
    /// columns of its anchors wide enough, its parts checked.
    pub(super) fn recursive_plan(&mut self, cte: &Cte) -> Plan {
        let (limit, sort, union) = split(&cte.plan);
        let mut parts = Vec::with_capacity(cte.parts.len());
        flatten(union, &cte.parts, &mut parts);
        self.check_parts(cte, &parts, limit.is_some());
        self.order_parts(cte, &mut parts);
        if self.source.dialect == Dialect::MySql && self.output.dialect == Dialect::MariaDb {
            self.widen_anchors(cte, &mut parts);
        }
        let mut plan = join(parts, union.schema());
        if let Some(sort) = sort {
            plan = Plan::Sort(Sort {
                input: Box::new(plan),
                ..sort.clone()
            });
        }
        if let Some(limit) = limit {
            plan = Plan::Limit(Limit {
                input: Box::new(plan),
                ..limit.clone()
            });
        }
        plan
    }

    /// Records the parts and the clauses of `cte` that the output refuses,
    /// and the columns of NULL that the other dialect fills otherwise.
    fn check_parts(&mut self, cte: &Cte, parts: &[Part], limited: bool) {
        let span = cte.span;
        let name = &cte.name.value;
        if limited && !self.has(Construct::RecursiveLimit) {
            let message = format!(
                "{} has no {}",
                self.target(),
                Construct::RecursiveLimit.label()
            );
            self.error("cte.recursive-limit", &message, span);
        }
        let output = self.output.dialect;
        for part in parts.iter().filter(|part| part.reading.recursive) {
            if output == Dialect::MySql && part.reading.derived {
                let message = format!(
                    "MySQL refuses the recursive common table expression `{name}` read inside a \
                     derived table (error 3577)"
                );
                self.error("cte.recursive-part", &message, span);
            }
            if output == Dialect::MariaDb && part.reading.outer_join {
                let message = format!(
                    "MariaDB refuses the recursive common table expression `{name}` read on the \
                     inner side of an outer join (error 4008)"
                );
                self.error("cte.recursive-part", &message, span);
            }
            let grouped = Block::fold(&part.plan)
                .is_some_and(|block| block.aggregate.is_some() || block.distinct);
            if output == Dialect::MySql && grouped {
                let message = format!(
                    "MySQL refuses `GROUP BY` and `DISTINCT` in the recursive parts of `{name}` \
                     (errors 3575 and 1235)"
                );
                self.error("cte.recursive-part", &message, span);
            }
        }
        if self.source.dialect == output {
            return;
        }
        let stored = cte.plan.schema();
        for (column, all) in cte.schema.columns.iter().zip(&stored.columns) {
            if column.ty.data_type == DataType::Null && all.ty.data_type != DataType::Null {
                let message = format!(
                    "the anchors of `{name}` give its column `{}` the type of NULL, where MySQL \
                     refuses another value (error 1406) and MariaDB stores an empty string",
                    column.name.value
                );
                self.error("cte.recursive-null", &message, span);
            }
        }
    }

    /// Writes the anchors of `cte` before its recursive parts towards
    /// MySQL, which refuses another order (error 3574), when one operator
    /// joins all its parts and MariaDB reads their order as the standard
    /// does: a recursive part then an anchor, or after a first anchor, no two
    /// anchors in a row. MariaDB keeps the last of two anchors in a row after
    /// a recursive part (`SELECT 1 UNION ALL R UNION ALL SELECT 10 UNION ALL
    /// SELECT 20` drops 10), and refuses two recursive parts after a first one
    /// (error 1235).
    fn order_parts(&mut self, cte: &Cte, parts: &mut [Part]) {
        let ordered = parts
            .windows(2)
            .all(|pair| !pair[0].reading.recursive || pair[1].reading.recursive);
        if ordered || self.output.dialect != Dialect::MySql || self.source.dialect == Dialect::MySql
        {
            return;
        }
        let name = &cte.name.value;
        let first = (parts.iter())
            .position(|part| part.reading.recursive)
            .unwrap_or_default();
        let standard = if first == 0 {
            parts.len() == 2
        } else {
            (parts.windows(2).skip(first))
                .all(|pair| pair[0].reading.recursive || pair[1].reading.recursive)
        };
        if !standard {
            let message = format!(
                "MySQL reads the anchors of `{name}` before its recursive parts, and MariaDB \
                 reads the order written otherwise than the standard: not supported yet by \
                 Prisme"
            );
            self.error("cte.recursive-order", &message, cte.span);
            return;
        }
        let operator = parts[1].operator;
        if parts.iter().skip(2).any(|part| part.operator != operator) {
            let message = format!(
                "MySQL reads the anchors of `{name}` before its recursive parts, joined here by \
                 several operators: not supported yet by Prisme"
            );
            self.error("cte.recursive-order", &message, cte.span);
            return;
        }
        parts.sort_by_key(|part| part.reading.recursive);
        parts[0].operator = None;
        for part in parts.iter_mut().skip(1) {
            part.operator = operator;
        }
        let message = format!(
            "MySQL reads the anchors of `{name}` before its recursive parts: written first"
        );
        self.step("cte.recursive-order", StepKind::Rewrite, message, cte.span);
    }

    /// Writes the integer columns of the anchors of `cte` as `BIGINT`s of
    /// MariaDB, which gives an integer literal of at most 9 digits an `INT`
    /// where MySQL gives it a `BIGINT`. A column of a table keeps its type.
    fn widen_anchors(&mut self, cte: &Cte, parts: &mut [Part]) {
        let widths: Vec<Option<bool>> = (cte.schema.columns.iter())
            .map(|column| match column.ty.data_type {
                DataType::Integer(IntegerType {
                    size: IntegerSize::Big,
                    unsigned,
                }) => Some(unsigned),
                DataType::Boolean => Some(false),
                _ => None,
            })
            .collect();
        if widths.iter().all(Option::is_none) {
            return;
        }
        let name = &cte.name.value;
        for part in parts.iter_mut().filter(|part| !part.reading.recursive) {
            if matches!(part.plan, Plan::Scan(_)) {
                continue;
            }
            let tables = Block::fold(&part.plan).map(|block| from_tables(block.source));
            let Some(project) = project_mut(&mut part.plan) else {
                let message = format!(
                    "MariaDB gives an `INT` to an integer literal of the anchors of `{name}`, \
                     where MySQL gives a `BIGINT`: an anchor without `SELECT` not supported yet \
                     by Prisme"
                );
                self.error("cte.recursive-type", &message, cte.span);
                return;
            };
            let tables = tables.unwrap_or(false);
            let mut position = 0;
            for item in &mut project.items {
                match item {
                    ProjectItem::Wildcard(wildcard) => {
                        let count = wildcard.columns.len();
                        let wide = widths[position..position + count]
                            .iter()
                            .any(Option::is_some);
                        if wide && !tables {
                            let message = format!(
                                "MariaDB gives an `INT` to an integer literal of the anchors of \
                                 `{name}`, where MySQL gives a `BIGINT`: `*` of a derived table \
                                 or of a common table expression not supported yet by Prisme"
                            );
                            self.error("cte.recursive-type", &message, cte.span);
                        }
                        position += count;
                    }
                    ProjectItem::Expr(named) => {
                        let column = matches!(named.expr.kind, ExprKind::Column(_)) && tables;
                        if let Some(unsigned) = widths[position]
                            && !(column
                                && named.expr.ty.data_type
                                    == cte.schema.columns[position].ty.data_type)
                        {
                            let column_name = &cte.schema.columns[position].name.value;
                            let bound = if unsigned {
                                "18446744073709551615"
                            } else {
                                "-9223372036854775807"
                            };
                            let message = format!(
                                "MariaDB gives an `INT` to an integer literal of at most 9 digits, \
                                 MySQL a `BIGINT`: the column `{column_name}` of `{name}` written \
                                 `IF(FALSE, {bound}, …)`, a `BIGINT` in MariaDB"
                            );
                            self.step(
                                "cte.recursive-type",
                                StepKind::Emulation,
                                message,
                                named.expr.span,
                            );
                            named.expr = widened(named.expr.clone(), unsigned);
                        }
                        position += 1;
                    }
                }
            }
        }
    }
}

/// The `LIMIT`, the `ORDER BY` and the `UNION` of the query of a common
/// table expression.
fn split(plan: &Plan) -> (Option<&Limit>, Option<&Sort>, &Plan) {
    let (limit, plan) = match plan {
        Plan::Limit(limit) => (Some(limit), limit.input.as_ref()),
        plan => (None, plan),
    };
    let (sort, plan) = match plan {
        Plan::Sort(sort) => (Some(sort), sort.input.as_ref()),
        plan => (None, plan),
    };
    (limit, sort, plan)
}

/// Parts of `plan`, the union of as many parts as `readings`, built from
/// left to right.
fn flatten(plan: &Plan, readings: &[CtePart], parts: &mut Vec<Part>) {
    let Some((last, before)) = readings.split_last() else {
        return;
    };
    match plan {
        Plan::SetOperation(operation) if !before.is_empty() => {
            flatten(&operation.left, before, parts);
            parts.push(Part {
                plan: (*operation.right).clone(),
                reading: *last,
                operator: Some(operation.operator),
            });
        }
        plan => parts.push(Part {
            plan: plan.clone(),
            reading: *last,
            operator: None,
        }),
    }
}

/// Union of `parts`, from left to right, of the columns `schema`.
fn join(parts: Vec<Part>, schema: Schema) -> Plan {
    let mut parts = parts.into_iter();
    let mut plan = parts.next().expect("a first part").plan;
    for part in parts {
        plan = Plan::SetOperation(SetOperation {
            operator: part.operator.unwrap_or(SetOperator::UnionAll),
            left: Box::new(plan),
            right: Box::new(part.plan),
            schema: schema.clone(),
            span: None,
        });
    }
    plan
}

/// Select list of the `SELECT` block `plan`.
fn project_mut(plan: &mut Plan) -> Option<&mut prisme_plan::Project> {
    match plan {
        Plan::Limit(limit) => project_mut(&mut limit.input),
        Plan::Distinct(distinct) => project_mut(&mut distinct.input),
        Plan::Project(project) => Some(project),
        _ => None,
    }
}

/// Tells whether the source `plan` reads tables only, whose columns have the
/// same type in both dialects.
fn from_tables(plan: &Plan) -> bool {
    match plan {
        Plan::Scan(_) => true,
        Plan::Join(join) => from_tables(&join.left) && from_tables(&join.right),
        _ => false,
    }
}

/// `IF(FALSE, -9223372036854775807, expr)`, a `BIGINT` in MariaDB, or
/// `IF(FALSE, 18446744073709551615, expr)` for an unsigned one.
fn widened(expr: Expr, unsigned: bool) -> Expr {
    let (bound, integer) = if unsigned {
        (
            i128::from(u64::MAX),
            IntegerType::unsigned(IntegerSize::Big),
        )
    } else {
        (-i128::from(i64::MAX), IntegerType::signed(IntegerSize::Big))
    };
    let span = expr.span;
    let ty = expr.ty.clone();
    let call = FunctionCall {
        qualifier: None,
        name: Ident::new("IF"),
        args: vec![
            Expr::literal(Value::Boolean(false), DataType::Boolean),
            Expr::literal(Value::Integer(bound), DataType::Integer(integer)),
            expr,
        ],
        stored: false,
        form: CallForm::Call,
    };
    let mut widened = Expr::function(call, ty);
    widened.span = span;
    widened
}
