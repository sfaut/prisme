//! `FROM` of the MySQL family: tables, derived tables and joins.
//!
//! The comma joins loosest: `a, b JOIN c ON x` is `a, (b JOIN c ON x)`. A
//! join reads its right side greedily, its own joins included, as MySQL and
//! MariaDB do: `a JOIN b JOIN c ON x ON y` is `a JOIN (b JOIN c ON x) ON
//! y`. An inner join left without condition then joins the leftmost table
//! of that right side: `a JOIN b JOIN c ON x` is `(a JOIN b) JOIN c ON x`,
//! which the servers accept with `x` naming `a`. A `NATURAL` join reads a
//! single table on its right.

use prisme_plan::{IndexHint, IndexHintAction, IndexHintScope, JoinForm, Span};

use super::{ParseError, Parser};
use crate::lexer::{Punct, TokenKind};
use crate::sql::ast::{Join, JoinConstraint, JoinKind, TableRef};
use crate::versions::Construct;

/// Operator of a join, as read.
#[derive(Debug, Clone, Copy)]
struct JoinOperator {
    /// Inner, left or right.
    kind: JoinKind,
    /// Keywords.
    form: JoinForm,
    /// `NATURAL`.
    natural: bool,
}

impl Parser<'_> {
    /// Sources of a `FROM`, separated by commas.
    pub(super) fn table_refs(&mut self) -> Result<TableRef, ParseError> {
        let mut left = self.table_reference()?;
        while self.eat_punct(Punct::Comma).is_some() {
            let right = self.table_reference()?;
            let span = Span::new(left.span().start, right.span().end);
            left = TableRef::Join(Box::new(Join {
                left,
                right,
                kind: JoinKind::Inner,
                form: JoinForm::Comma,
                natural: false,
                constraint: None,
                span,
            }));
        }
        Ok(left)
    }

    /// Source followed by its joins.
    fn table_reference(&mut self) -> Result<TableRef, ParseError> {
        self.nested(|parser| {
            let mut left = parser.table_factor()?;
            while let Some(operator) = parser.join_operator() {
                left = parser.join(left, operator)?;
            }
            Ok(left)
        })
    }

    /// Consumes the operator of a join that comes next.
    fn join_operator(&mut self) -> Option<JoinOperator> {
        let at = |parser: &Self, offset: usize, keyword: &str| {
            parser.is_word(parser.peek_at(offset), keyword)
        };
        let natural = at(self, 0, "NATURAL");
        let first = usize::from(natural);
        let (kind, form, length) = if at(self, first, "JOIN") {
            (JoinKind::Inner, JoinForm::Join, 1)
        } else if at(self, first, "INNER") && at(self, first + 1, "JOIN") {
            (JoinKind::Inner, JoinForm::Inner, 2)
        } else if !natural && at(self, 0, "CROSS") && at(self, 1, "JOIN") {
            (JoinKind::Inner, JoinForm::Cross, 2)
        } else if !natural && at(self, 0, "STRAIGHT_JOIN") {
            (JoinKind::Inner, JoinForm::Straight, 1)
        } else {
            let kind = if at(self, first, "LEFT") {
                JoinKind::Left
            } else if at(self, first, "RIGHT") {
                JoinKind::Right
            } else {
                return None;
            };
            if at(self, first + 1, "JOIN") {
                (kind, JoinForm::Join, 2)
            } else if at(self, first + 1, "OUTER") && at(self, first + 2, "JOIN") {
                (kind, JoinForm::Outer, 3)
            } else {
                return None;
            }
        };
        self.pos += first + length;
        Some(JoinOperator {
            kind,
            form,
            natural,
        })
    }

    /// Right side and condition of a join of `left`.
    fn join(&mut self, left: TableRef, operator: JoinOperator) -> Result<TableRef, ParseError> {
        let right = if operator.natural {
            self.table_factor()?
        } else {
            self.table_reference()?
        };
        let constraint = if operator.natural {
            None
        } else if self.eat_keyword("ON").is_some() {
            Some(JoinConstraint::On(self.expr()?))
        } else if operator.form != JoinForm::Straight && self.eat_keyword("USING").is_some() {
            Some(JoinConstraint::Using(self.name_list()?))
        } else {
            None
        };
        if constraint.is_none() && !operator.natural {
            if operator.kind != JoinKind::Inner {
                return Err(self.expected("`ON` or `USING`"));
            }
            if matches!(right, TableRef::Join(_)) {
                return Ok(rotate(left, operator, right));
            }
        }
        let span = Span::new(left.span().start, self.span_from(right.span().start).end);
        Ok(TableRef::Join(Box::new(Join {
            left,
            right,
            kind: operator.kind,
            form: operator.form,
            natural: operator.natural,
            constraint,
            span,
        })))
    }

    /// Table, derived table, or sources between parentheses.
    fn table_factor(&mut self) -> Result<TableRef, ParseError> {
        let start = self.here().start;
        if let Some(span) = self.eat_keyword("LATERAL") {
            self.check(Construct::Lateral, span)?;
            if !self.at_punct(Punct::LeftParen) {
                return Err(self.expected("a subquery between parentheses"));
            }
            return self.derived(start, true);
        }
        if self.at_punct(Punct::LeftParen) {
            if self.query_in_parentheses() {
                return self.derived(start, false);
            }
            self.advance();
            let inner = self.table_refs()?;
            self.expect_punct(Punct::RightParen)?;
            return Ok(TableRef::Nested {
                inner: Box::new(inner),
                span: self.span_from(start),
            });
        }
        if self.at_keyword("JSON_TABLE")
            && self
                .peek_at(1)
                .is_some_and(|token| token.kind == TokenKind::Punct(Punct::LeftParen))
        {
            return Err(self.unsupported("`JSON_TABLE`", self.here()));
        }
        let name = self.object_name(2)?;
        let partitions = if self.eat_keyword("PARTITION").is_some() {
            self.name_list()?
        } else {
            Vec::new()
        };
        let alias = self.alias(false)?;
        let index_hints = self.index_hints()?;
        Ok(TableRef::Named {
            name,
            partitions,
            alias,
            index_hints,
            span: self.span_from(start),
        })
    }

    /// Tells whether the parentheses that come next hold a query: their
    /// first token after the opening ones starts a query.
    pub(super) fn query_in_parentheses(&self) -> bool {
        let mut offset = 0;
        while self
            .peek_at(offset)
            .is_some_and(|token| token.kind == TokenKind::Punct(Punct::LeftParen))
        {
            offset += 1;
        }
        offset > 0 && self.starts_query(offset)
    }

    /// Tells whether the token `offset` positions ahead starts a query.
    pub(super) fn starts_query(&self, offset: usize) -> bool {
        ["SELECT", "WITH", "VALUES", "TABLE"]
            .iter()
            .any(|keyword| self.is_word(self.peek_at(offset), keyword))
    }

    /// `[LATERAL] (query) [AS] alias [(columns)]`, from `(`.
    fn derived(&mut self, start: usize, lateral: bool) -> Result<TableRef, ParseError> {
        self.expect_punct(Punct::LeftParen)?;
        let query = self.query()?;
        self.expect_punct(Punct::RightParen)?;
        let Some(alias) = self.alias(false)? else {
            return Err(ParseError {
                message: "every derived table must have its own alias".to_string(),
                span: self.span_from(start),
            });
        };
        let columns = if self.at_punct(Punct::LeftParen) {
            self.check(Construct::DerivedColumnList, self.here())?;
            self.name_list()?
        } else {
            Vec::new()
        };
        Ok(TableRef::Derived {
            query: Box::new(query),
            lateral,
            alias,
            columns,
            span: self.span_from(start),
        })
    }

    /// Index hints after a table: `USE INDEX (i)`, `IGNORE KEY FOR JOIN (i)`.
    fn index_hints(&mut self) -> Result<Vec<IndexHint>, ParseError> {
        let mut hints = Vec::new();
        loop {
            let action = if self.at_keyword("USE") {
                IndexHintAction::Use
            } else if self.at_keyword("FORCE") {
                IndexHintAction::Force
            } else if self.at_keyword("IGNORE") {
                IndexHintAction::Ignore
            } else {
                return Ok(hints);
            };
            self.advance();
            let key = if self.eat_keyword("INDEX").is_some() {
                false
            } else {
                self.expect_keyword("KEY")?;
                true
            };
            let scope = if self.eat_keyword("FOR").is_some() {
                if self.eat_keyword("JOIN").is_some() {
                    Some(IndexHintScope::Join)
                } else if self.eat_keyword("ORDER").is_some() {
                    self.expect_keyword("BY")?;
                    Some(IndexHintScope::OrderBy)
                } else {
                    self.expect_keyword("GROUP")?;
                    self.expect_keyword("BY")?;
                    Some(IndexHintScope::GroupBy)
                }
            } else {
                None
            };
            self.expect_punct(Punct::LeftParen)?;
            let mut indexes = Vec::new();
            if action != IndexHintAction::Use || !self.at_punct(Punct::RightParen) {
                // `PRIMARY` is a reserved word, and the name of an index.
                indexes.push(self.any_name().ok_or_else(|| self.expected("an index"))?);
                while self.eat_punct(Punct::Comma).is_some() {
                    indexes.push(self.any_name().ok_or_else(|| self.expected("an index"))?);
                }
            }
            self.expect_punct(Punct::RightParen)?;
            hints.push(IndexHint {
                action,
                key,
                scope,
                indexes,
            });
        }
    }
}

/// Join of `left` with the leftmost source of `right`, the joins of `right`
/// then applied to the result: `a JOIN (b JOIN c ON x)` without condition
/// becomes `(a JOIN b) JOIN c ON x`.
fn rotate(left: TableRef, operator: JoinOperator, right: TableRef) -> TableRef {
    match right {
        TableRef::Join(join) => {
            let Join {
                left: inner_left,
                right: inner_right,
                kind,
                form,
                natural,
                constraint,
                span,
            } = *join;
            let rotated = rotate(left, operator, inner_left);
            let span = Span::new(rotated.span().start, span.end);
            TableRef::Join(Box::new(Join {
                left: rotated,
                right: inner_right,
                kind,
                form,
                natural,
                constraint,
                span,
            }))
        }
        leaf => {
            let span = Span::new(left.span().start, leaf.span().end);
            TableRef::Join(Box::new(Join {
                left,
                right: leaf,
                kind: operator.kind,
                form: operator.form,
                natural: false,
                constraint: None,
                span,
            }))
        }
    }
}
