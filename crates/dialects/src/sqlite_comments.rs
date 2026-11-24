//! Comments of the objects of a SQLite database, read in the text of their
//! `CREATE` statement: SQLite has no `COMMENT` clause, and keeps that text in
//! `sqlite_schema.sql`, its comments included, from `CREATE` to the end of
//! the statement (`docs/SQL-COMPARISON.md`, section 3.18). Prisme reads
//! the comments placed where the object and its columns are named, as DB
//! Browser for SQLite does for the columns:
//!
//! - an object (table, view, index, trigger) takes the comments written right
//!   after its name; a table also takes those written after its opening
//!   parenthesis, on the same line;
//! - a column takes the comments written after its definition on the same
//!   line, its comma included, and those written on the lines just before it.
//!
//! ```sql
//! CREATE TABLE customer -- Customers of the shop
//! (
//!     id INTEGER PRIMARY KEY, -- Identifier
//!     -- Name, as written by the customer
//!     name TEXT NOT NULL
//! );
//! CREATE INDEX customer_name -- Search by name
//! ON customer (name);
//! ```
//!
//! A comment is written `-- ...` or `/* ... */`, on one line or several.
//! Several comments of one element are joined by a space; the text of a
//! comment loses its `--` or `/* */`, and its blanks, line breaks included,
//! become one space. A comment written before
//! `CREATE`, or after the closing parenthesis of a table, is not kept by
//! SQLite; a comment beside a constraint of the table (`PRIMARY KEY (a, b)`)
//! belongs to no column.

use sqlparser::dialect::SQLiteDialect;
use sqlparser::tokenizer::{Token, TokenWithSpan, Tokenizer, Whitespace};

/// Comments of a table: its own, and those of its columns.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct TableComments {
    /// Comment of the table, empty if none.
    pub table: String,
    /// Columns that have a comment, with it, in the order of the table.
    pub columns: Vec<(String, String)>,
}

/// Words that start a constraint of a table, not a column.
const CONSTRAINTS: [&str; 5] = ["CONSTRAINT", "PRIMARY", "UNIQUE", "CHECK", "FOREIGN"];

/// Words that come before the name of the object in a `CREATE` statement.
const KINDS: [&str; 4] = ["TABLE", "INDEX", "VIEW", "TRIGGER"];

/// Comment of the object created by the statement `sql` (table, view, index,
/// trigger): the comments right after its name. Empty if none, or if the
/// text cannot be read.
pub fn object_comment(sql: &str) -> String {
    let tokens = tokens(sql);
    after_name(&tokens).map_or_else(String::new, |at| joined(comments_from(&tokens, at)))
}

/// Comments of the table created by the statement `sql`: its own (after its
/// name, or after its opening parenthesis on the same line), and those of its
/// columns.
pub fn table_comments(sql: &str) -> TableComments {
    let tokens = tokens(sql);
    let Some(at) = after_name(&tokens) else {
        return TableComments::default();
    };
    let mut table = comments_from(&tokens, at);
    // The opening parenthesis of the columns.
    let Some(open) = (at..tokens.len()).find(|&i| tokens[i].token == Token::LParen) else {
        return TableComments {
            table: joined(table),
            columns: Vec::new(),
        };
    };
    let mut columns = Vec::new();
    // Comments waiting for the next definition (written on their own line).
    let mut pending: Vec<String> = Vec::new();
    // Line where the previous element ended: the parenthesis, then the comma
    // of each definition.
    let mut line = tokens[open].span.end.line;
    let mut definition: Option<Definition> = None;
    let mut previous: Option<Definition> = None;
    let mut depth = 0usize;
    for token in &tokens[open + 1..] {
        match &token.token {
            Token::Whitespace(_) if comment_text(&token.token).is_some() => {
                let text = comment_text(&token.token).unwrap_or_default();
                if let Some(current) = definition.as_mut() {
                    // Inside a definition, parentheses included: its own
                    // comment.
                    current.comments.push(text);
                } else if token.span.start.line == line {
                    match previous.as_mut() {
                        Some(previous) => previous.comments.push(text),
                        None => table.push(text),
                    }
                } else {
                    pending.push(text);
                }
            }
            Token::Whitespace(_) => {}
            Token::Comma if depth == 0 => {
                if let Some(done) = definition.take() {
                    line = token.span.end.line;
                    finish(previous.replace(done), &mut columns);
                }
            }
            Token::RParen if depth == 0 => {
                if let Some(done) = definition.take() {
                    finish(previous.replace(done), &mut columns);
                }
                break;
            }
            other => {
                if definition.is_none() {
                    finish(previous.take(), &mut columns);
                    definition = Some(Definition {
                        column: column_name(other),
                        comments: std::mem::take(&mut pending),
                    });
                }
                match other {
                    Token::LParen => depth += 1,
                    Token::RParen => depth = depth.saturating_sub(1),
                    _ => {}
                }
            }
        }
    }
    finish(previous, &mut columns);
    TableComments {
        table: joined(table),
        columns,
    }
}

/// Definition of a table being read: a column (its name), or a constraint.
struct Definition {
    /// Name of the column, `None` for a constraint of the table.
    column: Option<String>,
    /// Its comments.
    comments: Vec<String>,
}

/// Keeps the comment of a finished definition, when it is a column that has
/// one.
fn finish(definition: Option<Definition>, columns: &mut Vec<(String, String)>) {
    if let Some(Definition {
        column: Some(column),
        comments,
    }) = definition
        && !comments.is_empty()
    {
        columns.push((column, joined(comments)));
    }
}

/// Name of the column a definition starts with, `None` for a constraint.
fn column_name(token: &Token) -> Option<String> {
    match token {
        Token::Word(word)
            if word.quote_style.is_some()
                || !CONSTRAINTS
                    .iter()
                    .any(|constraint| word.value.eq_ignore_ascii_case(constraint)) =>
        {
            Some(word.value.clone())
        }
        _ => None,
    }
}

/// Tokens of the text, blanks and comments included; none if the text cannot
/// be read.
fn tokens(sql: &str) -> Vec<TokenWithSpan> {
    Tokenizer::new(&SQLiteDialect {}, sql)
        .tokenize_with_location()
        .unwrap_or_default()
}

/// Position of the token right after the name of the object of a `CREATE`
/// statement: after `TABLE`, `INDEX`, `VIEW` or `TRIGGER`, `IF NOT EXISTS`,
/// then the name, schema included (`main.t`).
fn after_name(tokens: &[TokenWithSpan]) -> Option<usize> {
    let mut words = significant(tokens);
    words.find(|&i| KINDS.iter().any(|kind| is_word(&tokens[i].token, kind)))?;
    let mut name = words.next()?;
    if is_word(&tokens[name].token, "IF") {
        words.next()?;
        words.next()?;
        name = words.next()?;
    }
    let mut end = name;
    // `schema.name`.
    let mut ahead = significant(tokens).skip_while(|&i| i <= name);
    if let Some(dot) = ahead.next()
        && tokens[dot].token == Token::Period
    {
        end = ahead.next()?;
    }
    Some(end + 1)
}

/// Positions of the tokens that are neither blanks nor comments.
fn significant(tokens: &[TokenWithSpan]) -> impl Iterator<Item = usize> + '_ {
    (0..tokens.len()).filter(|&i| !matches!(tokens[i].token, Token::Whitespace(_)))
}

/// Is the token the unquoted word `expected` (in any case)?
fn is_word(token: &Token, expected: &str) -> bool {
    matches!(token, Token::Word(word) if word.quote_style.is_none() && word.value.eq_ignore_ascii_case(expected))
}

/// Comments from the position `at` up to the next token that is neither a
/// blank nor a comment.
fn comments_from(tokens: &[TokenWithSpan], at: usize) -> Vec<String> {
    tokens[at..]
        .iter()
        .map_while(|token| match &token.token {
            Token::Whitespace(_) => Some(comment_text(&token.token)),
            _ => None,
        })
        .flatten()
        .collect()
}

/// Text of a comment token, `None` for another token: without its `--` or
/// `/* */`, nor the `*` that start the lines of a block, its blanks and line
/// breaks turned into single spaces.
fn comment_text(token: &Token) -> Option<String> {
    let words: Vec<&str> = match token {
        Token::Whitespace(Whitespace::SingleLineComment { comment, .. }) => {
            comment.split_whitespace().collect()
        }
        Token::Whitespace(Whitespace::MultiLineComment(comment)) => comment
            .lines()
            .flat_map(|line| {
                let line = line.trim_start();
                // ` * text` in a block, `*` alone at its edges (`/**`, `*/`).
                let line = line
                    .strip_prefix('*')
                    .filter(|rest| rest.is_empty() || rest.starts_with(char::is_whitespace))
                    .unwrap_or(line);
                line.split_whitespace()
            })
            .collect(),
        _ => return None,
    };
    Some(words.join(" "))
}

/// Comments joined by a space, the empty ones left out.
fn joined(comments: Vec<String>) -> String {
    comments
        .into_iter()
        .filter(|comment| !comment.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Columns and their comments, as `(&str, &str)`.
    fn columns(comments: &TableComments) -> Vec<(&str, &str)> {
        comments
            .columns
            .iter()
            .map(|(column, comment)| (column.as_str(), comment.as_str()))
            .collect()
    }

    #[test]
    fn reads_the_comments_of_a_table_and_its_columns() {
        let sql = "CREATE TABLE customer -- Customers of the shop\n\
                   ( /* sold to */\n\
                   \x20   id INTEGER PRIMARY KEY, -- Identifier\n\
                   \x20   -- Name, as written\n\
                   \x20   -- by the customer\n\
                   \x20   name TEXT NOT NULL,\n\
                   \x20   age INTEGER CHECK (age > 0) /* years */ -- or empty\n\
                   )";
        let comments = table_comments(sql);
        assert_eq!(comments.table, "Customers of the shop sold to");
        assert_eq!(
            columns(&comments),
            [
                ("id", "Identifier"),
                ("name", "Name, as written by the customer"),
                ("age", "years or empty"),
            ]
        );
    }

    #[test]
    fn leaves_the_constraints_and_the_columns_without_comments() {
        let sql = "CREATE TABLE IF NOT EXISTS main.\"order line\" (\n\
                   \x20   a INT,\n\
                   \x20   \"b c\" TEXT, -- Quoted\n\
                   \x20   PRIMARY KEY (a, \"b c\") -- Key\n\
                   )";
        let comments = table_comments(sql);
        assert_eq!(comments.table, "");
        assert_eq!(columns(&comments), [("b c", "Quoted")]);
    }

    #[test]
    fn reads_the_comment_after_the_name_of_an_object() {
        assert_eq!(
            object_comment("CREATE INDEX i -- Search by name\nON t (name) -- after"),
            "Search by name"
        );
        assert_eq!(
            object_comment("CREATE UNIQUE INDEX IF NOT EXISTS s.i /* By code */ ON t (code)"),
            "By code"
        );
        assert_eq!(
            object_comment("CREATE VIEW v -- Active\nAS SELECT 1"),
            "Active"
        );
        assert_eq!(
            object_comment(
                "CREATE TEMP TRIGGER tr -- Audit\nAFTER INSERT ON t BEGIN SELECT 1; END"
            ),
            "Audit"
        );
        assert_eq!(object_comment("CREATE VIEW v AS SELECT 1 -- after"), "");
        assert_eq!(object_comment("not sql '"), "");
    }

    #[test]
    fn reads_the_multi_line_comments_of_a_table_and_its_columns() {
        let sql = "CREATE TABLE customer -- Customers\n\
                   -- of the shop\n\
                   /* sold\n\
                   \x20  to */\n\
                   (\n\
                   \x20   id INTEGER PRIMARY KEY, /* Identifier,\n\
                   \x20      never reused */\n\
                   \x20   /*\n\
                   \x20    * Name, as written\n\
                   \x20    */\n\
                   \x20   name TEXT NOT NULL,\n\
                   \x20   score REAL DEFAULT (0 /* none yet */)\n\
                   )";
        let comments = table_comments(sql);
        assert_eq!(comments.table, "Customers of the shop sold to");
        assert_eq!(
            columns(&comments),
            [
                ("id", "Identifier, never reused"),
                ("name", "Name, as written"),
                ("score", "none yet"),
            ]
        );
    }

    #[test]
    fn reads_the_multi_line_comments_of_the_other_objects() {
        for (sql, comment) in [
            (
                "CREATE INDEX i -- Search\n-- by name\nON t (name)",
                "Search by name",
            ),
            (
                "CREATE INDEX i /* Search\n   by name */ ON t (name)",
                "Search by name",
            ),
            (
                "CREATE INDEX i /**\n * Search\n * by *name*\n */ ON t (name)",
                "Search by *name*",
            ),
            (
                "CREATE VIEW v -- Active\n-- customers\nAS SELECT 1",
                "Active customers",
            ),
            (
                "CREATE VIEW v /* Active\n   customers */ AS SELECT 1",
                "Active customers",
            ),
            (
                "CREATE TRIGGER tr -- Audit\n-- of the inserts\nAFTER INSERT ON t BEGIN SELECT 1; END",
                "Audit of the inserts",
            ),
            (
                "CREATE TRIGGER tr /* Audit\n   of the inserts */ AFTER INSERT ON t BEGIN SELECT 1; END",
                "Audit of the inserts",
            ),
        ] {
            assert_eq!(object_comment(sql), comment, "{sql}");
        }
    }
}
