//! Emitter of the MySQL family: logical plan to SQL text of MySQL or
//! MariaDB, aware of the version of the server (`docs/DEV-DIALECTS.md`,
//! steps 4 and 5).
//!
//! The emitter writes the form of the input when the output dialect accepts
//! it (section 2.3), otherwise another form, an emulation or an error, each
//! one with its step in the report. The plan of a `SELECT` is folded back
//! into a block,
//! `Limit(Distinct(Project(Sort(Window(Filter(Aggregate(Filter(source))))))))`;
//! the sources of a `FROM` (`source` module) and the set operations put the
//! parentheses their nesting needs; the aggregation and the windows are
//! written by the `call` module, the calls to scalar functions and the names
//! of the built-in functions by the `builtin` module, with the rules of the
//! functions (`rules` module); the recursive common table expressions by the
//! `recursive` module, the items of the select list read through their alias
//! by the `alias` module.
//!
//! Literals are written as in the input when the output reads them the same
//! way (same SQL mode for the quotes and the backslashes), otherwise in a
//! canonical spelling of the same value. The comments of the input are
//! written again, each one before the first construct that follows it in
//! the input.

mod aes;
mod alias;
mod bit_count;
mod bitwise;
mod builtin;
mod call;
mod case;
mod cast;
mod charset;
mod expr;
mod extract;
mod greatest;
mod grouped_order;
mod hexadecimal;
mod interval;
mod json;
mod literal;
mod parentheses;
mod predicate;
mod recursive;
mod rules;
mod source;
mod string;
mod temporal;
mod truth;

#[cfg(test)]
mod tests;

use prisme_plan::{
    Aggregate, AliasForm, Direction, Filter, FilterForm, Ident, Limit, LimitForm, Nulls, Plan,
    Project, ProjectItem, SetOperation, SetOperator, Sort, SortKey, SortReference, Span, Statement,
    Values, ValuesForm, Window, With,
};
use prisme_types::Value;

use crate::Dialect;
use crate::build::{is_dual, is_internal_name};
use crate::keywords::is_reserved;
use crate::lexer::{CommentKind, SqlMode};
use crate::report::{Step, StepKind, Untranslatable};
use crate::versions::{Construct, Version, registry};

/// Dialect of the server that runs the query, with its version and its SQL
/// mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutputDialect {
    /// Dialect.
    pub dialect: Dialect,
    /// Version of the server.
    pub version: Version,
    /// SQL mode of the session.
    pub mode: SqlMode,
}

/// Text of the input, for the literals written as is and the comments.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Source<'a> {
    /// Text.
    pub(crate) text: &'a str,
    /// SQL mode it was read with.
    pub(crate) mode: SqlMode,
    /// Dialect it was written in.
    pub(crate) dialect: Dialect,
    /// Version of that dialect.
    pub(crate) version: Version,
    /// Comments, in order.
    pub(crate) comments: &'a [(Span, CommentKind)],
}

/// Query written, with its report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Emitted {
    /// Text.
    pub(crate) sql: String,
    /// Steps.
    pub(crate) steps: Vec<Step>,
    /// Untranslatable constructs.
    pub(crate) errors: Vec<Untranslatable>,
}

/// Writes `statement` in the output dialect.
pub(crate) fn emit(statement: &Statement, output: &OutputDialect, source: &Source<'_>) -> Emitted {
    let mut emitter = Emitter {
        out: String::new(),
        output,
        source,
        next_comment: 0,
        quiet: 0,
        blocks: Vec::new(),
        top: true,
        column: false,
        number: false,
        json_as_is: false,
        as_bytes: false,
        compared_rows: false,
        steps: Vec::new(),
        errors: Vec::new(),
    };
    match statement {
        Statement::Query(plan) => emitter.query(plan, true),
        _ => emitter.error(
            "statement.kind",
            "statements other than queries: not supported yet by the emitter of Prisme",
            None,
        ),
    }
    emitter.remaining_comments();
    Emitted {
        sql: emitter.out,
        steps: emitter.steps,
        errors: emitter.errors,
    }
}

/// State of the emitter.
struct Emitter<'a> {
    /// Text written.
    out: String,
    /// Output dialect.
    output: &'a OutputDialect,
    /// Input.
    source: &'a Source<'a>,
    /// Next comment of the input to write.
    next_comment: usize,
    /// Above 0, the comments wait: a part of the input is written away from
    /// its place.
    quiet: usize,
    /// Columns the `SELECT` blocks being written compute for themselves,
    /// the innermost last.
    blocks: Vec<call::Internal>,
    /// Tells whether the next `SELECT` block gives the result of the
    /// statement: false inside a `WITH` and a subquery.
    top: bool,
    /// Tells whether the next expression is read as a value of its type: a
    /// column of the result of the statement, a key of `ORDER BY` or
    /// `GROUP BY`. An expression its parent reads as a number or a boolean
    /// may compute another value in the MySQL family (`LEAST('10', '9')`
    /// gives `'10'`, `LEAST('10', '9') + 0` gives 9 in MariaDB).
    column: bool,
    /// Tells whether the next expression is read as a number: an operand of
    /// an operator other than `LIKE`, an argument of an aggregate or window
    /// function that reads numbers. MySQL writes a `DECIMAL` `CASE` as a
    /// string at the scale of each result (`42` for an integer), as a
    /// number at the scale of its type.
    number: bool,
    /// Tells whether the next expression keeps a `JSON` as written, both
    /// dialects reading its text or its value there: a column of the
    /// result, an operand of `LIKE` or `IS NULL`, an argument of `CONCAT` or
    /// of a `JSON_` function (`json` module).
    json_as_is: bool,
    /// Tells whether the next expression is read as bytes, a hexadecimal or
    /// bit literal as its bytes alike in both dialects: an operand of a
    /// comparison with a string, of `LIKE`, an argument of `CAST` to a
    /// string or of a string function (`hexadecimal` module).
    as_bytes: bool,
    /// Tells whether the rows of the next query are compared: an operand of
    /// a set operation other than `UNION ALL`, a subquery of `IN`, `ANY` or
    /// `ALL`. Its `JSON` columns are then read as the input reads them
    /// (`json` module).
    compared_rows: bool,
    /// Steps of the report.
    steps: Vec<Step>,
    /// Untranslatable constructs.
    errors: Vec<Untranslatable>,
}

/// `SELECT` block folded from the plan.
struct Block<'p> {
    /// `LIMIT`.
    limit: Option<&'p Limit>,
    /// `DISTINCT`.
    distinct: bool,
    /// Select list.
    project: &'p Project,
    /// `ORDER BY`, on the source.
    sort: Option<&'p Sort>,
    /// Window functions and `WINDOW`.
    window: Option<&'p Window>,
    /// `HAVING`.
    having: Option<&'p Filter>,
    /// `GROUP BY` and aggregate functions.
    aggregate: Option<&'p Aggregate>,
    /// `WHERE`.
    filter: Option<&'p Filter>,
    /// `FROM`.
    source: &'p Plan,
}

impl<'p> Block<'p> {
    /// Folds `plan` into a `SELECT` block, `None` when it has no projection.
    fn fold(plan: &'p Plan) -> Option<Self> {
        let mut node = plan;
        let limit = match node {
            Plan::Limit(limit) => {
                node = &limit.input;
                Some(limit)
            }
            _ => None,
        };
        let distinct = match node {
            Plan::Distinct(distinct) => {
                node = &distinct.input;
                true
            }
            _ => false,
        };
        let Plan::Project(project) = node else {
            return None;
        };
        node = &project.input;
        let sort = match node {
            Plan::Sort(sort) => {
                node = &sort.input;
                Some(sort)
            }
            _ => None,
        };
        let window = match node {
            Plan::Window(window) => {
                node = &window.input;
                Some(window)
            }
            _ => None,
        };
        let having = match node {
            Plan::Filter(filter) if filter.form == FilterForm::Having => {
                node = &filter.input;
                Some(filter)
            }
            _ => None,
        };
        let aggregate = match node {
            Plan::Aggregate(aggregate) => {
                node = &aggregate.input;
                Some(aggregate)
            }
            _ => None,
        };
        let filter = match node {
            Plan::Filter(filter) if filter.form == FilterForm::Where => {
                node = &filter.input;
                Some(filter)
            }
            _ => None,
        };
        Some(Self {
            limit,
            distinct,
            project,
            sort,
            window,
            having,
            aggregate,
            filter,
            source: node,
        })
    }
}

impl Emitter<'_> {
    /// Appends `text`.
    fn write(&mut self, text: &str) {
        self.out.push_str(text);
    }

    /// Writes the comments of the input placed before byte `position`.
    fn comments_before(&mut self, position: usize) {
        while let Some(&(span, kind)) = self.source.comments.get(self.next_comment) {
            if span.start >= position {
                return;
            }
            self.next_comment += 1;
            if !self.out.is_empty() && !self.out.ends_with([' ', '\n', '(']) {
                self.write(" ");
            }
            let text = &self.source.text[span.start..span.end];
            self.out.push_str(text);
            match kind {
                CommentKind::Dash | CommentKind::Hash => self.write("\n"),
                _ => self.write(" "),
            }
        }
    }

    /// Writes `text`, the text of the input at `span`, with the comments it
    /// holds, not written again (`'a' /* x */ 'b'`); those before it come
    /// first.
    fn input_text(&mut self, span: Span, text: &str) {
        self.at(Some(span));
        self.write(text);
        while let Some(&(comment, _)) = self.source.comments.get(self.next_comment)
            && comment.start >= span.start
            && comment.end <= span.end
        {
            self.next_comment += 1;
        }
    }

    /// Writes the comments of the input placed before `span`, when known.
    fn at(&mut self, span: Option<Span>) {
        if self.quiet > 0 {
            return;
        }
        if let Some(span) = span {
            self.comments_before(span.start);
        }
    }

    /// Writes the comments not written yet, at the end.
    fn remaining_comments(&mut self) {
        self.comments_before(usize::MAX);
        if self.out.ends_with(' ') {
            self.out.pop();
        }
    }

    /// Writes a clause keyword preceded by a space, the space left out after
    /// a comment that already ends with one.
    fn clause(&mut self, text: &str) {
        if self.out.ends_with([' ', '\n']) {
            self.write(text.trim_start());
        } else {
            self.write(text);
        }
    }

    /// Records a step, once: an emulation that repeats an expression writes
    /// its steps again.
    fn step(&mut self, rule: &'static str, kind: StepKind, message: String, span: Option<Span>) {
        let step = Step {
            rule,
            kind,
            message,
            span,
        };
        if !self.steps.contains(&step) {
            self.steps.push(step);
        }
    }

    /// Records an untranslatable construct, once.
    fn error(&mut self, rule: &'static str, message: &str, span: Option<Span>) {
        let error = Untranslatable {
            rule,
            message: message.to_string(),
            span,
        };
        if !self.errors.contains(&error) {
            self.errors.push(error);
        }
    }

    /// Name of the output dialect with its version, for the messages.
    fn target(&self) -> String {
        format!("{} {}", self.output.dialect.label(), self.output.version)
    }

    /// Tells whether the output dialect has `construct` in its version.
    fn has(&self, construct: Construct) -> bool {
        let dialect = self.output.dialect;
        registry(dialect)
            .check(dialect, construct, self.output.version)
            .is_ok()
    }

    /// Tells whether the input dialect has `construct` in its version.
    fn input_has(&self, construct: Construct) -> bool {
        let dialect = self.source.dialect;
        registry(dialect)
            .check(dialect, construct, self.source.version)
            .is_ok()
    }

    /// Identifier, between backticks when written so or when the output
    /// needs it.
    fn ident(&mut self, ident: &Ident, span: Option<Span>) {
        let needed = needs_quotes(&ident.value, self.output);
        if ident.quoted || needed {
            if needed && !ident.quoted {
                let message = format!(
                    "`{}` is a reserved word or not a plain name in {}: written between backticks",
                    ident.value,
                    self.target()
                );
                self.step("identifier.quoted", StepKind::Rewrite, message, span);
            }
            let escaped = ident.value.replace('`', "``");
            self.write(&format!("`{escaped}`"));
        } else {
            self.write(&ident.value);
        }
    }

    /// Query: `WITH`, then its body. `names_matter` tells whether the names
    /// of its columns are seen (they are not under a column list).
    fn query(&mut self, plan: &Plan, names_matter: bool) {
        match plan {
            Plan::With(with) => {
                // The rows compared are those of the body.
                let compared = std::mem::take(&mut self.compared_rows);
                self.with(with);
                self.compared_rows = compared;
                self.query(&with.body, names_matter);
            }
            _ => self.body(plan, names_matter),
        }
    }

    /// `WITH name (columns) AS (query), ...`, followed by a space.
    fn with(&mut self, with: &With) {
        self.at(with.span);
        self.write(if with.recursive {
            "WITH RECURSIVE "
        } else {
            "WITH "
        });
        for (index, cte) in with.ctes.iter().enumerate() {
            if index > 0 {
                self.write(", ");
            }
            self.at(cte.span);
            self.ident(&cte.name, cte.span);
            if !cte.columns.is_empty() {
                self.write(" (");
                for (position, column) in cte.columns.iter().enumerate() {
                    if position > 0 {
                        self.write(", ");
                    }
                    self.ident(column, cte.span);
                }
                self.write(")");
            }
            self.write(" AS (");
            let top = std::mem::replace(&mut self.top, false);
            if cte.parts.is_empty() {
                self.query(&cte.plan, cte.columns.is_empty());
            } else {
                let plan = self.recursive_plan(cte);
                self.query(&plan, cte.columns.is_empty());
            }
            self.top = top;
            self.write(")");
        }
        self.write(" ");
    }

    /// Body of a query: a `SELECT` block, a `VALUES`, or a query followed
    /// by its `ORDER BY` and its `LIMIT`.
    fn body(&mut self, plan: &Plan, names_matter: bool) {
        // Rows compared by the parent: those of this query, not of its
        // subqueries.
        let compared = std::mem::take(&mut self.compared_rows);
        if let Some(block) = Block::fold(plan) {
            self.select(&block, names_matter, compared);
            return;
        }
        let (limit, inner) = match plan {
            Plan::Limit(limit) => (Some(limit), limit.input.as_ref()),
            _ => (None, plan),
        };
        let (sort, inner) = match inner {
            Plan::Sort(sort) => (Some(sort), sort.input.as_ref()),
            _ => (None, inner),
        };
        match inner {
            Plan::Values(values) if !is_dual(inner) => self.values(values, names_matter),
            Plan::SetOperation(operation) => {
                self.set_operation(operation, names_matter, compared);
            }
            Plan::Scan(_) | Plan::CteRef(_) => self.table_statement(inner, compared),
            _ if sort.is_some() || limit.is_some() => {
                self.compared_rows = compared;
                self.parenthesized(inner, sort, limit, names_matter);
            }
            _ => {
                self.error(
                    "plan.shape",
                    "query shape not supported yet by the emitter of Prisme",
                    inner.span(),
                );
                return;
            }
        }
        if let Some(sort) = sort {
            self.order_by(sort, None);
        }
        if let Some(limit) = limit {
            self.limit(limit);
        }
    }

    /// `SELECT` block, the columns it computes for itself written as what
    /// they stand for, its rows compared by the parent when `compared`.
    fn select(&mut self, block: &Block<'_>, names_matter: bool, compared: bool) {
        self.blocks.push(call::Internal::new(block));
        let top = std::mem::replace(&mut self.top, false);
        self.select_clauses(block, names_matter, top, compared || block.distinct);
        self.top = top;
        self.blocks.pop();
    }

    /// Clauses of a `SELECT` block, which gives the result of the statement
    /// when `top`, its rows compared when `compared`.
    fn select_clauses(&mut self, block: &Block<'_>, names_matter: bool, top: bool, compared: bool) {
        self.at(block.project.span);
        self.write("SELECT ");
        if block.distinct {
            self.write("DISTINCT ");
        }
        for (index, item) in block.project.items.iter().enumerate() {
            if index > 0 {
                self.write(", ");
            }
            match item {
                ProjectItem::Wildcard(wildcard) => {
                    if let Some(qualifier) = &wildcard.qualifier {
                        self.ident(qualifier, None);
                        self.write(".");
                    }
                    self.write("*");
                }
                ProjectItem::Expr(named) => {
                    let start = self.out.len();
                    self.column = top;
                    self.json_as_is = !compared;
                    self.expr(&named.expr, 0);
                    match named.alias {
                        AliasForm::As => {
                            self.write(" AS ");
                            self.ident(&named.name, named.expr.span);
                        }
                        AliasForm::Bare => {
                            self.write(" ");
                            self.ident(&named.name, named.expr.span);
                        }
                        AliasForm::Implicit if names_matter => {
                            self.keep_implicit_name(&named.expr, &named.name, start);
                        }
                        AliasForm::Implicit => {}
                    }
                }
            }
        }
        if !is_dual(block.source) {
            self.at(block.source.span());
            self.clause(" FROM ");
            self.source(block.source);
        }
        if let Some(filter) = block.filter {
            self.at(filter.span);
            self.clause(" WHERE ");
            self.truth_operand(&filter.predicate, 0, truth::Context::Clause("`WHERE`"));
        }
        if let Some(aggregate) = block.aggregate {
            self.group_by(aggregate, block.project, block.having.is_some());
            if aggregate.rollup && block.sort.is_some() && !self.has(Construct::RollupOrderBy) {
                let message = format!(
                    "{} has no {}: not supported yet by Prisme",
                    self.target(),
                    Construct::RollupOrderBy.label()
                );
                self.error("rollup.order", &message, block.project.span);
            }
        }
        if let Some(having) = block.having {
            self.at(having.span);
            self.clause(" HAVING ");
            self.having_aliases(&having.predicate, block.project);
            self.truth_operand(&having.predicate, 0, truth::Context::Clause("`HAVING`"));
        }
        if let Some(window) = block.window {
            self.window_clause(&window.definitions);
        }
        if let Some(sort) = block.sort {
            self.order_by(sort, Some(block.project));
            self.grouped_window_order(block);
        }
        if let Some(limit) = block.limit {
            self.limit(limit);
        }
    }

    /// Keeps the name of a column without alias, which the MySQL family takes
    /// from the text of its expression: when the text written differs from
    /// the text read, an alias gives the name.
    fn keep_implicit_name(&mut self, expr: &prisme_plan::Expr, name: &Ident, start: usize) {
        let value = self.naming_value(expr);
        let text = &self.out[start..];
        let written = match &value.kind {
            // An aggregate or window function the block computes is named
            // after its text, as a column read through a call (`json`
            // module), whose text ends with `)`.
            prisme_plan::ExprKind::Column(column)
                if (column.qualifier.is_some() || !is_internal_name(&column.name.value))
                    && !text.ends_with(')') =>
            {
                column.name.value.clone()
            }
            prisme_plan::ExprKind::Literal(Value::Text(value)) => self.string_name(text, value),
            prisme_plan::ExprKind::Literal(Value::Binary(bytes)) if literal::is_string(text) => {
                self.string_name(text, &String::from_utf8_lossy(bytes))
            }
            _ => text.to_string(),
        };
        if written == name.value {
            return;
        }
        self.write(" AS ");
        let quoted = Ident::quoted(name.value.clone());
        self.ident(&quoted, None);
        let message = format!(
            "the column takes its name from the text of its expression, `{}`: kept by an alias",
            name.value
        );
        self.step(
            "column.implicit-name",
            StepKind::Rewrite,
            message,
            expr.span,
        );
    }

    /// `TABLE t`, or `SELECT * FROM t` when the output has no `TABLE`
    /// statement; its rows compared by the parent when `compared`.
    fn table_statement(&mut self, plan: &Plan, compared: bool) {
        if compared {
            self.json_rows(plan);
        }
        if self.has(Construct::TableStatement) {
            self.write("TABLE ");
        } else {
            self.write("SELECT * FROM ");
            let message = format!(
                "{} has no `TABLE` statement: written `SELECT * FROM`",
                self.target()
            );
            self.step("table.select", StepKind::Rewrite, message, plan.span());
        }
        self.source(plan);
    }

    /// `UNION`, `EXCEPT` or `INTERSECT`, its operands between parentheses
    /// when their nesting needs it; its rows compared by the parent when
    /// `compared`.
    fn set_operation(&mut self, operation: &SetOperation, names_matter: bool, compared: bool) {
        let (keyword, construct) = match operation.operator {
            SetOperator::UnionAll => ("UNION ALL", None),
            SetOperator::UnionDistinct => ("UNION", None),
            SetOperator::MinusAll => ("EXCEPT ALL", Some(Construct::IntersectExceptAll)),
            SetOperator::MinusDistinct => ("EXCEPT", Some(Construct::IntersectExcept)),
            SetOperator::IntersectAll => ("INTERSECT ALL", Some(Construct::IntersectExceptAll)),
            SetOperator::IntersectDistinct => ("INTERSECT", Some(Construct::IntersectExcept)),
        };
        if let Some(construct) = construct
            && !self.has(construct)
        {
            let message = format!(
                "{} has no `{keyword}`: not supported yet by Prisme",
                self.target()
            );
            self.error("set.operator", &message, operation.span);
        }
        // Every set operation but `UNION ALL` compares the rows of its
        // operands.
        let compared = compared || operation.operator != SetOperator::UnionAll;
        self.compared_rows = compared;
        self.set_operand(&operation.left, operation.operator, false, names_matter);
        self.write(&format!(" {keyword} "));
        self.compared_rows = compared;
        self.set_operand(&operation.right, operation.operator, true, false);
    }

    /// Operand of a set operation of `parent`, on the right side or not.
    fn set_operand(&mut self, plan: &Plan, parent: SetOperator, right: bool, names_matter: bool) {
        let nested = self
            .written_set_operator(plan, names_matter)
            .map(precedence);
        let parenthesized = match nested {
            Some(level) if right => level <= precedence(parent),
            Some(level) => level < precedence(parent),
            None => match plan {
                Plan::With(_) | Plan::Sort(_) | Plan::Limit(_) => true,
                _ => Block::fold(plan)
                    .is_some_and(|block| block.sort.is_some() || block.limit.is_some()),
            },
        };
        if !parenthesized {
            self.query(plan, names_matter);
            return;
        }
        if right && nested.is_some() && !self.has(Construct::NestedSetOperation) {
            self.nested_set_operation(plan);
            return;
        }
        self.write("(");
        self.query(plan, names_matter);
        self.write(")");
    }

    /// Set operator `plan` is written with at its top, if any: a set
    /// operation, or a `VALUES` of several rows emulated by a union.
    fn written_set_operator(&self, plan: &Plan, names_matter: bool) -> Option<SetOperator> {
        match plan {
            Plan::SetOperation(operation) => Some(operation.operator),
            Plan::Values(values)
                if values.rows.len() > 1 && self.values_union(values, names_matter).is_some() =>
            {
                Some(SetOperator::UnionAll)
            }
            _ => None,
        }
    }

    /// Set operation as a right operand, toward an output that refuses it
    /// between parentheses: read from a derived table, whose columns need
    /// distinct names.
    fn nested_set_operation(&mut self, plan: &Plan) {
        let message = format!(
            "{} has no {}",
            self.target(),
            Construct::NestedSetOperation.label()
        );
        if self.has_duplicate_names(plan) {
            let message =
                format!("{message}, and its columns have the same names: not supported yet");
            self.error("set.nested", &message, plan.span());
        }
        self.write("SELECT * FROM (");
        self.query(plan, true);
        self.write(") AS `d`");
        self.step(
            "set.nested",
            StepKind::Emulation,
            format!("{message}: emulated by a derived table"),
            plan.span(),
        );
    }

    /// `ORDER BY`. The keys of a `SELECT` block designate a column of its
    /// select list as written: by its alias, by its position.
    fn order_by(&mut self, sort: &Sort, project: Option<&Project>) {
        self.at(sort.keys.first().and_then(|key| key.expr.span));
        self.clause(" ORDER BY ");
        self.sort_keys(&sort.keys, project);
    }

    /// Keys of an `ORDER BY`, separated by commas; those of a `SELECT`
    /// block designate a column of `project` as written.
    fn sort_keys(&mut self, keys: &[SortKey], project: Option<&Project>) {
        for (index, key) in keys.iter().enumerate() {
            if index > 0 {
                self.write(", ");
            }
            let smallest = Nulls::as_smallest(key.direction);
            if key.nulls != smallest {
                // NULL is the smallest value in the MySQL family: a key
                // that wants them elsewhere sorts on `IS NULL` first.
                self.expr(&key.expr, 6);
                self.write(match key.nulls {
                    Nulls::Last => " IS NULL, ",
                    Nulls::First => " IS NOT NULL, ",
                });
                let message = format!(
                    "{} has no `NULLS {}`: emulated by a key on `IS NULL`, which an index cannot serve",
                    self.target(),
                    match key.nulls {
                        Nulls::First => "FIRST",
                        Nulls::Last => "LAST",
                    }
                );
                self.step("sort.nulls", StepKind::Emulation, message, key.expr.span);
            }
            self.sort_value(key, project);
            match key.direction {
                Direction::Descending => self.write(" DESC"),
                Direction::Ascending if key.form.direction => self.write(" ASC"),
                Direction::Ascending => {}
            }
        }
    }

    /// Value of a sort key, by alias or position when written so.
    fn sort_value(&mut self, key: &SortKey, project: Option<&Project>) {
        let items = project.map_or(&[][..], |project| project.items.as_slice());
        let found = items
            .iter()
            .enumerate()
            .find_map(|(index, item)| match item {
                ProjectItem::Expr(named) if named.expr == key.expr => Some((index, named)),
                _ => None,
            });
        // A `JSON` the select list keeps as is is sorted by an expression of
        // its text (`json` module).
        let json = self.json_differs(&key.expr);
        match (key.form.reference, found) {
            (SortReference::Alias, Some((_, named)))
                if !json && named.alias != AliasForm::Implicit =>
            {
                self.ident(&named.name, key.expr.span);
            }
            (SortReference::Position, Some((index, _)))
                if !json
                    && items
                        .iter()
                        .all(|item| matches!(item, ProjectItem::Expr(_))) =>
            {
                self.at(key.expr.span);
                self.write(&(index + 1).to_string());
            }
            _ => {
                let expr = self.sort_aliases(&key.expr);
                self.column = true;
                self.expr(&expr, 0);
            }
        }
    }

    /// `LIMIT`, in the form written.
    fn limit(&mut self, limit: &Limit) {
        self.at(limit.span);
        self.clause(" LIMIT ");
        let Some(count) = &limit.count else {
            // The MySQL family has no `OFFSET` without `LIMIT`: the largest
            // count stands for "all the rows".
            self.write("18446744073709551615");
            if let Some(offset) = &limit.offset {
                self.write(" OFFSET ");
                self.expr(offset, 0);
            }
            let message = format!(
                "{} has no `OFFSET` without `LIMIT`: the largest count stands for all the rows",
                self.target()
            );
            self.step("limit.count", StepKind::Emulation, message, limit.span);
            return;
        };
        match (&limit.offset, limit.form) {
            (Some(offset), LimitForm::Comma) => {
                self.expr(offset, 0);
                self.write(", ");
                self.expr(count, 0);
            }
            (Some(offset), LimitForm::Offset) => {
                self.expr(count, 0);
                self.write(" OFFSET ");
                self.expr(offset, 0);
            }
            (None, _) => self.expr(count, 0),
        }
    }

    /// `VALUES`, in the form of the output dialect, or emulated by a union
    /// of `SELECT` when the output names its columns otherwise or has no
    /// `VALUES` statement.
    fn values(&mut self, values: &Values, names_matter: bool) {
        self.at(values.span);
        if let Some(message) = self.values_union(values, names_matter) {
            self.values_as_union(values);
            self.step("values.union", StepKind::Emulation, message, values.span);
            return;
        }
        let form = match self.output.dialect {
            Dialect::MySql => ValuesForm::Row,
            Dialect::MariaDb => ValuesForm::Bare,
        };
        if values.form != form {
            let message = match form {
                ValuesForm::Row => "MySQL writes the rows of `VALUES` `ROW (...)`",
                ValuesForm::Bare => "MariaDB writes the rows of `VALUES` `(...)`, without `ROW`",
            };
            self.step(
                "values.form",
                StepKind::Rewrite,
                message.to_string(),
                values.span,
            );
        }
        self.write("VALUES ");
        for (index, row) in values.rows.iter().enumerate() {
            if index > 0 {
                self.write(", ");
            }
            self.write(match form {
                ValuesForm::Row => "ROW(",
                ValuesForm::Bare => "(",
            });
            self.expr_list(row);
            self.write(")");
        }
    }

    /// Reason to emulate `values` by a union of `SELECT`: the output has no
    /// `VALUES` statement, or names its columns otherwise when their names
    /// matter; `None` when the output writes it.
    fn values_union(&self, values: &Values, names_matter: bool) -> Option<String> {
        let construct = match self.output.dialect {
            Dialect::MySql => Construct::ValuesRow,
            Dialect::MariaDb => Construct::ValuesBare,
        };
        if !self.has(construct) {
            return Some(format!(
                "{} has no `VALUES` statement: emulated by a union of `SELECT`",
                self.target()
            ));
        }
        let names: Vec<&str> = values
            .schema
            .columns
            .iter()
            .map(|column| column.name.value.as_str())
            .collect();
        let natural = self.natural_values_names(values);
        let renamed = names_matter && natural.as_deref().is_none_or(|natural| natural != names);
        renamed.then(|| {
            format!(
                "{} names the columns of `VALUES` otherwise: emulated by a union of `SELECT` that keeps their names",
                self.target()
            )
        })
    }

    /// Names the output dialect gives to the columns of `values`, `None`
    /// when unknown.
    fn natural_values_names(&self, values: &Values) -> Option<Vec<String>> {
        match self.output.dialect {
            Dialect::MySql => Some(
                (0..values.schema.len())
                    .map(|index| format!("column_{index}"))
                    .collect(),
            ),
            Dialect::MariaDb => values.rows.first()?.iter().map(literal_name).collect(),
        }
    }

    /// `SELECT ... UNION ALL SELECT ...`, the first row naming the columns.
    fn values_as_union(&mut self, values: &Values) {
        for (index, row) in values.rows.iter().enumerate() {
            if index > 0 {
                self.write(" UNION ALL ");
            }
            self.write("SELECT ");
            for (position, value) in row.iter().enumerate() {
                if position > 0 {
                    self.write(", ");
                }
                self.expr(value, 0);
                if index == 0 {
                    self.write(" AS ");
                    let name = Ident::quoted(values.schema.columns[position].name.value.clone());
                    self.ident(&name, None);
                }
            }
        }
    }

    /// Expressions separated by commas.
    fn expr_list(&mut self, exprs: &[prisme_plan::Expr]) {
        for (index, expr) in exprs.iter().enumerate() {
            if index > 0 {
                self.write(", ");
            }
            self.expr(expr, 0);
        }
    }
}

/// Precedence of a set operator: `INTERSECT` binds tighter than `UNION`
/// and `EXCEPT`.
const fn precedence(operator: SetOperator) -> u8 {
    match operator {
        SetOperator::IntersectAll | SetOperator::IntersectDistinct => 2,
        _ => 1,
    }
}

/// Name MariaDB gives to a column of `VALUES` holding `expr` in its first
/// row, `None` when it is not a literal.
fn literal_name(expr: &prisme_plan::Expr) -> Option<String> {
    let inner = match &expr.kind {
        prisme_plan::ExprKind::ToInteger(inner) => inner,
        _ => expr,
    };
    match &inner.kind {
        prisme_plan::ExprKind::Literal(value) => match value {
            Value::Null => Some("NULL".to_string()),
            Value::Boolean(true) => Some("TRUE".to_string()),
            Value::Boolean(false) => Some("FALSE".to_string()),
            Value::Integer(value) => Some(value.to_string()),
            Value::Decimal(value) => Some(value.as_str().to_string()),
            Value::Text(value) => Some(value.clone()),
            _ => None,
        },
        _ => None,
    }
}

/// Tells whether a name needs backticks in the output: not a plain word,
/// starting with a digit, or reserved.
fn needs_quotes(name: &str, output: &OutputDialect) -> bool {
    let plain = name
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'$' || byte >= 0x80);
    !plain
        || name.is_empty()
        || name.starts_with(|character: char| character.is_ascii_digit())
        || is_reserved(name, output.dialect, output.version)
}
