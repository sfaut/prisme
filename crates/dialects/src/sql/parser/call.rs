//! Function calls of the MySQL family: scalar functions, aggregate functions
//! (`COUNT(*)`, `DISTINCT`, `GROUP_CONCAT(... ORDER BY ... SEPARATOR ...)`),
//! window functions with their window (`OVER`), and the `WINDOW` clause.
//!
//! A call is to a built-in function of the dialect in its version, or to a
//! function stored in the database (reference file of the dialect,
//! `builtins` module): the names of some built-in functions call a stored
//! function when a space separates them from `(` (`SUM (a)`, so `COUNT (*)`
//! is a syntax error) or when written between backticks (`` `LEFT`(a) ``),
//! as on the servers. `CAST` and `CONVERT` have a module of their own
//! (`cast`); the other functions whose arguments have a syntax of their own
//! (`TRIM(LEADING ...)`, `DATE_ADD(d, INTERVAL ...)`...) are read in a later
//! increment of step 5.

use prisme_plan::{FrameUnits, Span};

use super::{ParseError, Parser};
use crate::builtins;
use crate::lexer::{Punct, TokenKind};
use crate::sql::ast::{
    AGGREGATES, Expr, ExprKind, Frame, FrameBound, FunctionCall, NamedWindow, ObjectName,
    WindowSpec,
};
use crate::versions::Construct;

/// Functions whose arguments have a syntax of their own, not read yet.
const SPECIAL_SYNTAX: [&str; 14] = [
    "ADDDATE",
    "CHAR",
    "DATE_ADD",
    "DATE_SUB",
    "EXTRACT",
    "GET_FORMAT",
    "JSON_TABLE",
    "JSON_VALUE",
    "POSITION",
    "SUBDATE",
    "TIMESTAMPADD",
    "TIMESTAMPDIFF",
    "TRIM",
    "WEIGHT_STRING",
];

impl Parser<'_> {
    /// Tells whether a function call comes next: a name followed by `(`, or
    /// a database, a dot, a name and `(`. `ANY`, `SOME` and `ALL` before `(`
    /// quantify a subquery.
    pub(super) fn at_call(&self) -> bool {
        if self.at_any_keyword(&["ANY", "SOME", "ALL"]) {
            return false;
        }
        let is_paren = |offset| {
            self.peek_at(offset)
                .is_some_and(|token| token.kind == TokenKind::Punct(Punct::LeftParen))
        };
        let is_dot = self
            .peek_at(1)
            .is_some_and(|token| token.kind == TokenKind::Punct(Punct::Dot));
        let named = matches!(
            self.peek().map(|token| &token.kind),
            Some(TokenKind::Word | TokenKind::QuotedIdent { .. })
        );
        named && (is_paren(1) || (is_dot && is_paren(3)))
    }

    /// Call to a function, from its name.
    pub(super) fn call(&mut self) -> Result<Expr, ParseError> {
        let start = self.here().start;
        let name = if self
            .peek_at(1)
            .is_some_and(|token| token.kind == TokenKind::Punct(Punct::Dot))
        {
            self.object_name(2)?
        } else {
            // A reserved word names a function before `(`: `LEFT(a, 1)`.
            let ident = self.any_name().ok_or_else(|| self.expected("a function"))?;
            ObjectName {
                span: Span::new(start, self.span_from(start).end),
                parts: vec![ident],
            }
        };
        let upper = name.parts[name.parts.len() - 1].value.to_ascii_uppercase();
        let stored = self.stored(&name, &upper);
        match upper.as_str() {
            "CAST" if !stored => return self.cast(start),
            "CONVERT" if !stored => return self.convert(start),
            _ => {}
        }
        if !stored && SPECIAL_SYNTAX.contains(&upper.as_str()) {
            return Err(self.unsupported(&format!("`{upper}`"), name.span));
        }
        self.expect_punct(Punct::LeftParen)?;
        let aggregate = !stored && AGGREGATES.contains(&upper.as_str());
        let mut call = FunctionCall {
            name,
            args: Vec::new(),
            star: false,
            distinct: false,
            order_by: Vec::new(),
            separator: None,
            stored,
            over: None,
        };
        if aggregate {
            self.aggregate_arguments(&mut call, &upper)?;
        } else if !self.at_punct(Punct::RightParen) {
            call.args.push(self.expr()?);
            while self.eat_punct(Punct::Comma).is_some() {
                call.args.push(self.expr()?);
            }
        }
        self.expect_punct(Punct::RightParen)?;
        if self.at_any_keyword(&["RESPECT", "IGNORE"])
            || (self.at_keyword("FROM") && self.at_any_word_at(1, &["FIRST", "LAST"]))
        {
            return Err(self.unsupported("`RESPECT NULLS` and `FROM FIRST`", self.here()));
        }
        if self.at_keyword("WITHIN") && self.at_any_word_at(1, &["GROUP"]) {
            return Err(self.unsupported("`WITHIN GROUP`", self.here()));
        }
        if self.at_keyword("OVER") && self.at_window_after_over() {
            if call.stored {
                return Err(self.expected("the end of the call of a stored function"));
            }
            let over = self.expect_keyword("OVER")?;
            self.check(Construct::WindowFunctions, over)?;
            call.over = Some(self.window()?);
        }
        Ok(Expr {
            kind: ExprKind::Function(Box::new(call)),
            span: self.span_from(start),
        })
    }

    /// Tells whether the call of `name`, `upper` in capitals, before `(`,
    /// calls a function stored in the database: a qualified name, a name the
    /// dialect does not know in its version, the name of a function of the
    /// grammar between backticks (`` `LEFT`(a) ``), or followed by a space
    /// (`NOW ()`) without the SQL mode `IGNORE_SPACE`.
    fn stored(&self, name: &ObjectName, upper: &str) -> bool {
        let [ident] = name.parts.as_slice() else {
            return true;
        };
        let options = self.options;
        let Some(builtin) = builtins::builtin(options.dialect, options.version, upper) else {
            return true;
        };
        if ident.quoted {
            return builtin.function.grammar;
        }
        let spaced = name.span.end != self.here().start;
        spaced && builtin.function.touching && !options.mode.ignore_space
    }

    /// Tells whether a window follows `OVER`: `(` or a name. `OVER` is a
    /// word that MariaDB does not reserve.
    fn at_window_after_over(&self) -> bool {
        let next = self.peek_at(1);
        next.is_some_and(|token| token.kind == TokenKind::Punct(Punct::LeftParen))
            || self.is_name(next)
    }

    /// Tells whether the word `offset` positions ahead is one of `keywords`.
    fn at_any_word_at(&self, offset: usize, keywords: &[&str]) -> bool {
        keywords
            .iter()
            .any(|keyword| self.is_word(self.peek_at(offset), keyword))
    }

    /// Arguments of an aggregate function, after `(`: `*`, `DISTINCT`, and
    /// the `ORDER BY` and `SEPARATOR` of `GROUP_CONCAT`.
    fn aggregate_arguments(
        &mut self,
        call: &mut FunctionCall,
        name: &str,
    ) -> Result<(), ParseError> {
        if name == "COUNT" && self.eat_punct(Punct::Star).is_some() {
            call.star = true;
            return Ok(());
        }
        if self.eat_keyword("DISTINCT").is_some() {
            call.distinct = true;
        } else {
            self.eat_keyword("ALL");
        }
        call.args.push(self.expr()?);
        while self.eat_punct(Punct::Comma).is_some() {
            call.args.push(self.expr()?);
        }
        if name != "GROUP_CONCAT" {
            return Ok(());
        }
        if self.eat_keyword("ORDER").is_some() {
            self.expect_keyword("BY")?;
            call.order_by = self.order_items()?;
        }
        if let Some(limit) = self.eat_keyword("LIMIT") {
            return Err(self.unsupported("`LIMIT` in `GROUP_CONCAT`", limit));
        }
        if self.eat_keyword("SEPARATOR").is_some() {
            let token = self.peek().cloned();
            match token.map(|token| token.kind) {
                Some(TokenKind::String { value, .. }) => {
                    self.advance();
                    call.separator = Some(value);
                }
                _ => return Err(self.expected("a string")),
            }
        }
        Ok(())
    }

    /// Window after `OVER`: a name, or a specification between parentheses.
    fn window(&mut self) -> Result<WindowSpec, ParseError> {
        if !self.at_punct(Punct::LeftParen) {
            let start = self.here().start;
            let name = self.name()?;
            return Ok(WindowSpec {
                base: Some(name),
                partition_by: Vec::new(),
                order_by: Vec::new(),
                frame: None,
                span: self.span_from(start),
            });
        }
        self.window_spec()
    }

    /// `([base] [PARTITION BY ...] [ORDER BY ...] [frame])`.
    fn window_spec(&mut self) -> Result<WindowSpec, ParseError> {
        let start = self.expect_punct(Punct::LeftParen)?.start;
        let clause = self.at_any_keyword(&["PARTITION", "ORDER", "ROWS", "RANGE"]);
        let base = if !clause && self.is_name(self.peek()) {
            Some(self.name()?)
        } else {
            None
        };
        let mut partition_by = Vec::new();
        if self.eat_keyword("PARTITION").is_some() {
            self.expect_keyword("BY")?;
            partition_by.push(self.expr()?);
            while self.eat_punct(Punct::Comma).is_some() {
                partition_by.push(self.expr()?);
            }
        }
        let order_by = if self.eat_keyword("ORDER").is_some() {
            self.expect_keyword("BY")?;
            self.order_items()?
        } else {
            Vec::new()
        };
        let frame = self.frame()?;
        self.expect_punct(Punct::RightParen)?;
        Ok(WindowSpec {
            base,
            partition_by,
            order_by,
            frame,
            span: self.span_from(start),
        })
    }

    /// `ROWS` or `RANGE` frame, when written.
    fn frame(&mut self) -> Result<Option<Frame>, ParseError> {
        let units = if self.eat_keyword("ROWS").is_some() {
            FrameUnits::Rows
        } else if self.eat_keyword("RANGE").is_some() {
            FrameUnits::Range
        } else {
            return Ok(None);
        };
        if self.eat_keyword("BETWEEN").is_none() {
            let start = self.frame_bound()?;
            return Ok(Some(Frame {
                units,
                start,
                end: None,
            }));
        }
        let start = self.frame_bound()?;
        self.expect_keyword("AND")?;
        let end = self.frame_bound()?;
        Ok(Some(Frame {
            units,
            start,
            end: Some(end),
        }))
    }

    /// Bound of a frame.
    fn frame_bound(&mut self) -> Result<FrameBound, ParseError> {
        if self.eat_keyword("UNBOUNDED").is_some() {
            if self.eat_keyword("PRECEDING").is_some() {
                return Ok(FrameBound::UnboundedPreceding);
            }
            self.expect_keyword("FOLLOWING")?;
            return Ok(FrameBound::UnboundedFollowing);
        }
        if self.eat_keyword("CURRENT").is_some() {
            self.expect_keyword("ROW")?;
            return Ok(FrameBound::CurrentRow);
        }
        // The bound is read below the comparisons: `1 PRECEDING`.
        let value = Box::new(self.bit_or()?);
        if self.eat_keyword("PRECEDING").is_some() {
            return Ok(FrameBound::Preceding(value));
        }
        self.expect_keyword("FOLLOWING")?;
        Ok(FrameBound::Following(value))
    }

    /// `WINDOW w AS (...), ...`, when written.
    pub(super) fn window_clause(&mut self) -> Result<Vec<NamedWindow>, ParseError> {
        let Some(keyword) = self.eat_keyword("WINDOW") else {
            return Ok(Vec::new());
        };
        self.check(Construct::WindowFunctions, keyword)?;
        let mut windows = Vec::new();
        loop {
            let name = self.name()?;
            self.expect_keyword("AS")?;
            let spec = self.window_spec()?;
            windows.push(NamedWindow { name, spec });
            if self.eat_punct(Punct::Comma).is_none() {
                return Ok(windows);
            }
        }
    }
}
