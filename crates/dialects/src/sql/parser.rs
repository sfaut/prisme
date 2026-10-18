//! Parser of the MySQL family: tokens to syntax tree
//! (`docs/DEV-DIALECTS.md`, steps 4 and 5).
//!
//! A recursive descent, one function per rule of the grammar, with the
//! precedence of the operators of the MySQL family (`expr` module). The
//! parser is set by the dialect and its version, checked against the
//! registry of the versions for each construct that depends on them, and by
//! the SQL mode. A construct the dialect has but the parser does not read
//! yet is an explicit error, "not supported yet by the parser of Prisme",
//! with its position.
//!
//! Comments are set aside with their position, for the emitter to write them
//! again; the executable comments whose content is code are listed, for the
//! report.

mod call;
mod cast;
mod expr;
mod from;
mod interval;
mod literal;
mod predicate;
mod string;
mod temporal;

#[cfg(test)]
mod tests;

use std::fmt;

use prisme_plan::{
    AliasForm, Direction, Ident, LimitForm, SelectOption, Span, ValuesForm, WrittenOption,
};

use crate::Dialect;
use crate::keywords::is_reserved;
use crate::lexer::{
    CommentKind, LexError, LexerOptions, Punct, SqlMode, Token, TokenKind, tokenize,
};
use crate::select_options::{self, OPTIONS};
use crate::sql::ast::{
    Alias, CteDefinition, Expr, Limit, ObjectName, OrderItem, Query, QueryBody, Row, Select,
    SelectItem, SetOperation, SetOperator, Statement, StatementKind, Values, With,
};
use crate::versions::{Construct, Version, registry};

/// Settings of the parser.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParseOptions {
    /// Dialect of the query.
    pub dialect: Dialect,
    /// Version of the dialect the query is written in.
    pub version: Version,
    /// SQL mode of the server.
    pub mode: SqlMode,
    /// Version against which the executable comments are resolved: the
    /// version of the server towards the same dialect, the version of the
    /// input dialect otherwise.
    pub conditional_version: Version,
}

/// Query read, with its comments.
#[derive(Debug, Clone, PartialEq)]
pub struct Parsed {
    /// Statement.
    pub statement: Statement,
    /// Comments, in order, with their kind.
    pub comments: Vec<(Span, CommentKind)>,
    /// Executable comments whose content was read as code, markers included.
    pub conditionals: Vec<Span>,
}

/// Query the parser does not read: a syntax error, a construct absent from
/// the dialect or the version, or not supported yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    /// What is wrong.
    pub message: String,
    /// Position of the faulty text.
    pub span: Span,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ParseError {}

impl From<LexError> for ParseError {
    fn from(error: LexError) -> Self {
        Self {
            message: error.message,
            span: error.span,
        }
    }
}

/// Deepest nesting of expressions and queries read, against the overflow of
/// the stack.
const MAX_DEPTH: usize = 100;

/// Reads a single statement, followed by an optional `;`.
///
/// # Errors
///
/// `ParseError` when the text is not a statement of the dialect in its
/// version, or uses a construct the parser does not read yet.
pub fn parse(text: &str, options: &ParseOptions) -> Result<Parsed, ParseError> {
    let lexer = LexerOptions {
        dialect: options.dialect,
        version: options.conditional_version,
        mode: options.mode,
    };
    let mut tokens = Vec::new();
    let mut comments = Vec::new();
    let mut conditionals = Vec::new();
    let mut conditional_start = None;
    for token in tokenize(text, &lexer)? {
        match token.kind {
            TokenKind::Comment(kind) => comments.push((token.span, kind)),
            TokenKind::ConditionalStart { .. } => conditional_start = Some(token.span.start),
            TokenKind::ConditionalEnd => {
                let start = conditional_start.take().unwrap_or(token.span.start);
                conditionals.push(Span::new(start, token.span.end));
            }
            _ => tokens.push(token),
        }
    }
    let mut parser = Parser {
        text,
        tokens,
        pos: 0,
        options,
        depth: 0,
        head: true,
    };
    let statement = parser.statement()?;
    parser.eat_punct(Punct::Semicolon);
    if parser.peek().is_some() {
        return Err(parser.expected("the end of the query"));
    }
    Ok(Parsed {
        statement,
        comments,
        conditionals,
    })
}

/// State of the parser.
struct Parser<'a> {
    /// Text read.
    text: &'a str,
    /// Tokens, comments aside.
    tokens: Vec<Token>,
    /// Next token.
    pos: usize,
    /// Settings.
    options: &'a ParseOptions,
    /// Nesting of the rule being read.
    depth: usize,
    /// Tells whether the next `SELECT` is the first one of the statement,
    /// which takes the options of the first `SELECT` only.
    head: bool,
}

impl Parser<'_> {
    /// Next token.
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    /// Token `offset` positions after the next one.
    fn peek_at(&self, offset: usize) -> Option<&Token> {
        self.tokens.get(self.pos + offset)
    }

    /// Consumes the next token.
    fn advance(&mut self) -> Option<Token> {
        let token = self.tokens.get(self.pos).cloned();
        if token.is_some() {
            self.pos += 1;
        }
        token
    }

    /// Tells whether `token` is the unquoted word `keyword`, in any case.
    fn is_word(&self, token: Option<&Token>, keyword: &str) -> bool {
        token.is_some_and(|token| {
            token.kind == TokenKind::Word && token.text(self.text).eq_ignore_ascii_case(keyword)
        })
    }

    /// Tells whether the next token is the word `keyword`.
    fn at_keyword(&self, keyword: &str) -> bool {
        self.is_word(self.peek(), keyword)
    }

    /// Tells whether the next token is one of the words `keywords`.
    fn at_any_keyword(&self, keywords: &[&str]) -> bool {
        keywords.iter().any(|keyword| self.at_keyword(keyword))
    }

    /// Consumes the word `keyword` when it comes next.
    fn eat_keyword(&mut self, keyword: &str) -> Option<Span> {
        if self.at_keyword(keyword) {
            self.advance().map(|token| token.span)
        } else {
            None
        }
    }

    /// Consumes the word `keyword`, which must come next.
    fn expect_keyword(&mut self, keyword: &str) -> Result<Span, ParseError> {
        self.eat_keyword(keyword)
            .ok_or_else(|| self.expected(&format!("`{keyword}`")))
    }

    /// Tells whether the next token is `punct`.
    fn at_punct(&self, punct: Punct) -> bool {
        self.peek()
            .is_some_and(|token| token.kind == TokenKind::Punct(punct))
    }

    /// Consumes `punct` when it comes next.
    fn eat_punct(&mut self, punct: Punct) -> Option<Span> {
        if self.at_punct(punct) {
            self.advance().map(|token| token.span)
        } else {
            None
        }
    }

    /// Consumes `punct`, which must come next.
    fn expect_punct(&mut self, punct: Punct) -> Result<Span, ParseError> {
        self.eat_punct(punct)
            .ok_or_else(|| self.expected(&format!("`{}`", punct.as_str())))
    }

    /// Position of the next token, or the end of the text.
    fn here(&self) -> Span {
        self.peek().map_or_else(
            || Span::new(self.text.len(), self.text.len()),
            |token| token.span,
        )
    }

    /// From `start` to the end of the last token consumed.
    fn span_from(&self, start: usize) -> Span {
        let end = self
            .pos
            .checked_sub(1)
            .map_or(start, |last| self.tokens[last].span.end);
        Span::new(start, end.max(start))
    }

    /// Error "expected `what`", at the next token.
    fn expected(&self, what: &str) -> ParseError {
        let found = self.peek().map_or_else(
            || "the end of the query".to_string(),
            |token| format!("`{}`", token.text(self.text)),
        );
        ParseError {
            message: format!("expected {what}, found {found}"),
            span: self.here(),
        }
    }

    /// Error for a construct of the dialect the parser does not read yet.
    fn unsupported(&self, what: &str, span: Span) -> ParseError {
        ParseError {
            message: format!("{what}: not supported yet by the parser of Prisme"),
            span,
        }
    }

    /// Checks that the dialect has `construct` in its version.
    fn check(&self, construct: Construct, span: Span) -> Result<(), ParseError> {
        let dialect = self.options.dialect;
        registry(dialect)
            .check(dialect, construct, self.options.version)
            .map_err(|error| ParseError {
                message: error.to_string(),
                span,
            })
    }

    /// Runs `rule` one level deeper.
    fn nested<T>(
        &mut self,
        rule: impl FnOnce(&mut Self) -> Result<T, ParseError>,
    ) -> Result<T, ParseError> {
        if self.depth >= MAX_DEPTH {
            return Err(ParseError {
                message: "query nested too deeply".to_string(),
                span: self.here(),
            });
        }
        self.depth += 1;
        let result = rule(self);
        self.depth -= 1;
        result
    }

    /// Tells whether `token` is a name: a quoted identifier, or a word that is
    /// not reserved in the dialect and its version.
    fn is_name(&self, token: Option<&Token>) -> bool {
        match token.map(|token| &token.kind) {
            Some(TokenKind::QuotedIdent(_)) => true,
            Some(TokenKind::Word) => !is_reserved(
                token.map_or("", |token| token.text(self.text)),
                self.options.dialect,
                self.options.version,
            ),
            _ => false,
        }
    }

    /// Name: a quoted identifier or a word that is not reserved.
    fn name(&mut self) -> Result<Ident, ParseError> {
        if !self.is_name(self.peek()) {
            return Err(self.expected("a name"));
        }
        Ok(self.any_name().expect("checked as a name"))
    }

    /// Name after a dot, where a reserved word is a name too (`t.order`).
    fn name_after_dot(&mut self) -> Result<Ident, ParseError> {
        self.any_name().ok_or_else(|| self.expected("a name"))
    }

    /// Consumes a word or a quoted identifier, as a name.
    fn any_name(&mut self) -> Option<Ident> {
        let token = self.peek()?;
        let ident = match &token.kind {
            TokenKind::Word => Ident::new(token.text(self.text)),
            TokenKind::QuotedIdent(value) => Ident::quoted(value.clone()),
            _ => return None,
        };
        self.pos += 1;
        Some(ident)
    }

    /// Name made of parts separated by dots, `parts` at most.
    fn object_name(&mut self, parts: usize) -> Result<ObjectName, ParseError> {
        let start = self.here().start;
        let mut names = vec![self.name()?];
        while names.len() < parts && self.eat_punct(Punct::Dot).is_some() {
            names.push(self.name_after_dot()?);
        }
        Ok(ObjectName {
            parts: names,
            span: self.span_from(start),
        })
    }

    /// Statement.
    fn statement(&mut self) -> Result<Statement, ParseError> {
        let start = self.here().start;
        if self.starts_query(0) || self.at_punct(Punct::LeftParen) {
            let query = self.query()?;
            return Ok(Statement {
                kind: StatementKind::Query(query),
                span: self.span_from(start),
            });
        }
        let statements = [
            "INSERT", "REPLACE", "UPDATE", "DELETE", "SHOW", "DESCRIBE", "DESC", "EXPLAIN", "USE",
            "SET", "CALL", "DO", "HANDLER",
        ];
        match self.peek() {
            Some(token)
                if statements
                    .iter()
                    .any(|name| self.is_word(Some(token), name)) =>
            {
                let name = token.text(self.text).to_ascii_uppercase();
                Err(self.unsupported(&format!("`{name}` statement"), token.span))
            }
            _ => Err(self.expected("a statement")),
        }
    }

    /// Query: `WITH`, body, `ORDER BY`, `LIMIT`.
    fn query(&mut self) -> Result<Query, ParseError> {
        self.nested(|parser| {
            let start = parser.here().start;
            let with = if parser.at_keyword("WITH") {
                Some(parser.with()?)
            } else {
                None
            };
            let body = parser.query_body()?;
            let order_by = if let Some(order) = parser.eat_keyword("ORDER") {
                parser.expect_keyword("BY")?;
                if matches!(&body, QueryBody::Select(select) if select.rollup) {
                    parser.check(Construct::RollupOrderBy, order)?;
                }
                parser.order_items()?
            } else {
                Vec::new()
            };
            let limit = if parser.at_keyword("LIMIT") {
                Some(parser.limit()?)
            } else {
                None
            };
            if !order_by.is_empty() || limit.is_some() {
                parser.parenthesized_options(&body)?;
            }
            Ok(Query {
                with,
                body,
                order_by,
                limit,
                span: parser.span_from(start),
            })
        })
    }

    /// Query inside another one: a common table expression, a derived table,
    /// a subquery. Its `SELECT` blocks are never the first one of the
    /// statement.
    pub(super) fn inner_query(&mut self) -> Result<Query, ParseError> {
        let head = std::mem::replace(&mut self.head, false);
        let query = self.query();
        self.head = head;
        query
    }

    /// Refuses, in MariaDB, the options of the first `SELECT` of the
    /// statement when `body`, followed by an `ORDER BY` or a `LIMIT`, is that
    /// `SELECT` between parentheses: `(SELECT SQL_CALC_FOUND_ROWS a FROM t
    /// LIMIT 2) LIMIT 1` (error 1234), which a set operation between the
    /// parentheses allows (`select_options` module).
    fn parenthesized_options(&self, body: &QueryBody) -> Result<(), ParseError> {
        let dialect = self.options.dialect;
        let QueryBody::Nested(query) = body else {
            return Ok(());
        };
        let mut body = &query.body;
        while let QueryBody::Nested(query) = body {
            body = &query.body;
        }
        let QueryBody::Select(select) = body else {
            return Ok(());
        };
        let refused = (select.options.iter())
            .find(|written| select_options::first_only(dialect, written.option));
        match refused {
            Some(written) if dialect == Dialect::MariaDb => Err(ParseError {
                message: format!(
                    "incorrect usage/placement of `{}`: MariaDB refuses it in a `SELECT` \
                     between parentheses followed by `ORDER BY` or `LIMIT`",
                    written.option.keyword()
                ),
                span: written.span.unwrap_or(select.span),
            }),
            _ => Ok(()),
        }
    }

    /// `WITH [RECURSIVE] name [(columns)] AS (query), ...`.
    fn with(&mut self) -> Result<With, ParseError> {
        let start = self.expect_keyword("WITH")?.start;
        self.check(Construct::With, Span::new(start, start + 4))?;
        let recursive = self.eat_keyword("RECURSIVE");
        if let Some(span) = recursive {
            self.check(Construct::WithRecursive, span)?;
        }
        let mut ctes = Vec::new();
        loop {
            let cte_start = self.here().start;
            let name = self.name()?;
            let columns = if self.at_punct(Punct::LeftParen) {
                self.name_list()?
            } else {
                Vec::new()
            };
            self.expect_keyword("AS")?;
            self.expect_punct(Punct::LeftParen)?;
            let query = self.inner_query()?;
            self.expect_punct(Punct::RightParen)?;
            if self.at_keyword("CYCLE") {
                return Err(self.unsupported("`CYCLE`", self.here()));
            }
            ctes.push(CteDefinition {
                name,
                columns,
                query: Box::new(query),
                span: self.span_from(cte_start),
            });
            if self.eat_punct(Punct::Comma).is_none() {
                break;
            }
        }
        Ok(With {
            recursive: recursive.is_some(),
            ctes,
            span: self.span_from(start),
        })
    }

    /// `(name, ...)`.
    fn name_list(&mut self) -> Result<Vec<Ident>, ParseError> {
        self.expect_punct(Punct::LeftParen)?;
        let mut names = vec![self.name()?];
        while self.eat_punct(Punct::Comma).is_some() {
            names.push(self.name()?);
        }
        self.expect_punct(Punct::RightParen)?;
        Ok(names)
    }

    /// Body of a query: set operations, `INTERSECT` binding tighter than
    /// `UNION` and `EXCEPT` (MySQL 8.0.31 and later, MariaDB), each one
    /// associating to the left.
    fn query_body(&mut self) -> Result<QueryBody, ParseError> {
        let start = self.here().start;
        let mut left = self.intersect_operand()?;
        loop {
            let operator = if self.at_keyword("UNION") {
                SetOperator::Union
            } else if self.at_keyword("EXCEPT") {
                SetOperator::Except
            } else {
                return Ok(left);
            };
            let all = self.set_quantifier(operator)?;
            let right = self.intersect_operand()?;
            left = self.set_operation(operator, all, left, right, start)?;
        }
    }

    /// Operand of `UNION` and `EXCEPT`: query bodies joined by `INTERSECT`.
    fn intersect_operand(&mut self) -> Result<QueryBody, ParseError> {
        let start = self.here().start;
        let mut left = self.query_primary()?;
        while self.at_keyword("INTERSECT") {
            let all = self.set_quantifier(SetOperator::Intersect)?;
            let right = self.query_primary()?;
            left = self.set_operation(SetOperator::Intersect, all, left, right, start)?;
        }
        Ok(left)
    }

    /// Consumes a set operator, then its `ALL` or `DISTINCT`; tells whether
    /// `ALL` is written.
    fn set_quantifier(&mut self, operator: SetOperator) -> Result<bool, ParseError> {
        let span = self.here();
        self.advance();
        if operator != SetOperator::Union {
            self.check(Construct::IntersectExcept, span)?;
        }
        if let Some(all) = self.eat_keyword("ALL") {
            if operator != SetOperator::Union {
                self.check(
                    Construct::IntersectExceptAll,
                    Span::new(span.start, all.end),
                )?;
            }
            return Ok(true);
        }
        self.eat_keyword("DISTINCT");
        Ok(false)
    }

    /// Set operation of `left` and `right`, from `start`.
    fn set_operation(
        &self,
        operator: SetOperator,
        all: bool,
        left: QueryBody,
        right: QueryBody,
        start: usize,
    ) -> Result<QueryBody, ParseError> {
        if let QueryBody::Nested(query) = &right
            && matches!(query.body, QueryBody::SetOperation(_))
        {
            self.check(Construct::NestedSetOperation, query.span)?;
        }
        Ok(QueryBody::SetOperation(Box::new(SetOperation {
            operator,
            all,
            left,
            right,
            span: self.span_from(start),
        })))
    }

    /// `SELECT`, `VALUES`, `TABLE`, or a query between parentheses.
    fn query_primary(&mut self) -> Result<QueryBody, ParseError> {
        if self.at_keyword("SELECT") {
            return Ok(QueryBody::Select(Box::new(self.select()?)));
        }
        // A `SELECT` after a `VALUES` or a `TABLE` is not the first one.
        if self.at_keyword("VALUES") {
            self.head = false;
            return Ok(QueryBody::Values(self.values()?));
        }
        if let Some(span) = self.eat_keyword("TABLE") {
            self.head = false;
            self.check(Construct::TableStatement, span)?;
            return Ok(QueryBody::Table(self.object_name(2)?));
        }
        if self.eat_punct(Punct::LeftParen).is_some() {
            let query = self.query()?;
            self.expect_punct(Punct::RightParen)?;
            return Ok(QueryBody::Nested(Box::new(query)));
        }
        Err(self.expected("`SELECT`, `VALUES`, `TABLE` or a query between parentheses"))
    }

    /// `VALUES ROW (...), ...` (MySQL) or `VALUES (...), ...` (MariaDB).
    fn values(&mut self) -> Result<Values, ParseError> {
        let start = self.expect_keyword("VALUES")?.start;
        let mut rows = Vec::new();
        let mut form = None;
        loop {
            let row_start = self.here().start;
            let row_form = if let Some(span) = self.eat_keyword("ROW") {
                self.check(Construct::ValuesRow, Span::new(start, span.end))?;
                ValuesForm::Row
            } else {
                if self.options.dialect == Dialect::MySql && self.at_punct(Punct::LeftParen) {
                    return Err(ParseError {
                        message: "MySQL writes the rows of a `VALUES` statement `ROW (...)`"
                            .to_string(),
                        span: self.here(),
                    });
                }
                self.check(Construct::ValuesBare, Span::new(start, start + 6))?;
                ValuesForm::Bare
            };
            form.get_or_insert(row_form);
            self.expect_punct(Punct::LeftParen)?;
            let mut values = vec![self.expr()?];
            while self.eat_punct(Punct::Comma).is_some() {
                values.push(self.expr()?);
            }
            self.expect_punct(Punct::RightParen)?;
            rows.push(Row {
                values,
                span: self.span_from(row_start),
            });
            if self.eat_punct(Punct::Comma).is_none() {
                break;
            }
        }
        Ok(Values {
            rows,
            form: form.unwrap_or(ValuesForm::Row),
            span: self.span_from(start),
        })
    }

    /// `SELECT` block.
    fn select(&mut self) -> Result<Select, ParseError> {
        let start = self.expect_keyword("SELECT")?.start;
        let head = std::mem::replace(&mut self.head, false);
        let (distinct, options) = self.select_options(head)?;
        let mut items = vec![self.select_item()?];
        while self.eat_punct(Punct::Comma).is_some() {
            items.push(self.select_item()?);
        }
        self.refuse_clauses(&["INTO"])?;
        let from = if self.eat_keyword("FROM").is_some() {
            if self.eat_keyword("DUAL").is_some() {
                None
            } else {
                Some(self.table_refs()?)
            }
        } else {
            None
        };
        let filter = if self.eat_keyword("WHERE").is_some() {
            Some(self.expr()?)
        } else {
            None
        };
        let (group_by, rollup) = self.group_by()?;
        let having = if self.eat_keyword("HAVING").is_some() {
            Some(self.expr()?)
        } else {
            None
        };
        let windows = self.window_clause()?;
        self.refuse_clauses(&["INTO", "FOR", "LOCK", "PROCEDURE"])?;
        Ok(Select {
            distinct,
            options,
            items,
            from,
            filter,
            group_by,
            rollup,
            having,
            windows,
            span: self.span_from(start),
        })
    }

    /// `ALL`, `DISTINCT` or `DISTINCTROW`, and the options of the dialect,
    /// in any order after `SELECT`; tells whether `DISTINCT` is written, and
    /// gives each option once. The options of the first `SELECT` only are
    /// refused when it is not `head` (`select_options` module).
    fn select_options(&mut self, head: bool) -> Result<(bool, Vec<WrittenOption>), ParseError> {
        let dialect = self.options.dialect;
        let (mut all, mut distinct) = (false, false);
        let mut options: Vec<WrittenOption> = Vec::new();
        loop {
            let span = self.here();
            if self.eat_keyword("ALL").is_some() {
                all = true;
            } else if self.eat_keyword("DISTINCT").is_some()
                || self.eat_keyword("DISTINCTROW").is_some()
            {
                distinct = true;
            } else if let Some(option) = self.select_option() {
                let keyword = option.keyword();
                let error = |message: String| Err(ParseError { message, span });
                if !head && select_options::first_only(dialect, option) {
                    return error(format!(
                        "incorrect usage/placement of `{keyword}`: only on the first `SELECT` \
                         of the statement"
                    ));
                }
                let cache = [SelectOption::Cache, SelectOption::NoCache];
                if dialect == Dialect::MariaDb && cache.contains(&option) {
                    // MariaDB: errors 1225 and 1221.
                    if options.iter().any(|written| written.option == option) {
                        return error(format!("option `{keyword}` used twice in statement"));
                    }
                    if options
                        .iter()
                        .any(|written| cache.contains(&written.option))
                    {
                        return error(
                            "incorrect usage of `SQL_NO_CACHE` and `SQL_CACHE`".to_string(),
                        );
                    }
                }
                if !options.iter().any(|written| written.option == option) {
                    options.push(WrittenOption {
                        option,
                        span: Some(span),
                    });
                }
            } else {
                break;
            }
            if all && distinct {
                // Error 1221 of both dialects.
                return Err(ParseError {
                    message: "incorrect usage of `ALL` and `DISTINCT`".to_string(),
                    span,
                });
            }
        }
        Ok((distinct, options))
    }

    /// Consumes an option of `SELECT` of the dialect, when one follows.
    fn select_option(&mut self) -> Option<SelectOption> {
        let dialect = self.options.dialect;
        let option = (OPTIONS.iter().copied()).find(|&option| {
            select_options::has(dialect, option) && self.at_keyword(option.keyword())
        })?;
        self.advance();
        Some(option)
    }

    /// `GROUP BY a, b [WITH ROLLUP]`, when written.
    fn group_by(&mut self) -> Result<(Vec<Expr>, bool), ParseError> {
        if self.eat_keyword("GROUP").is_none() {
            return Ok((Vec::new(), false));
        }
        self.expect_keyword("BY")?;
        let mut keys = Vec::new();
        loop {
            keys.push(self.expr()?);
            if self.at_any_keyword(&["ASC", "DESC"]) {
                // MariaDB sorts the groups; MySQL 8 refuses it.
                return Err(self.unsupported("`ASC` and `DESC` in `GROUP BY`", self.here()));
            }
            if self.eat_punct(Punct::Comma).is_none() {
                break;
            }
        }
        let rollup = self.is_word(self.peek(), "WITH") && self.is_word(self.peek_at(1), "ROLLUP");
        if rollup {
            self.advance();
            self.advance();
        }
        Ok((keys, rollup))
    }

    /// Refuses the clauses `keywords`, not read yet.
    fn refuse_clauses(&self, keywords: &[&str]) -> Result<(), ParseError> {
        match keywords.iter().find(|keyword| self.at_keyword(keyword)) {
            Some(keyword) => Err(self.unsupported(&format!("`{keyword}` clause"), self.here())),
            None => Ok(()),
        }
    }

    /// Item of a select list: `*`, `t.*`, or an expression with its alias.
    fn select_item(&mut self) -> Result<SelectItem, ParseError> {
        let start = self.here().start;
        if self.eat_punct(Punct::Star).is_some() {
            return Ok(SelectItem::Wildcard {
                qualifier: None,
                span: self.span_from(start),
            });
        }
        if let Some(parts) = self.qualified_wildcard_parts() {
            let qualifier = self.object_name(parts)?;
            self.expect_punct(Punct::Dot)?;
            self.expect_punct(Punct::Star)?;
            return Ok(SelectItem::Wildcard {
                qualifier: Some(qualifier),
                span: self.span_from(start),
            });
        }
        let expr = self.expr()?;
        let alias = self.alias(true)?;
        Ok(SelectItem::Expr {
            expr,
            alias,
            span: self.span_from(start),
        })
    }

    /// Number of parts of the qualifier of a `t.*` or `db.t.*` that comes
    /// next.
    fn qualified_wildcard_parts(&self) -> Option<usize> {
        let is_star = |offset| {
            self.peek_at(offset)
                .is_some_and(|token| token.kind == TokenKind::Punct(Punct::Star))
        };
        let is_dot = |offset| {
            self.peek_at(offset)
                .is_some_and(|token| token.kind == TokenKind::Punct(Punct::Dot))
        };
        if !self.is_name(self.peek()) || !is_dot(1) {
            return None;
        }
        if is_star(2) {
            return Some(1);
        }
        (is_dot(3) && is_star(4)).then_some(2)
    }

    /// Alias after an expression or a table: `AS name`, or a bare name. A
    /// column alias may also be a string.
    fn alias(&mut self, column: bool) -> Result<Option<Alias>, ParseError> {
        let string = |token: Option<&Token>| match token.map(|token| &token.kind) {
            Some(TokenKind::String { value, .. }) => Some(value.clone()),
            _ => None,
        };
        let form = if self.eat_keyword("AS").is_some() {
            AliasForm::As
        } else if self.is_name(self.peek()) || (column && string(self.peek()).is_some()) {
            AliasForm::Bare
        } else {
            return Ok(None);
        };
        if column && let Some(value) = string(self.peek()) {
            self.pos += 1;
            return Ok(Some(Alias {
                name: Ident::quoted(value),
                form,
            }));
        }
        Ok(Some(Alias {
            name: self.name()?,
            form,
        }))
    }

    /// Keys of an `ORDER BY`.
    fn order_items(&mut self) -> Result<Vec<OrderItem>, ParseError> {
        let mut items = Vec::new();
        loop {
            let start = self.here().start;
            let expr = self.expr()?;
            let direction = if self.eat_keyword("ASC").is_some() {
                Some(Direction::Ascending)
            } else if self.eat_keyword("DESC").is_some() {
                Some(Direction::Descending)
            } else {
                None
            };
            items.push(OrderItem {
                expr,
                direction,
                span: self.span_from(start),
            });
            if self.eat_punct(Punct::Comma).is_none() {
                return Ok(items);
            }
        }
    }

    /// `LIMIT count`, `LIMIT count OFFSET offset` or `LIMIT offset, count`.
    fn limit(&mut self) -> Result<Limit, ParseError> {
        let start = self.expect_keyword("LIMIT")?.start;
        let first = self.limit_value()?;
        let (count, offset, form) = if self.eat_punct(Punct::Comma).is_some() {
            (self.limit_value()?, Some(first), LimitForm::Comma)
        } else if self.eat_keyword("OFFSET").is_some() {
            (first, Some(self.limit_value()?), LimitForm::Offset)
        } else {
            (first, None, LimitForm::Offset)
        };
        Ok(Limit {
            count,
            offset,
            form,
            span: self.span_from(start),
        })
    }

    /// Value of a `LIMIT`: an integer.
    fn limit_value(&mut self) -> Result<crate::sql::ast::Expr, ParseError> {
        match self.peek().map(|token| &token.kind) {
            Some(TokenKind::Integer) => self.expr_primary(),
            Some(TokenKind::Placeholder) => {
                Err(self.unsupported("parameters of prepared statements", self.here()))
            }
            _ => Err(self.expected("an integer")),
        }
    }
}
