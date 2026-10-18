//! Query between parentheses with its own `ORDER BY` or `LIMIT`, followed by
//! an outer `ORDER BY` or `LIMIT` (rule `limit.nested`), measured on each
//! image of MySQL 8.0.11 to 9.3.0 and MariaDB 10.5.29 to 11.8.9
//! (`crates/probe/cases/parenthesized-limits.sql`,
//! `docs/sources/mysql/parenthesized-limits.md`).
//!
//! The plan reads it by the standard, as MariaDB does:
//! `(SELECT ... LIMIT 2) LIMIT 5` gives 2 rows at most. MySQL reads an outer
//! `LIMIT` without outer `ORDER BY` over an inner `LIMIT` otherwise:
//!
//! - up to 8.0.30, the outer `LIMIT` overrides the inner one, its offset
//!   included (5 rows; construct `parenthesized_limit_kept`), and up to
//!   8.0.18 it drops the inner `ORDER BY` as well (construct
//!   `parenthesized_order_kept`, Bug #30521098);
//! - from 8.0.31, an outer `OFFSET` counts from the start of the inner rows
//!   without bounding them by the inner `LIMIT`:
//!   `(SELECT a ... ORDER BY a LIMIT 3) LIMIT 5 OFFSET 1` gives 2, 3, 4, and
//!   2, 3 in MariaDB, in every version measured (construct
//!   `parenthesized_offset_kept`, absent from MySQL);
//! - from 8.0.19 to 8.0.30, a set operation between parentheses with its own
//!   `ORDER BY` or `LIMIT`, followed by another, is refused, error 1235
//!   (construct `parenthesized_set_limit`);
//! - up to 8.0.18, an outer `ORDER BY` sorts the rows of a set operation
//!   between parentheses before its own `LIMIT`:
//!   `(SELECT a ... UNION SELECT 6 ORDER BY 1 LIMIT 2) ORDER BY 1 DESC` gives
//!   6, 5, and 2, 1 by the standard (construct `parenthesized_set_order`).
//!
//! An outer `ORDER BY` after a single query reads by the standard in every
//! version. A derived
//! table reads by the standard in every version too
//! (`SELECT * FROM (SELECT ... LIMIT 2) AS d LIMIT 5`): it stands for the
//! parentheses towards a server that reads them otherwise than the input,
//! when the input reads them by the standard; when the input reads them
//! otherwise, and the output otherwise than the input, the query is an error
//! (**[decided autonomously]**).

use prisme_plan::{Limit, Plan, Sort};
use prisme_types::Value;

use super::{Block, Emitter};
use crate::report::StepKind;
use crate::versions::Construct;

/// How a server reads a query between parentheses followed by an outer
/// `ORDER BY` or `LIMIT`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reading {
    /// By the standard: each `LIMIT` bounds the rows of its own level.
    Standard,
    /// The outer `LIMIT` overrides the inner one, offset included.
    Override,
    /// The outer `LIMIT` overrides the inner one and drops the inner
    /// `ORDER BY`.
    OverrideUnordered,
    /// The outer `OFFSET` counts from the start of the inner rows, without
    /// bounding them by the inner `LIMIT`.
    OffsetUnbounded,
    /// The outer `ORDER BY` sorts the rows of the set operation before its
    /// own `LIMIT`.
    SortedFirst,
    /// Refused (error 1235).
    Refused,
}

/// Shape of a query between parentheses and of the clauses after it.
#[derive(Debug, Clone, Copy)]
struct Nesting {
    /// The outer query has an `ORDER BY`.
    outer_sort: bool,
    /// The outer query has a `LIMIT`.
    outer_limit: bool,
    /// The outer `LIMIT` has an offset other than 0.
    outer_offset: bool,
    /// The query between parentheses has an `ORDER BY` at its top.
    inner_sort: bool,
    /// The query between parentheses has a `LIMIT` at its top.
    inner_limit: bool,
    /// The query between parentheses is a set operation.
    inner_set: bool,
}

impl Nesting {
    /// Shape of `inner`, between parentheses, followed by `sort` and
    /// `limit`.
    fn of(inner: &Plan, sort: Option<&Sort>, limit: Option<&Limit>) -> Self {
        let mut node = match inner {
            Plan::With(with) => with.body.as_ref(),
            _ => inner,
        };
        let mut inner_limit = false;
        let mut inner_sort = false;
        if let Some(block) = Block::fold(node) {
            inner_limit = block.limit.is_some();
            inner_sort = block.sort.is_some();
        } else {
            if let Plan::Limit(limit) = node {
                inner_limit = true;
                node = &limit.input;
            }
            if let Plan::Sort(sort) = node {
                inner_sort = true;
                node = &sort.input;
            }
        }
        Self {
            outer_sort: sort.is_some(),
            outer_limit: limit.is_some(),
            outer_offset: limit.is_some_and(|limit| {
                limit.offset.as_ref().is_some_and(|offset| {
                    !matches!(
                        &offset.kind,
                        prisme_plan::ExprKind::Literal(Value::Integer(0))
                    )
                })
            }),
            inner_sort,
            inner_limit,
            inner_set: matches!(node, Plan::SetOperation(_)),
        }
    }
}

impl Emitter<'_> {
    /// `inner` between parentheses, before the outer `sort` and `limit`
    /// that the caller writes: as written when the output reads it as the
    /// input does, from a derived table when the input reads it by the
    /// standard and the output otherwise, an error otherwise.
    pub(super) fn parenthesized(
        &mut self,
        inner: &Plan,
        sort: Option<&Sort>,
        limit: Option<&Limit>,
        names_matter: bool,
    ) {
        let nesting = Nesting::of(inner, sort, limit);
        let input = self.reading(nesting, true);
        let output = self.reading(nesting, false);
        if input == output {
            self.write("(");
            self.query(inner, names_matter);
            self.write(")");
            return;
        }
        let span = limit.and_then(|limit| limit.span).or_else(|| inner.span());
        if input != Reading::Standard {
            let source = format!("{} {}", self.source.dialect.label(), self.source.version);
            let message = format!(
                "{source} {}, {} {}: not supported yet by Prisme",
                describe(input),
                self.target(),
                describe(output)
            );
            self.error("limit.nested", &message, span);
            self.write("(");
            self.query(inner, names_matter);
            self.write(")");
            return;
        }
        let message = format!("{} {}", self.target(), describe(output));
        if self.has_duplicate_names(inner) {
            let message =
                format!("{message}, and its columns have the same names: not supported yet");
            self.error("limit.nested", &message, span);
        }
        self.write("SELECT * FROM (");
        self.query(inner, true);
        self.write(") AS `d`");
        self.step(
            "limit.nested",
            StepKind::Emulation,
            format!("{message}: emulated by a derived table"),
            span,
        );
    }

    /// Reading of `nesting` by the input when `input`, by the output
    /// otherwise.
    fn reading(&self, nesting: Nesting, input: bool) -> Reading {
        let has = |construct| {
            if input {
                self.input_has(construct)
            } else {
                self.has(construct)
            }
        };
        let limited = nesting.inner_sort || nesting.inner_limit;
        let outer = nesting.outer_sort || nesting.outer_limit;
        if nesting.inner_set && limited && outer && !has(Construct::ParenthesizedSetLimit) {
            return Reading::Refused;
        }
        if nesting.inner_set
            && nesting.inner_limit
            && nesting.outer_sort
            && !has(Construct::ParenthesizedSetOrder)
        {
            return Reading::SortedFirst;
        }
        if !nesting.outer_limit || nesting.outer_sort || !nesting.inner_limit {
            return Reading::Standard;
        }
        if !has(Construct::ParenthesizedLimitKept) {
            if nesting.inner_sort && !has(Construct::ParenthesizedOrderKept) {
                return Reading::OverrideUnordered;
            }
            return Reading::Override;
        }
        if nesting.outer_offset && !has(Construct::ParenthesizedOffsetKept) {
            return Reading::OffsetUnbounded;
        }
        Reading::Standard
    }

    /// Tells whether two columns of `plan` have the same name, which a
    /// derived table refuses.
    pub(super) fn has_duplicate_names(&self, plan: &Plan) -> bool {
        let schema = plan.schema();
        let names = &schema.columns;
        names.iter().enumerate().any(|(index, column)| {
            names[..index]
                .iter()
                .any(|other| other.name.value.eq_ignore_ascii_case(&column.name.value))
        })
    }
}

/// What a server does with a query between parentheses read as `reading`,
/// for the messages.
fn describe(reading: Reading) -> &'static str {
    match reading {
        Reading::Standard => "reads a `LIMIT` after a query between parentheses by the standard",
        Reading::Override => {
            "lets a `LIMIT` after a query between parentheses override the `LIMIT` of that query"
        }
        Reading::OverrideUnordered => {
            "lets a `LIMIT` after a query between parentheses override the `LIMIT` and the `ORDER BY` of that query"
        }
        Reading::OffsetUnbounded => {
            "counts an `OFFSET` after a query between parentheses without the `LIMIT` of that query"
        }
        Reading::SortedFirst => {
            "sorts the rows of a set operation between parentheses by the outer `ORDER BY` before its own `LIMIT`"
        }
        Reading::Refused => {
            "refuses an `ORDER BY` or a `LIMIT` after a set operation between parentheses with its own"
        }
    }
}
