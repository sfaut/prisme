//! Beautifier layout: line breaks, indentation and spaces.
//!
//! Rules (see `docs/V1.md`, section 2.9):
//!
//! - each main clause starts a line: `SELECT`, `FROM`, joins, `WHERE`,
//!   `GROUP BY`, `HAVING`, `ORDER BY`, `LIMIT`, `UNION`...;
//! - a list of several columns after `SELECT` puts one column per line,
//!   indented; a single column stays on the `SELECT` line;
//! - several predicates in `WHERE`, `HAVING` or `ON`: one per line, `AND` and
//!   `OR` at the start of the line, indented under the clause (the `AND` of a
//!   `BETWEEN` and those of a `CASE` stay on the line); `ON` stays on the
//!   line of its join;
//! - when there are several, predicates are preceded by `TRUE` (if the first
//!   link is `AND`) or by `FALSE` (if it is `OR`), so that each has its line,
//!   `AND` or `OR` first (see `predicate_prefix`);
//! - a subquery between parentheses is indented, its closing parenthesis on
//!   its own line;
//! - a space around binary operators (`=`, `<`, `+`, `-`...), none after a
//!   unary operator (`-1`); a space after a comma;
//! - `ALL` follows each `SELECT` that has neither `DISTINCT`, nor
//!   `DISTINCTROW`, nor `ALL` already (except in `GRANT` and `REVOKE`, where
//!   `SELECT` is a privilege);
//! - two queries are separated by an empty line.
//!
//! The text of each token is taken as is (strings, comments, identifiers
//! between backticks, case of words): only spaces and line breaks change. A
//! function name stays stuck to its parenthesis if it was, as MySQL requires
//! it for its functions. A text that contains the `DELIMITER` command, or that
//! the tokenizer does not understand, is returned as is: the beautifier first
//! cuts the text around the `DELIMITER` commands, and lays out each query
//! separately. The body of a stored program (procedure, function...) follows
//! more rules (`program` module).

use sqlparser::tokenizer::{Token, Tokenizer, Whitespace};

use crate::Dialect;
use crate::keywords::is_structural_keyword;
use crate::limit::byte_offset;

mod program;

/// Width of an indentation level.
const INDENT: usize = 4;

/// Clauses that start a line.
const CLAUSES: &[&str] = &[
    "SELECT",
    "FROM",
    "WHERE",
    "GROUP",
    "HAVING",
    "ORDER",
    "LIMIT",
    "UNION",
    "INTERSECT",
    "EXCEPT",
    "WINDOW",
    "SET",
    "VALUES",
];

/// Words that start a join (and therefore a line).
const JOIN_STARTS: &[&str] = &["JOIN", "INNER", "LEFT", "RIGHT", "CROSS", "NATURAL", "FULL"];

/// Words that precede `JOIN` in the same join.
const JOIN_MODIFIERS: &[&str] = &[
    "INNER", "LEFT", "RIGHT", "OUTER", "CROSS", "NATURAL", "FULL",
];

/// Options of a `SELECT`, which stay on its line before the column list.
const SELECT_OPTIONS: &[&str] = &[
    "ALL",
    "DISTINCT",
    "DISTINCTROW",
    "HIGH_PRIORITY",
    "STRAIGHT_JOIN",
    "SQL_SMALL_RESULT",
    "SQL_BIG_RESULT",
    "SQL_BUFFER_RESULT",
    "SQL_NO_CACHE",
    "SQL_CACHE",
    "SQL_CALC_FOUND_ROWS",
];

/// Clauses that end the column list of a `SELECT`.
const SELECT_LIST_ENDS: &[&str] = &[
    "FROM",
    "WHERE",
    "GROUP",
    "HAVING",
    "ORDER",
    "LIMIT",
    "UNION",
    "INTERSECT",
    "EXCEPT",
    "INTO",
    "WINDOW",
    "FOR",
];

/// Words that end a list of predicates (`WHERE`, `HAVING`, `ON`).
const PREDICATES_ENDS: &[&str] = &["ON", "USING", "FOR", "INTO", "LOCK"];

/// Text added right after a keyword.
enum Injection {
    /// `TRUE` or `FALSE` after `WHERE`, `HAVING` or `ON`, then the first link on a new line.
    Prefix {
        constant: &'static str,
        connective: &'static str,
        indent: usize,
    },
    /// `ALL` after a `SELECT` without `DISTINCT` (`SELECT ALL` is equivalent to `SELECT`).
    All,
}

/// Token to place, with its original text.
struct Item<'a> {
    token: Token,
    text: &'a str,
    /// Did the original text have a space (or a line break) before this token?
    space_before: bool,
    /// Keyword of the dialect that does not designate a value (`SELECT`,
    /// `AND`...), computed while tokenising, where the dialect is known.
    structural: bool,
}

impl Item<'_> {
    /// Unquoted word, in capitals, or empty string for another token.
    fn word(&self) -> String {
        match &self.token {
            Token::Word(word) if word.quote_style.is_none() => word.value.to_ascii_uppercase(),
            _ => String::new(),
        }
    }

    /// Is the token the (unquoted) word `keyword`?
    fn is(&self, keyword: &str) -> bool {
        self.word() == keyword
    }
}

/// Current clause of a query, to place the `AND`s and `OR`s.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Clause {
    /// Column list of a `SELECT`.
    Select,
    /// `WHERE`, `HAVING` or `ON` of a join: predicates indented under the clause.
    Filter,
    /// Other clause.
    Other,
}

/// Query being laid out (the main query, or a subquery).
struct Query {
    /// Parenthesis depth where the clauses of the query live.
    depth: usize,
    /// Indentation of the clauses of the query.
    indent: usize,
    /// Current clause.
    clause: Clause,
    /// Does the column list of the current `SELECT` have several columns?
    multi_columns: bool,
    /// Is the column list still waiting for its first column?
    columns_pending: bool,
    /// A `BETWEEN` waits for its `AND`, which stays on the line.
    between_pending: bool,
    /// Number of open `CASE`s: their `AND`s and `OR`s stay on the line.
    open_cases: usize,
    /// Is the last opened clause a join (waiting for its `ON`)?
    joined: bool,
}

impl Query {
    fn new(depth: usize, indent: usize) -> Self {
        Self {
            depth,
            indent,
            clause: Clause::Other,
            multi_columns: false,
            columns_pending: false,
            between_pending: false,
            open_cases: 0,
            joined: false,
        }
    }
}

/// Lays out `sql`. See the rules at the top of the module.
///
/// With `program`, `sql` is a single query that can be a stored program,
/// whose `;` separate the statements of the body (`program` module).
pub(crate) fn layout(sql: &str, dialect: Dialect, program: bool) -> String {
    if has_delimiter_command(sql) {
        return sql.to_string();
    }
    let parser_dialect = dialect.parser_dialect();
    let Ok(tokens) = Tokenizer::new(parser_dialect.as_ref(), sql).tokenize_with_location() else {
        return sql.to_string();
    };

    let mut items = Vec::new();
    let mut space_before = false;
    for token in tokens {
        if matches!(
            token.token,
            Token::Whitespace(Whitespace::Space | Whitespace::Newline | Whitespace::Tab)
        ) {
            space_before = true;
            continue;
        }
        let (Some(start), Some(end)) = (
            byte_offset(sql, token.span.start),
            byte_offset(sql, token.span.end),
        ) else {
            return sql.to_string();
        };
        let structural = matches!(
            &token.token,
            Token::Word(word) if word.quote_style.is_none() && is_structural_keyword(&word.value, dialect)
        );
        items.push(Item {
            token: token.token,
            text: &sql[start..end],
            space_before,
            structural,
        });
        space_before = false;
    }
    Printer {
        program,
        statement_start: program,
        ..Printer::default()
    }
    .print(&items)
}

/// Tells whether the text contains a `DELIMITER` command of the `mysql` client.
pub(crate) fn has_delimiter_command(sql: &str) -> bool {
    sql.lines().any(|line| {
        line.split_whitespace()
            .next()
            .is_some_and(|word| word.eq_ignore_ascii_case("delimiter"))
    })
}

/// Open parenthesis.
struct Paren {
    /// Does it open a subquery?
    subquery: bool,
    /// Indentation of the line where it was opened.
    line_indent: usize,
}

/// Stack of the queries being laid out: the main query, then the subqueries
/// opened inside it, innermost last. The main query is always there, so the
/// current query always exists.
struct QueryStack {
    /// The main query, which never leaves the stack.
    main: Query,
    /// Subqueries opened inside the main query, innermost last.
    nested: Vec<Query>,
}

impl QueryStack {
    /// Current query, the innermost one.
    fn current(&self) -> &Query {
        self.nested.last().unwrap_or(&self.main)
    }

    /// Current query, the innermost one, for writing.
    fn current_mut(&mut self) -> &mut Query {
        self.nested.last_mut().unwrap_or(&mut self.main)
    }

    /// Opens a subquery.
    fn push(&mut self, query: Query) {
        self.nested.push(query);
    }

    /// Closes the innermost subquery; the main query stays.
    fn pop(&mut self) {
        self.nested.pop();
    }

    /// Replaces the whole stack by a single main query.
    fn reset(&mut self, main: Query) {
        self.main = main;
        self.nested.clear();
    }
}

impl Default for QueryStack {
    fn default() -> Self {
        Self {
            main: Query::new(0, 0),
            nested: Vec::new(),
        }
    }
}

/// Writes the laid-out text, token by token.
#[derive(Default)]
struct Printer {
    out: String,
    queries: QueryStack,
    parens: Vec<Paren>,
    /// The next token starts a line (after an end-of-line comment).
    newline_pending: bool,
    /// The previous token was a unary operator, `(` or `.`: the next one sticks to it.
    glue_next: bool,
    /// The previous token was a binary operator or a comma: the next one is
    /// preceded by a space.
    space_next: bool,
    /// Text to add after the word being placed.
    injection: Option<Injection>,
    /// First word of the current query (`GRANT`, `SELECT`...).
    statement_word: String,
    /// Is the text a stored program (ended by a separator set by
    /// `DELIMITER`)? Its `;` then separate the statements of the body.
    program: bool,
    /// Open blocks of the program (`BEGIN`, `IF`, `WHILE`...).
    blocks: Vec<program::Block>,
    /// Does the next word start a statement of the program?
    statement_start: bool,
    /// The previous word was the `END` of a block: `IF`, `WHILE`... that
    /// follow complete it (`END IF`).
    end_pending: bool,
}

impl Printer {
    /// Lays out all the tokens and returns the text.
    fn print(mut self, items: &[Item]) -> String {
        for index in 0..items.len() {
            self.place(items, index);
        }
        self.out.trim_end().to_string()
    }

    /// Current query (the innermost one).
    fn query(&mut self) -> &mut Query {
        self.queries.current_mut()
    }

    /// Is the token at the clause level of the current query?
    fn at_query_level(&self) -> bool {
        self.queries.current().depth == self.parens.len()
    }

    /// Places token `index`.
    fn place(&mut self, items: &[Item], index: usize) {
        let item = &items[index];
        let previous = index.checked_sub(1).map(|previous| &items[previous]);
        let next = items.get(index + 1);
        let word = item.word();

        if self.newline_pending {
            self.newline_pending = false;
            let indent = self.current_line_indent();
            self.newline(indent);
        }

        // End of a statement of a program body: new line, without an empty line.
        if self.program && matches!(item.token, Token::SemiColon) && self.parens.is_empty() {
            self.end_pending = false;
            self.end_program_statement(item);
            return;
        }

        // End of a query: empty line before the next one.
        if matches!(item.token, Token::SemiColon) && self.parens.is_empty() {
            self.push_glued(item.text);
            self.out.push_str("\n\n");
            self.queries.reset(Query::new(0, 0));
            self.statement_word.clear();
            return;
        }
        if self.statement_word.is_empty() && !word.is_empty() {
            self.statement_word.clone_from(&word);
        }

        if let Token::Whitespace(comment) = &item.token {
            self.place_comment(item, comment);
            return;
        }

        if self.program
            && matches!(item.token, Token::Word(_))
            && self.place_program_word(item, next)
        {
            return;
        }

        if matches!(item.token, Token::LParen) {
            self.open_paren(item, previous, next);
            return;
        }
        if matches!(item.token, Token::RParen) {
            self.close_paren(item);
            return;
        }

        // In GRANT and REVOKE, SELECT, INSERT... are privileges, not clauses.
        let structured = !matches!(self.statement_word.as_str(), "GRANT" | "REVOKE");
        if structured && self.at_query_level() && !word.is_empty() {
            let query = self.queries.current();
            let starts_predicates = word == "WHERE"
                || word == "HAVING"
                || (word == "ON" && query.joined && query.clause == Clause::Other);
            if starts_predicates
                && let Some((constant, connective)) = predicate_prefix(&items[index + 1..])
            {
                self.injection = Some(Injection::Prefix {
                    constant,
                    connective,
                    indent: query.indent + INDENT,
                });
            }
            if word == "SELECT"
                && !matches!(self.statement_word.as_str(), "GRANT" | "REVOKE")
                && !next.is_some_and(|next| {
                    matches!(next.word().as_str(), "ALL" | "DISTINCT" | "DISTINCTROW")
                })
            {
                self.injection = Some(Injection::All);
            }
            let several_columns = word == "SELECT" && has_several_columns(&items[index + 1..]);
            self.place_keyword_line(&word, previous, next, several_columns);
        }

        // First column of a list of several columns: new line, indented.
        let query = self.query();
        if query.clause == Clause::Select
            && query.columns_pending
            && !SELECT_OPTIONS.contains(&word.as_str())
            && !item.is("SELECT")
        {
            query.columns_pending = false;
            if query.multi_columns {
                let indent = query.indent + INDENT;
                self.newline(indent);
            }
        }

        if matches!(item.token, Token::Comma) {
            self.push_glued(item.text);
            let query = self.queries.current();
            if query.clause == Clause::Select
                && query.multi_columns
                && query.depth == self.parens.len()
            {
                let indent = query.indent + INDENT;
                self.newline(indent);
            } else {
                self.space_next = true;
            }
            return;
        }

        if matches!(item.token, Token::Period) {
            self.push_glued(item.text);
            self.glue_next = true;
            return;
        }

        if is_operator(&item.token) {
            if previous.is_some_and(is_operand) {
                self.push_spaced(item.text);
                self.space_next = true;
            } else {
                // Unary operator (-1): stuck to what follows. A lone *
                // (SELECT *, t.*, db.*) stays a token in its own right.
                self.push_spaced(item.text);
                self.glue_next = !matches!(item.token, Token::Mul);
            }
            return;
        }

        if item.space_before || !word.is_empty() {
            self.push_spaced(item.text);
        } else {
            self.push_glued(item.text);
        }

        match self.injection.take() {
            Some(Injection::Prefix {
                constant,
                connective,
                indent,
            }) => {
                self.push_spaced(constant);
                self.newline(indent);
                self.push_spaced(connective);
            }
            Some(Injection::All) => self.push_spaced("ALL"),
            None => {}
        }
    }

    /// Breaks the line before a clause, a join or an `AND` / `OR`, and tracks
    /// the current clause.
    fn place_keyword_line(
        &mut self,
        word: &str,
        previous: Option<&Item>,
        next: Option<&Item>,
        several_columns: bool,
    ) {
        let function_call =
            next.is_some_and(|next| matches!(next.token, Token::LParen) && !next.space_before);
        let query_indent = self.query().indent;

        if word == "CASE" {
            self.query().open_cases += 1;
            return;
        }
        if word == "END" && self.query().open_cases > 0 {
            self.query().open_cases -= 1;
            return;
        }
        if word == "BETWEEN" {
            self.query().between_pending = true;
            return;
        }

        if (word == "AND" || word == "OR") && self.query().open_cases == 0 {
            if word == "AND" && self.query().between_pending {
                self.query().between_pending = false;
                return;
            }
            if self.query().clause == Clause::Filter {
                self.newline(query_indent + INDENT);
            }
            return;
        }

        if word == "ON" && matches!(self.query().clause, Clause::Other) && self.after_join() {
            self.query().clause = Clause::Filter;
            return;
        }

        let joins = JOIN_STARTS.contains(&word)
            && !function_call
            && !(word == "JOIN"
                && previous
                    .is_some_and(|previous| JOIN_MODIFIERS.contains(&previous.word().as_str())))
            && !(word == "STRAIGHT_JOIN");
        if joins {
            self.newline(query_indent);
            let query = self.query();
            query.clause = Clause::Other;
            query.joined = true;
            return;
        }

        if CLAUSES.contains(&word) && !function_call {
            // FROM right after DELETE (and its options) stays on its line:
            // DELETE FROM t. After a list of tables (DELETE t1 FROM...), it
            // starts a line as elsewhere.
            let delete_from = word == "FROM"
                && previous.is_some_and(|previous| {
                    matches!(
                        previous.word().as_str(),
                        "DELETE" | "LOW_PRIORITY" | "QUICK" | "IGNORE"
                    )
                })
                && self.statement_word == "DELETE";
            // BY follows GROUP and ORDER on the same line.
            if !delete_from {
                self.newline(query_indent);
            }
            let query = self.query();
            query.between_pending = false;
            query.joined = false;
            query.clause = match word {
                "SELECT" => Clause::Select,
                "WHERE" | "HAVING" => Clause::Filter,
                _ => Clause::Other,
            };
            if word == "SELECT" {
                query.columns_pending = true;
                query.multi_columns = several_columns;
            }
        }
    }

    /// Is the last opened clause a join (to recognise its `ON`)?
    fn after_join(&self) -> bool {
        self.queries.current().joined
    }

    /// Places an opening parenthesis; a subquery is indented.
    fn open_paren(&mut self, item: &Item, previous: Option<&Item>, next: Option<&Item>) {
        // After a word, the original space is kept: a function name stays
        // stuck to its parenthesis (MySQL requires it for its functions).
        let after_word = previous.is_some_and(|previous| matches!(previous.token, Token::Word(_)));
        if !after_word || item.space_before {
            self.push_spaced(item.text);
        } else {
            self.push_glued(item.text);
        }
        self.glue_next = true;

        let subquery = next.is_some_and(|next| next.is("SELECT") || next.is("WITH"));
        let line_indent = self.current_line_indent();
        self.parens.push(Paren {
            subquery,
            line_indent,
        });
        if subquery {
            let indent = line_indent + INDENT;
            self.queries.push(Query::new(self.parens.len(), indent));
            self.newline(indent);
        }
    }

    /// Places a closing parenthesis; that of a subquery has its own line.
    fn close_paren(&mut self, item: &Item) {
        let Some(paren) = self.parens.pop() else {
            self.push_glued(item.text);
            return;
        };
        if paren.subquery {
            self.queries.pop();
            self.newline(paren.line_indent);
            self.out.push_str(item.text);
        } else {
            self.push_glued(item.text);
        }
    }

    /// Places a comment: an end-of-line comment forces a line break.
    fn place_comment(&mut self, item: &Item, comment: &Whitespace) {
        self.push_spaced(item.text.trim_end());
        if matches!(comment, Whitespace::SingleLineComment { .. }) {
            self.newline_pending = true;
        }
    }

    /// Adds a text, preceded by a space if the current line is not empty.
    fn push_spaced(&mut self, text: &str) {
        self.space_next = false;
        let glued = std::mem::take(&mut self.glue_next);
        let at_line_start =
            self.out.is_empty() || self.out.ends_with('\n') || self.out.ends_with(' ');
        if !glued && !at_line_start {
            self.out.push(' ');
        }
        self.out.push_str(text);
    }

    /// Adds a text stuck to what precedes.
    fn push_glued(&mut self, text: &str) {
        if std::mem::take(&mut self.space_next) {
            self.push_spaced(text);
            return;
        }
        self.glue_next = false;
        let trimmed = self.out.trim_end_matches(' ').len();
        if !self.out.ends_with('\n') && !self.out[..trimmed].ends_with('\n') {
            self.out.truncate(trimmed);
        }
        self.out.push_str(text);
    }

    /// Breaks the line with indentation `indent`, except at the very start of the text.
    fn newline(&mut self, indent: usize) {
        self.glue_next = false;
        self.space_next = false;
        let trimmed = self.out.trim_end_matches(' ').len();
        self.out.truncate(trimmed);
        if self.out.is_empty() {
            return;
        }
        if !self.out.ends_with('\n') {
            self.out.push('\n');
        }
        self.out.push_str(&" ".repeat(indent));
    }

    /// Indentation of the current line.
    fn current_line_indent(&self) -> usize {
        let line = self.out.rsplit('\n').next().unwrap_or("");
        line.len() - line.trim_start_matches(' ').len()
    }
}

/// Tells whether the column list of a `SELECT` (the tokens that follow it)
/// has several columns, separated by a comma at its level.
fn has_several_columns(items: &[Item]) -> bool {
    let mut depth = 0usize;
    for item in items {
        match item.token {
            Token::LParen => depth += 1,
            Token::RParen if depth == 0 => return false,
            Token::RParen => depth -= 1,
            Token::SemiColon if depth == 0 => return false,
            Token::Comma if depth == 0 => return true,
            _ if depth == 0 && SELECT_LIST_ENDS.contains(&item.word().as_str()) => return false,
            _ => {}
        }
    }
    false
}

/// Prefix to add before a list of predicates (the tokens that follow
/// `WHERE`, `HAVING` or `ON`), so that each predicate has its line: `TRUE`
/// then `AND` if the first link of the list is `AND`, `FALSE` then `OR` if it
/// is `OR`. `None` for a single predicate, or if the prefix is already there.
///
/// The meaning of the query does not change: in three-valued logic,
/// `TRUE AND x` and `FALSE OR x` equal `x`, including when `x` is NULL. And
/// since `AND` binds tighter than `OR` (then `XOR`), `TRUE AND p1 OR p2` reads
/// `(TRUE AND p1) OR p2`, and `FALSE OR p1 AND p2` reads
/// `FALSE OR (p1 AND p2)`: the prefix only attaches to the first term,
/// whatever the mix.
fn predicate_prefix(items: &[Item]) -> Option<(&'static str, &'static str)> {
    let mut significant = items
        .iter()
        .filter(|item| !matches!(item.token, Token::Whitespace(_)));
    let first = significant.next()?.word();
    let second = significant.next().map(Item::word).unwrap_or_default();
    if (first == "TRUE" && second == "AND") || (first == "FALSE" && second == "OR") {
        return None;
    }

    let mut depth = 0usize;
    let mut open_cases = 0usize;
    let mut between_pending = false;
    for (index, item) in items.iter().enumerate() {
        match item.token {
            Token::LParen => depth += 1,
            Token::RParen if depth == 0 => return None,
            Token::RParen => depth -= 1,
            Token::SemiColon if depth == 0 => return None,
            _ => {}
        }
        if depth > 0 {
            continue;
        }
        let word = item.word();
        let function_call = items
            .get(index + 1)
            .is_some_and(|next| matches!(next.token, Token::LParen) && !next.space_before);
        let ends = CLAUSES.contains(&word.as_str())
            || SELECT_LIST_ENDS.contains(&word.as_str())
            || PREDICATES_ENDS.contains(&word.as_str())
            || (JOIN_STARTS.contains(&word.as_str()) && !function_call);
        if ends {
            return None;
        }
        match word.as_str() {
            "CASE" => open_cases += 1,
            "END" if open_cases > 0 => open_cases -= 1,
            "BETWEEN" => between_pending = true,
            "AND" if between_pending => between_pending = false,
            "AND" if open_cases == 0 => return Some(("TRUE", "AND")),
            "OR" if open_cases == 0 => return Some(("FALSE", "OR")),
            _ => {}
        }
    }
    None
}

/// Tells whether a token is an operator, binary or unary.
fn is_operator(token: &Token) -> bool {
    matches!(
        token,
        Token::Eq
            | Token::DoubleEq
            | Token::Neq
            | Token::Lt
            | Token::Gt
            | Token::LtEq
            | Token::GtEq
            | Token::Spaceship
            | Token::Plus
            | Token::Minus
            | Token::Mul
            | Token::Div
            | Token::Mod
            | Token::StringConcat
            | Token::Assignment
            | Token::Ampersand
            | Token::Pipe
            | Token::Caret
            | Token::ShiftLeft
            | Token::ShiftRight
            | Token::Arrow
            | Token::LongArrow
    )
}

/// Tells whether a token ends an operand: the operator that follows it is binary.
fn is_operand(item: &Item) -> bool {
    match &item.token {
        Token::Word(_) => !item.structural,
        Token::RParen | Token::Placeholder(_) | Token::Number(..) => true,
        Token::SingleQuotedString(_)
        | Token::DoubleQuotedString(_)
        | Token::NationalStringLiteral(_)
        | Token::HexStringLiteral(_)
        | Token::SingleQuotedByteStringLiteral(_)
        | Token::DoubleQuotedByteStringLiteral(_) => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn laid_out(sql: &str) -> String {
        layout(sql, Dialect::MariaDb, false)
    }

    /// Lays out the body of a stored program.
    fn program(sql: &str) -> String {
        layout(sql, Dialect::MariaDb, true)
    }

    #[test]
    fn keeps_delete_from_on_one_line() {
        assert_eq!(
            laid_out("DELETE FROM logs WHERE a = 1"),
            "DELETE FROM logs\nWHERE a = 1"
        );
        assert_eq!(
            laid_out("DELETE LOW_PRIORITY QUICK FROM logs"),
            "DELETE LOW_PRIORITY QUICK FROM logs"
        );
        // Deletion on several tables: FROM follows the list of tables.
        assert_eq!(
            laid_out("DELETE l FROM logs l JOIN u ON u.id = l.uid"),
            "DELETE l\nFROM logs l\nJOIN u ON u.id = l.uid"
        );
    }

    #[test]
    fn indents_the_blocks_of_a_stored_program() {
        assert_eq!(
            program(
                "create procedure p(n int) begin declare i int default 0; \
                 if n > 0 then delete from logs; elseif n < 0 then select 1; else select 2; end if; \
                 while i < n do set i = i + 1; end while; \
                 my_loop: loop leave my_loop; end loop my_loop; \
                 repeat set i = i - 1; until i = 0 end repeat; \
                 case n when 1 then select 1; else begin end; end case; end"
            ),
            "create procedure p(n int)\n\
             BEGIN\n    \
                 declare i int default 0;\n    \
                 IF n > 0 THEN\n        \
                     delete from logs;\n    \
                 ELSEIF n < 0 THEN\n        \
                     select ALL 1;\n    \
                 ELSE\n        \
                     select ALL 2;\n    \
                 END IF;\n    \
                 WHILE i < n DO\n        \
                     set i = i + 1;\n    \
                 END WHILE;\n    \
                 my_loop: LOOP\n        \
                     leave my_loop;\n    \
                 END LOOP my_loop;\n    \
                 REPEAT\n        \
                     set i = i - 1;\n    \
                 UNTIL i = 0\n    \
                 END REPEAT;\n    \
                 CASE n\n        \
                     WHEN 1 THEN\n            \
                         select ALL 1;\n        \
                     ELSE\n            \
                         BEGIN\n            \
                         END;\n    \
                 END CASE;\n\
             END"
        );
    }

    #[test]
    fn keeps_expression_case_and_do_statements_inside_a_program() {
        assert_eq!(
            program(
                "begin while x do do sleep(1); set y = case when a then 1 else 2 end; end while; end"
            ),
            "BEGIN\n    \
                 WHILE x DO\n        \
                     do sleep(1);\n        \
                     set y = case when a then 1 else 2 end;\n    \
                 END WHILE;\n\
             END"
        );
    }

    #[test]
    fn puts_each_clause_on_its_own_line() {
        assert_eq!(
            laid_out(
                "SELECT ALL * FROM video WHERE created_at >= CURRENT_DATE - INTERVAL 120 SECOND ORDER BY id DESC LIMIT 10;"
            ),
            "SELECT ALL *\nFROM video\nWHERE created_at >= CURRENT_DATE - INTERVAL 120 SECOND\nORDER BY id DESC\nLIMIT 10;"
        );
    }

    #[test]
    fn puts_one_column_per_line_when_there_are_several() {
        assert_eq!(
            laid_out("SELECT DISTINCT id, count(*), LEFT(name, 3) AS prefix FROM t"),
            "SELECT DISTINCT\n    id,\n    count(*),\n    LEFT(name, 3) AS prefix\nFROM t"
        );
    }

    #[test]
    fn puts_one_predicate_per_line_in_where_and_on() {
        let sql = "SELECT v.id FROM video v JOIN person p ON p.id = v.person_id AND p.active=1 \
                   LEFT JOIN tag t ON t.video_id = v.id WHERE v.year BETWEEN 2000 AND 2010 AND v.score>3 OR v.free";
        assert_eq!(
            laid_out(sql),
            "SELECT ALL v.id\nFROM video v\nJOIN person p ON TRUE\n    AND p.id = v.person_id\n    AND p.active = 1\nLEFT JOIN tag t ON t.video_id = v.id\nWHERE TRUE\n    AND v.year BETWEEN 2000 AND 2010\n    AND v.score > 3\n    OR v.free"
        );
    }

    #[test]
    fn keeps_case_conditions_on_their_line() {
        assert_eq!(
            laid_out("SELECT CASE WHEN a AND b THEN 1 ELSE 0 END FROM t WHERE x AND y"),
            "SELECT ALL CASE WHEN a AND b THEN 1 ELSE 0 END\nFROM t\nWHERE TRUE\n    AND x\n    AND y"
        );
    }

    #[test]
    fn prefixes_predicates_with_true_or_false() {
        assert_eq!(
            laid_out("SELECT * FROM t1 INNER JOIN t2 ON p1 AND p2 WHERE p3 OR p4"),
            "SELECT ALL *\nFROM t1\nINNER JOIN t2 ON TRUE\n    AND p1\n    AND p2\nWHERE FALSE\n    OR p3\n    OR p4"
        );
        // A single predicate: nothing to add.
        assert_eq!(
            laid_out("SELECT * FROM t WHERE a = 1"),
            "SELECT ALL *\nFROM t\nWHERE a = 1"
        );
        // Prefix already present: it is not doubled.
        assert_eq!(
            laid_out("SELECT * FROM t WHERE FALSE OR a OR b"),
            "SELECT ALL *\nFROM t\nWHERE FALSE\n    OR a\n    OR b"
        );
        // The links of a subquery or a function do not count.
        assert_eq!(
            laid_out("SELECT * FROM t WHERE a IN (SELECT x FROM u WHERE y OR z)"),
            "SELECT ALL *\nFROM t\nWHERE a IN (\n    SELECT ALL x\n    FROM u\n    WHERE FALSE\n        OR y\n        OR z\n)"
        );
    }

    #[test]
    fn adds_all_after_select_without_distinct() {
        assert_eq!(laid_out("SELECT a FROM t"), "SELECT ALL a\nFROM t");
        assert_eq!(
            laid_out("SELECT DISTINCTROW a FROM t"),
            "SELECT DISTINCTROW a\nFROM t"
        );
        assert_eq!(laid_out("SELECT ALL a FROM t"), "SELECT ALL a\nFROM t");
        // In GRANT and REVOKE, SELECT is a privilege: nothing to add.
        assert_eq!(
            laid_out("GRANT SELECT ON db.* TO u"),
            "GRANT SELECT ON db.* TO u"
        );
    }

    #[test]
    fn keeps_grant_and_star_as_written() {
        // In GRANT and REVOKE, SELECT is a privilege, not a clause.
        assert_eq!(
            laid_out("GRANT SELECT, INSERT ON db.* TO u"),
            "GRANT SELECT, INSERT ON db.* TO u"
        );
        // A lone * does not stick to what follows it.
        assert_eq!(laid_out("SELECT t.* FROM t"), "SELECT ALL t.*\nFROM t");
    }

    #[test]
    fn indents_subqueries() {
        assert_eq!(
            laid_out("SELECT id FROM video WHERE id IN (SELECT video_id FROM fav WHERE ok = 1)"),
            "SELECT ALL id\nFROM video\nWHERE id IN (\n    SELECT ALL video_id\n    FROM fav\n    WHERE ok = 1\n)"
        );
    }

    #[test]
    fn spaces_binary_operators_only() {
        assert_eq!(
            laid_out("SELECT a-b, -1, a - -1, t.*, x=-1 FROM t WHERE (a+b)*2>=c"),
            "SELECT ALL\n    a - b,\n    -1,\n    a - -1,\n    t.*,\n    x = -1\nFROM t\nWHERE (a + b) * 2 >= c"
        );
    }

    #[test]
    fn keeps_comments_strings_and_function_parentheses() {
        assert_eq!(
            laid_out(
                "SELECT `a  b`, 'x   y' -- note\nFROM t /* c */ WHERE f (1) = concat('a',  'b')"
            ),
            "SELECT ALL\n    `a  b`,\n    'x   y' -- note\nFROM t /* c */\nWHERE f (1) = concat('a', 'b')"
        );
    }

    #[test]
    fn separates_statements_with_a_blank_line() {
        assert_eq!(
            laid_out("SELECT 1; select 2;"),
            "SELECT ALL 1;\n\nselect ALL 2;"
        );
    }

    #[test]
    fn leaves_delimiter_scripts_unchanged() {
        let sql = "DELIMITER //\nCREATE PROCEDURE p() BEGIN SELECT 1; END //";
        assert_eq!(laid_out(sql), sql);
    }
}
