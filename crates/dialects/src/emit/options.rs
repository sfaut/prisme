//! Options of a `SELECT` block (`select_options` module), written after
//! `SELECT` and its `DISTINCT` in the order of the input.
//!
//! The options without effect on the rows (`HIGH_PRIORITY`,
//! `STRAIGHT_JOIN`, `SQL_SMALL_RESULT`, `SQL_BIG_RESULT`,
//! `SQL_BUFFER_RESULT`, `SQL_NO_CACHE`, `SQL_CACHE`) are written towards the
//! same dialect, left out towards another one, with a step in the report
//! (rule `option.dialect`, point 42 of `docs/QUESTIONS.md`).
//! `SQL_CALC_FOUND_ROWS` sets what `FOUND_ROWS()` gives next: it goes
//! through.
//!
//! An option of the first `SELECT` only, on a `SELECT` that is another one
//! in the output (inside the derived table that stands for parentheses, or
//! between parentheses followed by `ORDER BY` or `LIMIT` towards MariaDB),
//! is left out, with its step; `SQL_CALC_FOUND_ROWS` there is an error (rule
//! `option.placement`).
//!
//! `FOUND_ROWS()` after `SQL_CALC_FOUND_ROWS` on a set operation of `UNION
//! ALL` alone, with a `LIMIT` and without `ORDER BY`, counts every row in
//! MySQL from 8.0.18, the rows up to the `LIMIT` before and in MariaDB
//! (construct `union_all_found_rows`, rule `option.found-rows`): an error
//! between them, the next statement being out of reach of the emitter.

use prisme_plan::{Plan, Project, SelectOption, SetOperation, SetOperator};

use super::{Block, Emitter};
use crate::report::StepKind;
use crate::select_options;
use crate::versions::Construct;

impl Emitter<'_> {
    /// Options of `project`, each followed by a space, on the first
    /// `SELECT` of the output when `head`.
    pub(super) fn select_options(&mut self, project: &Project, head: bool) {
        let output = self.output.dialect;
        for written in &project.options {
            let option = written.option;
            let keyword = option.keyword();
            let target = self.target();
            if option != SelectOption::CalcFoundRows && self.source.dialect != output {
                let message = format!(
                    "`{keyword}`, without effect on the rows, left out towards another dialect, \
                     {target}"
                );
                self.step("option.dialect", StepKind::Omission, message, written.span);
                continue;
            }
            if !head && select_options::first_only(output, option) {
                let refused = format!(
                    "{target} refuses `{keyword}` on a `SELECT` other than the first one of \
                     the statement, or between parentheses followed by `ORDER BY` or `LIMIT` \
                     (error 1234)"
                );
                if option == SelectOption::CalcFoundRows {
                    let message = format!("{refused}: not supported yet by Prisme");
                    self.error("option.placement", &message, written.span);
                } else {
                    let message = format!("{refused}: without effect on the rows, left out");
                    self.step(
                        "option.placement",
                        StepKind::Omission,
                        message,
                        written.span,
                    );
                }
                continue;
            }
            self.at(written.span);
            self.write(&format!("{keyword} "));
        }
    }

    /// Refuses `SQL_CALC_FOUND_ROWS` on a set operation of `UNION ALL` alone
    /// with a `LIMIT` and without `ORDER BY`, `plan`, when the output counts
    /// its rows for `FOUND_ROWS()` otherwise than the input.
    pub(super) fn found_rows_union(&mut self, plan: &Plan) {
        let plan = match plan {
            Plan::With(with) => with.body.as_ref(),
            _ => plan,
        };
        let Plan::Limit(limit) = plan else {
            return;
        };
        let Plan::SetOperation(operation) = limit.input.as_ref() else {
            return;
        };
        let construct = Construct::UnionAllFoundRows;
        if !union_all(operation) || self.input_has(construct) == self.has(construct) {
            return;
        }
        let Some(written) = head_project(&operation.left).and_then(|project| {
            (project.options.iter()).find(|written| written.option == SelectOption::CalcFoundRows)
        }) else {
            return;
        };
        let source = format!("{} {}", self.source.dialect.label(), self.source.version);
        let counts = |all: bool| {
            if all {
                "every row"
            } else {
                "the rows up to the `LIMIT`"
            }
        };
        let message = format!(
            "after `SQL_CALC_FOUND_ROWS` on a `UNION ALL` with a `LIMIT` and without `ORDER BY`, \
             `FOUND_ROWS()` counts {} in {source}, {} in {}: not supported by Prisme",
            counts(self.input_has(construct)),
            counts(self.has(construct)),
            self.target()
        );
        self.error("option.found-rows", &message, written.span);
    }
}

/// Tells whether every operator of `operation`, and of the set operations it
/// holds without parentheses of their own, is `UNION ALL`.
fn union_all(operation: &SetOperation) -> bool {
    let operand = |plan: &Plan| match plan {
        Plan::SetOperation(operation) => union_all(operation),
        _ => true,
    };
    operation.operator == SetOperator::UnionAll
        && operand(&operation.left)
        && operand(&operation.right)
}

/// Select list of the first `SELECT` of `plan`, an operand of a set
/// operation.
fn head_project(plan: &Plan) -> Option<&Project> {
    match plan {
        Plan::SetOperation(operation) => head_project(&operation.left),
        Plan::Lock(lock) => head_project(&lock.input),
        Plan::Limit(limit) if Block::fold(plan).is_none() => head_project(&limit.input),
        Plan::Sort(sort) if Block::fold(plan).is_none() => head_project(&sort.input),
        _ => Block::fold(plan).map(|block| block.project),
    }
}
