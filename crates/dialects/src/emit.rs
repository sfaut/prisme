//! Emitter of the MySQL family: logical plan to SQL text of MySQL or
//! MariaDB, aware of the version of the server (`docs/DEV-DIALECTS.md`,
//! step 4).
//!
//! The emitter writes the form of the input when the output dialect accepts
//! it (section 2.3), otherwise another form, an emulation or an error, each
//! one with its step in the report. The plan of a `SELECT` is folded back
//! into a block, `Limit(Distinct(Project(Sort(Filter(source)))))`.
//!
//! Literals are written as in the input when the output reads them the same
//! way (same SQL mode for the quotes and the backslashes), otherwise in a
//! canonical spelling of the same value. The comments of the input are
//! written again, each one before the first construct that follows it in
//! the input.

mod expr;

#[cfg(test)]
mod tests;

use prisme_plan::{
    AliasForm, Direction, Ident, Limit, LimitForm, Nulls, Plan, Project, ProjectItem, Sort,
    SortReference, Span, Statement, Values, ValuesForm, With,
};
use prisme_types::Value;

use crate::Dialect;
use crate::build::is_dual;
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
    /// `WHERE`.
    filter: Option<&'p prisme_plan::Filter>,
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
        let filter = match node {
            Plan::Filter(filter) => {
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

    /// Writes the comments of the input placed before `span`, when known.
    fn at(&mut self, span: Option<Span>) {
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

    /// Records a step.
    fn step(&mut self, rule: &'static str, kind: StepKind, message: String, span: Option<Span>) {
        self.steps.push(Step {
            rule,
            kind,
            message,
            span,
        });
    }

    /// Records an untranslatable construct.
    fn error(&mut self, rule: &'static str, message: &str, span: Option<Span>) {
        self.errors.push(Untranslatable {
            rule,
            message: message.to_string(),
            span,
        });
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
                self.with(with);
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
            self.query(&cte.plan, cte.columns.is_empty());
            self.write(")");
        }
        self.write(" ");
    }

    /// Body of a query: a `SELECT` block, a `VALUES`, or a query followed
    /// by its `ORDER BY` and its `LIMIT`.
    fn body(&mut self, plan: &Plan, names_matter: bool) {
        if let Some(block) = Block::fold(plan) {
            self.select(&block, names_matter);
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
            _ if sort.is_some() || limit.is_some() => {
                self.write("(");
                self.query(inner, names_matter);
                self.write(")");
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

    /// `SELECT` block.
    fn select(&mut self, block: &Block<'_>, names_matter: bool) {
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
            self.expr(&filter.predicate, 0);
        }
        if let Some(sort) = block.sort {
            self.order_by(sort, Some(block.project));
        }
        if let Some(limit) = block.limit {
            self.limit(limit);
        }
    }

    /// Keeps the name of a column without alias, which the MySQL family takes
    /// from the text of its expression: when the text written differs from
    /// the text read, an alias gives the name.
    fn keep_implicit_name(&mut self, expr: &prisme_plan::Expr, name: &Ident, start: usize) {
        let written = match &expr.kind {
            prisme_plan::ExprKind::Column(column) => column.name.value.clone(),
            prisme_plan::ExprKind::Literal(Value::Text(value)) => value.clone(),
            _ => self.out[start..].to_string(),
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

    /// Source of a `FROM`.
    fn source(&mut self, plan: &Plan) {
        match plan {
            Plan::Scan(scan) => {
                self.at(scan.span);
                if let Some(database) = &scan.table.database {
                    self.ident(database, scan.span);
                    self.write(".");
                }
                self.ident(&scan.table.name, scan.span);
                self.table_alias(scan.alias.as_ref(), scan.span);
            }
            Plan::CteRef(cte) => {
                self.at(cte.span);
                self.ident(&cte.name, cte.span);
                self.table_alias(cte.alias.as_ref(), cte.span);
            }
            _ => self.error(
                "plan.shape",
                "source not supported yet by the emitter of Prisme",
                plan.span(),
            ),
        }
    }

    /// Alias of a table.
    fn table_alias(&mut self, alias: Option<&prisme_plan::TableAlias>, span: Option<Span>) {
        let Some(alias) = alias else {
            return;
        };
        self.write(match alias.form {
            AliasForm::Bare => " ",
            _ => " AS ",
        });
        self.ident(&alias.name, span);
    }

    /// `ORDER BY`. The keys of a `SELECT` block designate a column of its
    /// select list as written: by its alias, by its position.
    fn order_by(&mut self, sort: &Sort, project: Option<&Project>) {
        self.at(sort.keys.first().and_then(|key| key.expr.span));
        self.clause(" ORDER BY ");
        let mut first = true;
        for key in &sort.keys {
            if !first {
                self.write(", ");
            }
            first = false;
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
    fn sort_value(&mut self, key: &prisme_plan::SortKey, project: Option<&Project>) {
        let items = project.map_or(&[][..], |project| project.items.as_slice());
        let found = items
            .iter()
            .enumerate()
            .find_map(|(index, item)| match item {
                ProjectItem::Expr(named) if named.expr == key.expr => Some((index, named)),
                _ => None,
            });
        match (key.form.reference, found) {
            (SortReference::Alias, Some((_, named))) if named.alias != AliasForm::Implicit => {
                self.ident(&named.name, key.expr.span);
            }
            (SortReference::Position, Some((index, _)))
                if items
                    .iter()
                    .all(|item| matches!(item, ProjectItem::Expr(_))) =>
            {
                self.at(key.expr.span);
                self.write(&(index + 1).to_string());
            }
            _ => self.expr(&key.expr, 0),
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
        let (construct, form) = match self.output.dialect {
            Dialect::MySql => (Construct::ValuesRow, ValuesForm::Row),
            Dialect::MariaDb => (Construct::ValuesBare, ValuesForm::Bare),
        };
        let names: Vec<&str> = values
            .schema
            .columns
            .iter()
            .map(|column| column.name.value.as_str())
            .collect();
        let natural = self.natural_values_names(values);
        let renamed = names_matter && natural.as_deref().is_none_or(|natural| natural != names);
        if !self.has(construct) || renamed {
            self.values_as_union(values);
            let message = if renamed {
                format!(
                    "{} names the columns of `VALUES` otherwise: emulated by a union of `SELECT` that keeps their names",
                    self.target()
                )
            } else {
                format!(
                    "{} has no `VALUES` statement: emulated by a union of `SELECT`",
                    self.target()
                )
            };
            self.step("values.union", StepKind::Emulation, message, values.span);
            return;
        }
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
