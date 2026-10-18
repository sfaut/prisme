//! Expressions of the MySQL family, with the precedence of its operators,
//! from the loosest to the tightest:
//!
//! 1. `OR`, `||` (without `PIPES_AS_CONCAT`);
//! 2. `XOR`;
//! 3. `AND`, `&&`;
//! 4. `NOT` (with `HIGH_NOT_PRECEDENCE`, as tight as `!`);
//! 5. comparisons, `IS NULL`, `IS TRUE`, `IS FALSE`, `IS UNKNOWN`;
//! 6. the tests `LIKE`, `IN`, `BETWEEN`, `REGEXP`, `SOUNDS LIKE`, `MEMBER OF`;
//! 7. `|`; 8. `&`; 9. `<<`, `>>`; 10. `+`, `-`; 11. `*`, `/`, `DIV`, `%`,
//!    `MOD`; 12. `^`;
//! 13. prefix `-`, `+`, `~`, `!`.
//!
//! Binary operators associate to the left (`a - b - c` is `(a - b) - c`,
//! `a = b = c` is `(a = b) = c`). A comparison compares two tests
//! (`a = b BETWEEN 1 AND 2` compares `a` with the `BETWEEN`), and the high
//! bound of `BETWEEN` is a test; MySQL reads one test after a value, MariaDB
//! chains them (`crates/probe/cases/predicate-grammar.txt`, `test`).

use prisme_plan::{CallForm, Ident, Span, Truth};

use super::{ParseError, Parser};
use crate::lexer::{Punct, TokenKind};
use crate::sql::ast::{
    BinaryOp, Expr, ExprKind, FunctionCall, Literal, ObjectName, Query, Strings, TemporalKind,
    UnaryOp,
};
use crate::versions::Construct;
use crate::{Dialect, keywords};

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
    pub(super) fn not_expr(&mut self) -> Result<Expr, ParseError> {
        if !self.options.mode.high_not_precedence
            && let Some(span) = self.eat_keyword("NOT")
        {
            let operand = self.nested(Self::not_expr)?;
            return Ok(unary(UnaryOp::Not, span.start, operand));
        }
        self.predicate()
    }

    /// Comparisons and `IS`, left to right, each comparing two predicates
    /// (`a = b BETWEEN 1 AND 2` compares `a` with the `BETWEEN`). `IS TRUE`,
    /// `IS FALSE` and `IS UNKNOWN` end the expression in MySQL (`a IS TRUE
    /// = 1` is a syntax error); MariaDB goes on.
    fn predicate(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.test()?;
        loop {
            if let Some(op) = self.comparison() {
                self.advance();
                left = if self.at_quantifier(op) {
                    self.quantified(left, op)?
                } else {
                    let right = self.test()?;
                    binary(op, left, right)
                };
                continue;
            }
            if self.eat_keyword("IS").is_some() {
                let negated = self.eat_keyword("NOT").is_some();
                let truth = [
                    ("TRUE", Truth::True),
                    ("FALSE", Truth::False),
                    ("UNKNOWN", Truth::Unknown),
                ]
                .into_iter()
                .find_map(|(keyword, truth)| self.eat_keyword(keyword).map(|_| truth));
                if let Some(truth) = truth {
                    let span = self.span_from(left.span.start);
                    left = Expr {
                        kind: ExprKind::IsTruth {
                            operand: Box::new(left),
                            truth,
                            negated,
                        },
                        span,
                    };
                    // MySQL closes the expression there: a comparison or
                    // `IS` after it is a syntax error.
                    if self.options.dialect == Dialect::MySql {
                        return Ok(left);
                    }
                    continue;
                }
                if self.eat_keyword("NULL").is_none() {
                    return Err(self.expected("`NULL`, `TRUE`, `FALSE` or `UNKNOWN`"));
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
            return Ok(left);
        }
    }

    /// `BETWEEN`, `IN`, `LIKE` after a value, its high bound a test
    /// (`a BETWEEN 0 AND b IN (1)` bounds `a` by the `IN`), and the tests of
    /// the `predicate` module (`REGEXP`, `SOUNDS LIKE`, `MEMBER OF`). MySQL
    /// reads one, the pattern and the escape character of `LIKE` simple
    /// operands (`'2' LIKE 1 + 1` is a syntax error); MariaDB chains them
    /// from left to right (`a IN (1) IN (1)`), the pattern and the escape
    /// character of `LIKE` at the level of `|`, the bounds of `BETWEEN`
    /// tests.
    fn test(&mut self) -> Result<Expr, ParseError> {
        let mariadb = self.options.dialect == Dialect::MariaDb;
        let mut left = self.bit_or()?;
        loop {
            let negated = self.at_keyword("NOT")
                && self.is_any_word(1, &["BETWEEN", "IN", "LIKE", "REGEXP", "RLIKE"]);
            if negated {
                self.advance();
            }
            if self.eat_keyword("BETWEEN").is_some() {
                let low = if mariadb {
                    self.nested(Self::test)?
                } else {
                    self.bit_or()?
                };
                self.expect_keyword("AND")?;
                let high = self.nested(Self::test)?;
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
            } else if self.eat_keyword("IN").is_some() {
                left = self.in_list(left, negated)?;
            } else if self.eat_keyword("LIKE").is_some() {
                let pattern = self.like_operand()?;
                let escape = if self.eat_keyword("ESCAPE").is_some() {
                    Some(Box::new(self.like_operand()?))
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
            } else if self.eat_keyword("REGEXP").is_some() || self.eat_keyword("RLIKE").is_some() {
                left = self.regexp(left, negated)?;
            } else if self.at_keyword("SOUNDS") {
                left = self.sounds_like(left)?;
            } else if self.at_member_of() {
                left = self.member_of(left)?;
            } else {
                return Ok(left);
            }
            if !mariadb {
                return Ok(left);
            }
        }
    }

    /// Pattern or escape character of `LIKE`: a simple operand in MySQL, an
    /// expression at the level of `|` in MariaDB.
    fn like_operand(&mut self) -> Result<Expr, ParseError> {
        match self.options.dialect {
            Dialect::MySql => self.unary(),
            Dialect::MariaDb => self.bit_or(),
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
    pub(super) fn bit_or(&mut self) -> Result<Expr, ParseError> {
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

    /// `+`, `-`, and an interval added or subtracted (`d + INTERVAL 1 DAY`),
    /// whose sum closes at its unit, then takes the operators of a higher
    /// level: `d + INTERVAL 1 DAY * 2` multiplies the sum.
    fn additive(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.multiplicative()?;
        loop {
            let subtract = if self.at_punct(Punct::Minus) {
                true
            } else if self.at_punct(Punct::Plus) {
                false
            } else {
                return Ok(left);
            };
            self.advance();
            if self.at_interval() {
                let sum = self.interval_operand(left, subtract)?;
                let sum = self.binary_level_from(sum, Self::unary, &BIT_XOR, &[])?;
                left = self.binary_level_from(
                    sum,
                    Self::bit_xor,
                    &MULTIPLICATIVE,
                    &MULTIPLICATIVE_WORDS,
                )?;
                continue;
            }
            let right = self.multiplicative()?;
            let op = if subtract {
                BinaryOp::Subtract
            } else {
                BinaryOp::Add
            };
            left = binary(op, left, right);
        }
    }

    /// `*`, `/`, `DIV`, `%`, `MOD`.
    fn multiplicative(&mut self) -> Result<Expr, ParseError> {
        self.binary_level(Self::bit_xor, &MULTIPLICATIVE, &MULTIPLICATIVE_WORDS)
    }

    /// `^`.
    fn bit_xor(&mut self) -> Result<Expr, ParseError> {
        self.binary_level(Self::unary, &BIT_XOR, &[])
    }

    /// Level of left-associative binary operators, written as a symbol or a
    /// word, whose operands are read by `operand`.
    fn binary_level(
        &mut self,
        operand: fn(&mut Self) -> Result<Expr, ParseError>,
        symbols: &[(Punct, BinaryOp)],
        words: &[(&str, BinaryOp)],
    ) -> Result<Expr, ParseError> {
        let left = operand(self)?;
        self.binary_level_from(left, operand, symbols, words)
    }

    /// Level of left-associative binary operators after its first operand,
    /// `left`.
    fn binary_level_from(
        &mut self,
        mut left: Expr,
        operand: fn(&mut Self) -> Result<Expr, ParseError>,
        symbols: &[(Punct, BinaryOp)],
        words: &[(&str, BinaryOp)],
    ) -> Result<Expr, ParseError> {
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
    pub(super) fn unary(&mut self) -> Result<Expr, ParseError> {
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
        let symbol = self
            .advance()
            .is_some_and(|token| token.kind != TokenKind::Word);
        if symbol
            && op != UnaryOp::Plus
            && self.options.dialect == Dialect::MariaDb
            && self.at_interval()
        {
            // MariaDB reads `- INTERVAL 1 DAY + d` as `d - INTERVAL 1 DAY`,
            // and refuses `~` and `!` before an interval.
            if op == UnaryOp::Minus {
                return Err(self.unsupported(
                    "`- INTERVAL ... + x`, which MariaDB reads as `x - INTERVAL ...`",
                    Span::new(start, self.here().end),
                ));
            }
            return Err(self.expected("an expression"));
        }
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
            TokenKind::NationalString(_) => Ok(self.national()),
            TokenKind::HexString(_)
            | TokenKind::BitString(_)
            | TokenKind::HexNumber(_)
            | TokenKind::BitNumber(_) => Ok(self.bytes_literal()),
            TokenKind::UserVariable { .. } | TokenKind::SystemVariable => {
                Err(self.unsupported("variables", token.span))
            }
            TokenKind::Placeholder => {
                Err(self.unsupported("parameters of prepared statements", token.span))
            }
            TokenKind::Punct(Punct::LeftParen) => self.parenthesized(),
            TokenKind::Punct(Punct::LeftBrace) => self.odbc_literal(),
            TokenKind::Word | TokenKind::QuotedIdent(_) => self.word_expr(),
            _ => Err(self.expected("an expression")),
        }
    }

    /// Strings in a row, joined into one (`'a' 'b'` is `'ab'`).
    fn string(&mut self) -> Expr {
        let start = self.here().start;
        let strings = self.strings(None);
        leaf(
            ExprKind::Literal(Literal::String(strings)),
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

    /// `CASE [operand] WHEN a THEN b ... [ELSE c] END`, from `CASE`.
    fn case(&mut self) -> Result<Expr, ParseError> {
        let start = self.here().start;
        self.advance();
        let operand = if self.at_keyword("WHEN") {
            None
        } else {
            Some(Box::new(self.expr()?))
        };
        let mut branches = Vec::new();
        while self.eat_keyword("WHEN").is_some() {
            let when = self.expr()?;
            self.expect_keyword("THEN")?;
            branches.push((when, self.expr()?));
        }
        if branches.is_empty() {
            return Err(self.expected("`WHEN`"));
        }
        let otherwise = match self.eat_keyword("ELSE") {
            Some(_) => Some(Box::new(self.expr()?)),
            None => None,
        };
        self.expect_keyword("END")?;
        Ok(leaf(
            ExprKind::Case {
                operand,
                branches,
                otherwise,
            },
            self.span_from(start),
        ))
    }

    /// Expression that starts with a word: a keyword literal, `CASE`, a call
    /// with or without parentheses, a column, or a construct not read yet.
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
        if token.kind == TokenKind::Word && self.at_keyword("CASE") {
            return self.case();
        }
        if self.at_interval() {
            return self.interval_first();
        }
        if self.at_introducer() {
            return self.introduced();
        }
        if token.kind == TokenKind::Word {
            if matches!(next, Some(TokenKind::Punct(Punct::LeftParen)))
                && let Some(construct) = self.keyword_construct()?
            {
                return Ok(construct);
            }
            // A word followed by a string: a typed literal, or a column and
            // its alias (`a 'x'`).
            let next = self.peek_at(1).map(|next| &next.kind);
            if matches!(next, Some(TokenKind::String { .. })) {
                let upper = token.text(self.text).to_ascii_uppercase();
                let kind = match upper.as_str() {
                    "DATE" => Some(TemporalKind::Date),
                    "TIME" => Some(TemporalKind::Time),
                    "TIMESTAMP" => Some(TemporalKind::Timestamp),
                    _ => None,
                };
                if let Some(kind) = kind {
                    return self.typed_literal(kind);
                }
            }
            let constructs = [
                "EXISTS",
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
        let before_paren = self
            .peek_at(1)
            .is_some_and(|next| next.kind == TokenKind::Punct(Punct::LeftParen));
        if before_paren && self.at_any_keyword(&["ANY", "SOME", "ALL"]) {
            // A quantifier out of a comparison: `a <=> ANY (...)`.
            return Err(self.expected("an expression"));
        }
        if self.at_call() {
            return self.call();
        }
        if token.kind == TokenKind::Word
            && keywords::is_call_without_parentheses(token.text(self.text), self.options.dialect)
        {
            // `CURRENT_DATE`, `LOCALTIME`: a call without parentheses.
            self.advance();
            let call = FunctionCall {
                name: ObjectName {
                    parts: vec![Ident::new(token.text(self.text))],
                    span: token.span,
                },
                args: Vec::new(),
                star: false,
                distinct: false,
                order_by: Vec::new(),
                separator: None,
                stored: false,
                over: None,
                form: CallForm::Call,
            };
            return Ok(leaf(ExprKind::Function(Box::new(call)), token.span));
        }
        let name: ObjectName = self.object_name(3)?;
        let span = name.span;
        self.json_arrow(leaf(ExprKind::Column(name), span))
    }

    /// `column->path` or `column->>path` after `column` (MySQL), the path a
    /// single string: the `JSON_EXTRACT` of the column and the path, and its
    /// `JSON_UNQUOTE` for `->>`; the column alone otherwise.
    fn json_arrow(&mut self, column: Expr) -> Result<Expr, ParseError> {
        let unquote = if self.at_punct(Punct::LongArrow) {
            true
        } else if self.at_punct(Punct::Arrow) {
            false
        } else {
            return Ok(column);
        };
        let arrow = self.here();
        let construct = if unquote {
            Construct::JsonDoubleArrow
        } else {
            Construct::JsonArrow
        };
        self.check(construct, arrow)?;
        self.advance();
        let Some(TokenKind::String { value, .. }) = self.peek().map(|token| token.kind.clone())
        else {
            return Err(self.expected("a JSON path between quotes"));
        };
        let path_span = self.here();
        self.advance();
        let path = Strings {
            first: value.len(),
            value,
        };
        let start = column.span.start;
        let path = leaf(ExprKind::Literal(Literal::String(path)), path_span);
        let span = self.span_from(start);
        let call = |name: &str, args: Vec<Expr>, form: CallForm| {
            let call = FunctionCall {
                name: ObjectName {
                    parts: vec![Ident::new(name)],
                    span: arrow,
                },
                args,
                star: false,
                distinct: false,
                order_by: Vec::new(),
                separator: None,
                stored: false,
                over: None,
                form,
            };
            leaf(ExprKind::Function(Box::new(call)), span)
        };
        Ok(if unquote {
            let extract = call("JSON_EXTRACT", vec![column, path], CallForm::Call);
            call("JSON_UNQUOTE", vec![extract], CallForm::LongArrow)
        } else {
            call("JSON_EXTRACT", vec![column, path], CallForm::Arrow)
        })
    }
}

/// Operators of the level of `*`, written as a symbol.
const MULTIPLICATIVE: [(Punct, BinaryOp); 3] = [
    (Punct::Star, BinaryOp::Multiply),
    (Punct::Slash, BinaryOp::Divide),
    (Punct::Percent, BinaryOp::Modulo),
];

/// Operators of the level of `*`, written as a word.
const MULTIPLICATIVE_WORDS: [(&str, BinaryOp); 2] =
    [("DIV", BinaryOp::IntegerDivide), ("MOD", BinaryOp::Modulo)];

/// Operator `^`.
const BIT_XOR: [(Punct, BinaryOp); 1] = [(Punct::Caret, BinaryOp::BitXor)];

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
