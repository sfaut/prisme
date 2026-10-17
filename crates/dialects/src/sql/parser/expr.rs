//! Expressions of the MySQL family, with the precedence of its operators,
//! from the loosest to the tightest:
//!
//! 1. `OR`, `||` (without `PIPES_AS_CONCAT`);
//! 2. `XOR`;
//! 3. `AND`, `&&`;
//! 4. `NOT` (with `HIGH_NOT_PRECEDENCE`, as tight as `!`);
//! 5. comparisons, `IS`, `LIKE`, `IN`, `BETWEEN`;
//! 6. `|`; 7. `&`; 8. `<<`, `>>`; 9. `+`, `-`; 10. `*`, `/`, `DIV`, `%`,
//!    `MOD`; 11. `^`;
//! 12. prefix `-`, `+`, `~`, `!`.
//!
//! Binary operators associate to the left (`a - b - c` is `(a - b) - c`,
//! `a = b = c` is `(a = b) = c`).

use prisme_plan::Span;

use super::{ParseError, Parser};
use crate::lexer::{Punct, TokenKind};
use crate::sql::ast::{BinaryOp, Expr, ExprKind, Literal, ObjectName, Query, UnaryOp};

impl Parser<'_> {
    /// Expression.
    pub(super) fn expr(&mut self) -> Result<Expr, ParseError> {
        self.nested(Self::or_expr)
    }

    /// `OR`, `||`.
    fn or_expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.xor_expr()?;
        loop {
            if self.at_punct(Punct::DoublePipe) && self.options.mode.pipes_as_concat {
                return Err(self.unsupported("`||` as concatenation", self.here()));
            }
            if self.eat_keyword("OR").is_none() && self.eat_punct(Punct::DoublePipe).is_none() {
                return Ok(left);
            }
            let right = self.xor_expr()?;
            left = binary(BinaryOp::Or, left, right);
        }
    }

    /// `XOR`.
    fn xor_expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.and_expr()?;
        while self.eat_keyword("XOR").is_some() {
            let right = self.and_expr()?;
            left = binary(BinaryOp::Xor, left, right);
        }
        Ok(left)
    }

    /// `AND`, `&&`.
    fn and_expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.not_expr()?;
        while self.eat_keyword("AND").is_some() || self.eat_punct(Punct::DoubleAmpersand).is_some()
        {
            let right = self.not_expr()?;
            left = binary(BinaryOp::And, left, right);
        }
        Ok(left)
    }

    /// `NOT`, loose unless `HIGH_NOT_PRECEDENCE`.
    fn not_expr(&mut self) -> Result<Expr, ParseError> {
        if !self.options.mode.high_not_precedence
            && let Some(span) = self.eat_keyword("NOT")
        {
            let operand = self.nested(Self::not_expr)?;
            return Ok(unary(UnaryOp::Not, span.start, operand));
        }
        self.predicate()
    }

    /// Comparisons, `IS`, `BETWEEN`, `IN`, `LIKE`.
    fn predicate(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.bit_or()?;
        loop {
            if let Some(op) = self.comparison() {
                self.advance();
                left = if self.at_quantifier(op) {
                    self.quantified(left, op)?
                } else {
                    let right = self.bit_or()?;
                    binary(op, left, right)
                };
                continue;
            }
            if self.eat_keyword("IS").is_some() {
                let negated = self.eat_keyword("NOT").is_some();
                if self.eat_keyword("NULL").is_none() {
                    if self.at_any_keyword(&["TRUE", "FALSE", "UNKNOWN"]) {
                        return Err(self.unsupported("`IS TRUE`, `IS FALSE`", self.here()));
                    }
                    return Err(self.expected("`NULL`"));
                }
                let span = self.span_from(left.span.start);
                left = Expr {
                    kind: ExprKind::IsNull {
                        operand: Box::new(left),
                        negated,
                    },
                    span,
                };
                continue;
            }
            let negated = self.at_keyword("NOT")
                && self.is_any_word(1, &["BETWEEN", "IN", "LIKE", "REGEXP", "RLIKE"]);
            if negated {
                self.advance();
            }
            if self.eat_keyword("BETWEEN").is_some() {
                let low = self.bit_or()?;
                self.expect_keyword("AND")?;
                let high = self.bit_or()?;
                let span = self.span_from(left.span.start);
                left = Expr {
                    kind: ExprKind::Between {
                        operand: Box::new(left),
                        low: Box::new(low),
                        high: Box::new(high),
                        negated,
                    },
                    span,
                };
                continue;
            }
            if self.eat_keyword("IN").is_some() {
                left = self.in_list(left, negated)?;
                continue;
            }
            if self.eat_keyword("LIKE").is_some() {
                let pattern = self.bit_or()?;
                let escape = if self.eat_keyword("ESCAPE").is_some() {
                    Some(Box::new(self.bit_or()?))
                } else {
                    None
                };
                let span = self.span_from(left.span.start);
                left = Expr {
                    kind: ExprKind::Like {
                        operand: Box::new(left),
                        pattern: Box::new(pattern),
                        escape,
                        negated,
                    },
                    span,
                };
                continue;
            }
            if self.at_any_keyword(&["REGEXP", "RLIKE", "SOUNDS", "MEMBER"]) {
                return Err(self.unsupported("this operator", self.here()));
            }
            return Ok(left);
        }
    }

    /// Tells whether the token `offset` positions after the next one is one
    /// of the words `keywords`.
    fn is_any_word(&self, offset: usize, keywords: &[&str]) -> bool {
        keywords
            .iter()
            .any(|keyword| self.is_word(self.peek_at(offset), keyword))
    }

    /// Comparison operator that comes next.
    fn comparison(&self) -> Option<BinaryOp> {
        let TokenKind::Punct(punct) = self.peek()?.kind else {
            return None;
        };
        Some(match punct {
            Punct::Eq => BinaryOp::Eq,
            Punct::LtGt | Punct::BangEq => BinaryOp::NotEq,
            Punct::Lt => BinaryOp::Lt,
            Punct::LtEq => BinaryOp::LtEq,
            Punct::Gt => BinaryOp::Gt,
            Punct::GtEq => BinaryOp::GtEq,
            Punct::Spaceship => BinaryOp::NullSafeEq,
            _ => return None,
        })
    }

    /// Tells whether `ANY`, `SOME` or `ALL` and a subquery follow the
    /// comparison `op`; `<=>` has none in the MySQL family.
    fn at_quantifier(&self, op: BinaryOp) -> bool {
        op != BinaryOp::NullSafeEq
            && self.at_any_keyword(&["ANY", "SOME", "ALL"])
            && self
                .peek_at(1)
                .is_some_and(|token| token.kind == TokenKind::Punct(Punct::LeftParen))
    }

    /// `operand op ANY (subquery)`, from `ANY`, `SOME` or `ALL`.
    fn quantified(&mut self, operand: Expr, op: BinaryOp) -> Result<Expr, ParseError> {
        let all = self.at_keyword("ALL");
        self.advance();
        let query = self.subquery()?;
        let span = self.span_from(operand.span.start);
        Ok(Expr {
            kind: ExprKind::Quantified {
                op,
                operand: Box::new(operand),
                all,
                query: Box::new(query),
            },
            span,
        })
    }

    /// `(query)`: a subquery between parentheses.
    fn subquery(&mut self) -> Result<Query, ParseError> {
        self.expect_punct(Punct::LeftParen)?;
        let query = self.query()?;
        self.expect_punct(Punct::RightParen)?;
        Ok(query)
    }

    /// `IN (values)` or `IN (subquery)`, after `IN`.
    fn in_list(&mut self, operand: Expr, negated: bool) -> Result<Expr, ParseError> {
        if self.query_in_parentheses() {
            let query = self.subquery()?;
            let span = self.span_from(operand.span.start);
            return Ok(Expr {
                kind: ExprKind::InSubquery {
                    operand: Box::new(operand),
                    query: Box::new(query),
                    negated,
                },
                span,
            });
        }
        self.expect_punct(Punct::LeftParen)?;
        let mut list = vec![self.expr()?];
        while self.eat_punct(Punct::Comma).is_some() {
            list.push(self.expr()?);
        }
        self.expect_punct(Punct::RightParen)?;
        let span = self.span_from(operand.span.start);
        Ok(Expr {
            kind: ExprKind::InList {
                operand: Box::new(operand),
                list,
                negated,
            },
            span,
        })
    }

    /// `|`.
    fn bit_or(&mut self) -> Result<Expr, ParseError> {
        self.binary_level(Self::bit_and, &[(Punct::Pipe, BinaryOp::BitOr)], &[])
    }

    /// `&`.
    fn bit_and(&mut self) -> Result<Expr, ParseError> {
        self.binary_level(Self::shift, &[(Punct::Ampersand, BinaryOp::BitAnd)], &[])
    }

    /// `<<`, `>>`.
    fn shift(&mut self) -> Result<Expr, ParseError> {
        self.binary_level(
            Self::additive,
            &[
                (Punct::ShiftLeft, BinaryOp::ShiftLeft),
                (Punct::ShiftRight, BinaryOp::ShiftRight),
            ],
            &[],
        )
    }

    /// `+`, `-`.
    fn additive(&mut self) -> Result<Expr, ParseError> {
        self.binary_level(
            Self::multiplicative,
            &[
                (Punct::Plus, BinaryOp::Add),
                (Punct::Minus, BinaryOp::Subtract),
            ],
            &[],
        )
    }

    /// `*`, `/`, `DIV`, `%`, `MOD`.
    fn multiplicative(&mut self) -> Result<Expr, ParseError> {
        self.binary_level(
            Self::bit_xor,
            &[
                (Punct::Star, BinaryOp::Multiply),
                (Punct::Slash, BinaryOp::Divide),
                (Punct::Percent, BinaryOp::Modulo),
            ],
            &[("DIV", BinaryOp::IntegerDivide), ("MOD", BinaryOp::Modulo)],
        )
    }

    /// `^`.
    fn bit_xor(&mut self) -> Result<Expr, ParseError> {
        self.binary_level(Self::unary, &[(Punct::Caret, BinaryOp::BitXor)], &[])
    }

    /// Level of left-associative binary operators, written as a symbol or a
    /// word, whose operands are read by `operand`.
    fn binary_level(
        &mut self,
        operand: fn(&mut Self) -> Result<Expr, ParseError>,
        symbols: &[(Punct, BinaryOp)],
        words: &[(&str, BinaryOp)],
    ) -> Result<Expr, ParseError> {
        let mut left = operand(self)?;
        loop {
            let symbol = symbols
                .iter()
                .find(|(punct, _)| self.at_punct(*punct))
                .map(|&(_, op)| op);
            let op = symbol.or_else(|| {
                words
                    .iter()
                    .find(|(word, _)| self.at_keyword(word))
                    .map(|&(_, op)| op)
            });
            let Some(op) = op else {
                return Ok(left);
            };
            self.advance();
            let right = operand(self)?;
            left = binary(op, left, right);
        }
    }

    /// Prefix operators: `-`, `+`, `~`, `!` (and `NOT` under
    /// `HIGH_NOT_PRECEDENCE`).
    fn unary(&mut self) -> Result<Expr, ParseError> {
        let op = match self.peek().map(|token| &token.kind) {
            Some(TokenKind::Punct(Punct::Minus)) => Some(UnaryOp::Minus),
            Some(TokenKind::Punct(Punct::Plus)) => Some(UnaryOp::Plus),
            Some(TokenKind::Punct(Punct::Tilde)) => Some(UnaryOp::BitNot),
            Some(TokenKind::Punct(Punct::Bang)) => Some(UnaryOp::Not),
            Some(TokenKind::Word)
                if self.options.mode.high_not_precedence && self.at_keyword("NOT") =>
            {
                Some(UnaryOp::Not)
            }
            _ => None,
        };
        let Some(op) = op else {
            return self.expr_primary();
        };
        let start = self.here().start;
        self.advance();
        let operand = self.nested(Self::unary)?;
        Ok(unary(op, start, operand))
    }

    /// Literal, column, or expression between parentheses.
    pub(super) fn expr_primary(&mut self) -> Result<Expr, ParseError> {
        let Some(token) = self.peek().cloned() else {
            return Err(self.expected("an expression"));
        };
        let literal = match &token.kind {
            TokenKind::Integer => Some(Literal::Integer(token.text(self.text).to_string())),
            TokenKind::Decimal => Some(Literal::Decimal(token.text(self.text).to_string())),
            TokenKind::Float => Some(Literal::Float(token.text(self.text).to_string())),
            _ => None,
        };
        if let Some(literal) = literal {
            self.advance();
            return Ok(leaf(ExprKind::Literal(literal), token.span));
        }
        match &token.kind {
            TokenKind::String { .. } => Ok(self.string()),
            TokenKind::NationalString(_)
            | TokenKind::HexString(_)
            | TokenKind::BitString(_)
            | TokenKind::HexNumber(_)
            | TokenKind::BitNumber(_) => {
                Err(self.unsupported("national, hexadecimal and bit literals", token.span))
            }
            TokenKind::UserVariable { .. } | TokenKind::SystemVariable => {
                Err(self.unsupported("variables", token.span))
            }
            TokenKind::Placeholder => {
                Err(self.unsupported("parameters of prepared statements", token.span))
            }
            TokenKind::Punct(Punct::LeftParen) => self.parenthesized(),
            TokenKind::Word | TokenKind::QuotedIdent(_) => self.word_expr(),
            _ => Err(self.expected("an expression")),
        }
    }

    /// Strings in a row, joined into one (`'a' 'b'` is `'ab'`).
    fn string(&mut self) -> Expr {
        let start = self.here().start;
        let mut value = String::new();
        while let Some(TokenKind::String { value: part, .. }) = self.peek().map(|token| &token.kind)
        {
            value.push_str(part);
            self.pos += 1;
        }
        leaf(
            ExprKind::Literal(Literal::String(value)),
            self.span_from(start),
        )
    }

    /// Expression, subquery or row between parentheses.
    fn parenthesized(&mut self) -> Result<Expr, ParseError> {
        let start = self.here().start;
        if self.query_in_parentheses() {
            return self.scalar_subquery(start);
        }
        self.expect_punct(Punct::LeftParen)?;
        let inner = self.expr()?;
        if self.at_punct(Punct::Comma) {
            return self.row(start, inner);
        }
        self.expect_punct(Punct::RightParen)?;
        Ok(leaf(
            ExprKind::Nested(Box::new(inner)),
            self.span_from(start),
        ))
    }

    /// Subquery between parentheses, from `start`.
    fn scalar_subquery(&mut self, start: usize) -> Result<Expr, ParseError> {
        let query = self.subquery()?;
        Ok(leaf(
            ExprKind::Subquery(Box::new(query)),
            self.span_from(start),
        ))
    }

    /// Row from `start`, after its first value: `, b, c)`.
    fn row(&mut self, start: usize, first: Expr) -> Result<Expr, ParseError> {
        let mut values = vec![first];
        while self.eat_punct(Punct::Comma).is_some() {
            values.push(self.expr()?);
        }
        self.expect_punct(Punct::RightParen)?;
        Ok(leaf(ExprKind::Row(values), self.span_from(start)))
    }

    /// `EXISTS (subquery)` or `ROW (values)`, from the keyword; `None` for
    /// another word.
    fn keyword_construct(&mut self) -> Result<Option<Expr>, ParseError> {
        let start = self.here().start;
        if self.at_keyword("EXISTS") {
            self.advance();
            let query = self.subquery()?;
            return Ok(Some(leaf(
                ExprKind::Exists(Box::new(query)),
                self.span_from(start),
            )));
        }
        if self.at_keyword("ROW") {
            self.advance();
            self.expect_punct(Punct::LeftParen)?;
            // `ROW` holds two values at least.
            let mut values = vec![self.expr()?];
            self.expect_punct(Punct::Comma)?;
            values.push(self.expr()?);
            while self.eat_punct(Punct::Comma).is_some() {
                values.push(self.expr()?);
            }
            self.expect_punct(Punct::RightParen)?;
            return Ok(Some(leaf(ExprKind::Row(values), self.span_from(start))));
        }
        Ok(None)
    }

    /// Expression that starts with a word: a keyword literal, a column, or a
    /// construct not read yet.
    fn word_expr(&mut self) -> Result<Expr, ParseError> {
        let token = self.peek().cloned().expect("a word comes next");
        let keyword_literal = [
            ("NULL", Literal::Null),
            ("TRUE", Literal::True),
            ("FALSE", Literal::False),
        ]
        .into_iter()
        .find(|(keyword, _)| self.is_word(Some(&token), keyword));
        if let Some((_, literal)) = keyword_literal {
            self.advance();
            return Ok(leaf(ExprKind::Literal(literal), token.span));
        }
        let next = self.peek_at(1).map(|next| &next.kind);
        if token.kind == TokenKind::Word {
            if matches!(next, Some(TokenKind::Punct(Punct::LeftParen)))
                && let Some(construct) = self.keyword_construct()?
            {
                return Ok(construct);
            }
            let next = self.peek_at(1).map(|next| &next.kind);
            if matches!(next, Some(TokenKind::Punct(Punct::LeftParen))) {
                return Err(self.unsupported("function calls", token.span));
            }
            if matches!(next, Some(TokenKind::String { .. })) {
                return Err(self.unsupported("typed literals and charset introducers", token.span));
            }
            let constructs = [
                "CASE",
                "CAST",
                "CONVERT",
                "EXISTS",
                "INTERVAL",
                "BINARY",
                "DATE",
                "TIME",
                "TIMESTAMP",
                "DEFAULT",
                "ROW",
                "MATCH",
            ];
            if constructs
                .iter()
                .any(|keyword| self.is_word(Some(&token), keyword))
                && !self.is_name(Some(&token))
            {
                return Err(self.unsupported(
                    &format!("`{}`", token.text(self.text).to_ascii_uppercase()),
                    token.span,
                ));
            }
        }
        let name: ObjectName = self.object_name(3)?;
        let span = name.span;
        Ok(leaf(ExprKind::Column(name), span))
    }
}

/// Expression without children.
fn leaf(kind: ExprKind, span: Span) -> Expr {
    Expr { kind, span }
}

/// Binary expression, from the start of `left` to the end of `right`.
fn binary(op: BinaryOp, left: Expr, right: Expr) -> Expr {
    let span = Span::new(left.span.start, right.span.end);
    Expr {
        kind: ExprKind::Binary {
            op,
            left: Box::new(left),
            right: Box::new(right),
        },
        span,
    }
}

/// Prefix expression from `start` to the end of its operand.
fn unary(op: UnaryOp, start: usize, operand: Expr) -> Expr {
    let span = Span::new(start, operand.span.end);
    Expr {
        kind: ExprKind::Unary {
            op,
            operand: Box::new(operand),
        },
        span,
    }
}
