//! Sources of a `FROM` written in the MySQL family: tables with their
//! partitions and index hints, common table expressions, derived tables and
//! joins.
//!
//! The comma joins loosest, and a join reads its right side greedily: a
//! comma join under another join, and a join as a right side, are written
//! between parentheses (`(a, b) JOIN c`, `a JOIN (b JOIN c ON x) ON y`); a
//! join as a left side is written as is (`a JOIN b JOIN c`).

use prisme_plan::{
    AliasForm, Derived, IndexHint, IndexHintAction, IndexHintScope, Join, JoinForm, JoinKind, Plan,
    Scan, Span, TableAlias,
};

use super::Emitter;
use super::truth::Context;
use crate::report::StepKind;
use crate::versions::Construct;

/// Tells whether `plan` is a comma join.
fn is_comma(plan: &Plan) -> bool {
    matches!(plan, Plan::Join(join) if comma(join))
}

/// Tells whether `join` is written with a comma: a cross join written so.
fn comma(join: &Join) -> bool {
    join.form == JoinForm::Comma && join.kind == JoinKind::Cross
}

impl Emitter<'_> {
    /// Source of a `FROM`.
    pub(super) fn source(&mut self, plan: &Plan) {
        match plan {
            Plan::Scan(scan) => self.scan(scan),
            Plan::CteRef(cte) => {
                self.at(cte.span);
                self.ident(&cte.name, cte.span);
                self.table_alias(cte.alias.as_ref(), cte.span);
            }
            Plan::Derived(derived) => self.derived(derived),
            Plan::Join(join) => self.join(join),
            _ => self.error(
                "plan.shape",
                "source not supported yet by the emitter of Prisme",
                plan.span(),
            ),
        }
    }

    /// Table, with its partitions, its alias and its index hints.
    fn scan(&mut self, scan: &Scan) {
        self.at(scan.span);
        if let Some(database) = &scan.table.database {
            self.ident(database, scan.span);
            self.write(".");
        }
        self.ident(&scan.table.name, scan.span);
        if !scan.partitions.is_empty() {
            self.write(" PARTITION (");
            self.idents(&scan.partitions, scan.span);
            self.write(")");
        }
        self.table_alias(scan.alias.as_ref(), scan.span);
        for hint in &scan.index_hints {
            self.index_hint(hint, scan.span);
        }
    }

    /// `USE INDEX FOR JOIN (i, j)`, after a space.
    fn index_hint(&mut self, hint: &IndexHint, span: Option<Span>) {
        self.write(match hint.action {
            IndexHintAction::Use => " USE",
            IndexHintAction::Force => " FORCE",
            IndexHintAction::Ignore => " IGNORE",
        });
        self.write(if hint.key { " KEY" } else { " INDEX" });
        self.write(match hint.scope {
            None => "",
            Some(IndexHintScope::Join) => " FOR JOIN",
            Some(IndexHintScope::OrderBy) => " FOR ORDER BY",
            Some(IndexHintScope::GroupBy) => " FOR GROUP BY",
        });
        self.write(" (");
        for (index, name) in hint.indexes.iter().enumerate() {
            if index > 0 {
                self.write(", ");
            }
            // `PRIMARY`, a reserved word, names the primary key as is.
            if !name.quoted && name.value.eq_ignore_ascii_case("PRIMARY") {
                self.write(&name.value);
            } else {
                self.ident(name, span);
            }
        }
        self.write(")");
    }

    /// Names separated by commas.
    fn idents(&mut self, idents: &[prisme_plan::Ident], span: Option<Span>) {
        for (index, ident) in idents.iter().enumerate() {
            if index > 0 {
                self.write(", ");
            }
            self.ident(ident, span);
        }
    }

    /// Alias of a table.
    pub(super) fn table_alias(&mut self, alias: Option<&TableAlias>, span: Option<Span>) {
        let Some(alias) = alias else {
            return;
        };
        self.write(match alias.form {
            AliasForm::Bare => " ",
            _ => " AS ",
        });
        self.ident(&alias.name, span);
    }

    /// Derived table: `[LATERAL] (query) AS d (columns)`, its column list
    /// emulated by a common table expression when the output has none.
    fn derived(&mut self, derived: &Derived) {
        self.at(derived.span);
        if derived.lateral {
            if !self.has(Construct::Lateral) {
                let message = format!(
                    "{} has no `LATERAL`: not supported yet by Prisme",
                    self.target()
                );
                self.error("derived.lateral", &message, derived.span);
            }
            self.write("LATERAL ");
        }
        if derived.outer_references && !self.has(Construct::DerivedOuterReference) {
            let message = format!(
                "{} reads no column of the enclosing queries in a derived table: not supported yet by Prisme",
                self.target()
            );
            self.error("derived.outer-reference", &message, derived.span);
        }
        let alias = &derived.alias;
        if !alias.columns.is_empty() && !self.has(Construct::DerivedColumnList) {
            self.write("(WITH ");
            self.ident(&alias.name, derived.span);
            self.write(" (");
            self.idents(&alias.columns, derived.span);
            self.write(") AS (");
            self.query(&derived.input, false);
            self.write(") SELECT * FROM ");
            self.ident(&alias.name, derived.span);
            self.write(")");
            self.table_alias(Some(alias), derived.span);
            let message = format!(
                "{} has no {}: emulated by a common table expression",
                self.target(),
                Construct::DerivedColumnList.label()
            );
            self.step(
                "derived.columns",
                StepKind::Emulation,
                message,
                derived.span,
            );
            return;
        }
        self.write("(");
        self.query(&derived.input, alias.columns.is_empty());
        self.write(")");
        self.table_alias(Some(alias), derived.span);
        if !alias.columns.is_empty() {
            self.write(" (");
            self.idents(&alias.columns, derived.span);
            self.write(")");
        }
    }

    /// Join of two sources, with its condition.
    fn join(&mut self, join: &Join) {
        let natural = join.using.as_ref().is_some_and(|using| using.natural);
        let comma = comma(join);
        self.join_side(&join.left, !comma && is_comma(&join.left));
        let keyword = match join.kind {
            JoinKind::Inner | JoinKind::Cross => match join.form {
                JoinForm::Comma if comma => ",",
                JoinForm::Straight if join.using.is_none() => "STRAIGHT_JOIN",
                JoinForm::Inner => "INNER JOIN",
                JoinForm::Cross if !natural => "CROSS JOIN",
                _ => "JOIN",
            },
            JoinKind::Left if join.form == JoinForm::Outer => "LEFT OUTER JOIN",
            JoinKind::Left => "LEFT JOIN",
            JoinKind::Right if join.form == JoinForm::Outer => "RIGHT OUTER JOIN",
            JoinKind::Right => "RIGHT JOIN",
            JoinKind::Semi | JoinKind::Anti => {
                let message = format!(
                    "{} has no semi-join or anti-join operator: not supported yet by Prisme",
                    self.target()
                );
                self.error("join.kind", &message, join.span);
                "JOIN"
            }
        };
        if comma {
            self.write(", ");
        } else if natural {
            self.write(&format!(" NATURAL {keyword} "));
        } else {
            self.write(&format!(" {keyword} "));
        }
        let right_parenthesized =
            matches!(&*join.right, Plan::Join(_)) && (!comma || is_comma(&join.right));
        self.join_side(&join.right, right_parenthesized);
        self.json_using(join);
        match (&join.using, &join.condition) {
            (Some(using), _) if !using.natural => {
                self.write(" USING (");
                self.idents(&using.columns, join.span);
                self.write(")");
            }
            (None, Some(condition)) => {
                self.write(" ON ");
                self.truth_operand(condition, 0, Context::Clause("`ON`"));
            }
            _ => {}
        }
    }

    /// Side of a join, between parentheses or not.
    fn join_side(&mut self, plan: &Plan, parenthesized: bool) {
        if parenthesized {
            self.write("(");
            self.source(plan);
            self.write(")");
        } else {
            self.source(plan);
        }
    }
}
