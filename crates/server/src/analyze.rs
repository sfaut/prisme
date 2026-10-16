//! Analysis of the editor's text, requested by the interface shortly after
//! each keystroke: splitting into queries, to highlight the one Ctrl+Enter
//! would run, and syntax errors, to underline them.
//!
//! The splitting is that of the query sessions (`splitter` crate): the
//! interface shows exactly the query that will be sent. Each query is then
//! parsed on its own (`prisme_dialects::syntax`). Positions are in UTF-16
//! units, like the editor's.

use prisme_connectors::mysql::Flavor;
use prisme_dialects::Dialect;
use prisme_dialects::syntax::syntax_error;
use prisme_splitter::{Statement, split};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Text to analyse.
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct AnalyzeRequest {
    /// Whole text of the editor.
    pub sql: String,
    /// Flavor of the session's server, which sets the dialect of the analysis.
    pub flavor: Flavor,
}

/// Result of the analysis.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct Analysis {
    /// Queries of the text, in order.
    pub statements: Vec<StatementSpan>,
    /// Syntax errors, at most one per query.
    pub errors: Vec<SyntaxIssue>,
}

/// Syntax error to underline in the editor.
#[derive(Debug, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct SyntaxIssue {
    /// Start of the underlined passage, in UTF-16 units.
    pub start: usize,
    /// End of the underlined passage, in UTF-16 units.
    pub end: usize,
    /// Message shown on hover.
    pub message: String,
}

/// Location of a query in the text, in UTF-16 units.
#[derive(Debug, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct StatementSpan {
    /// First significant character of the query.
    pub start: usize,
    /// End of the query, separator included.
    pub end: usize,
}

/// SQL dialect of a server flavor.
pub fn dialect_of(flavor: Flavor) -> Dialect {
    match flavor {
        Flavor::MySql => Dialect::MySql,
        Flavor::MariaDb => Dialect::MariaDb,
    }
}

/// Analyses the editor's text.
pub fn analyze(request: &AnalyzeRequest) -> Analysis {
    let dialect = dialect_of(request.flavor);
    let statements = split(&request.sql);
    Analysis {
        errors: statements
            .iter()
            .filter_map(|statement| issue(statement, dialect))
            .collect(),
        statements: statements
            .iter()
            .map(|statement| StatementSpan {
                start: statement.start,
                end: statement.end,
            })
            .collect(),
    }
}

/// Syntax error of a query, placed in the editor's text.
///
/// The underlined passage is the word that starts at the error's position;
/// without a known position (unexpected end of query), the last character of
/// the query.
fn issue(statement: &Statement, dialect: Dialect) -> Option<SyntaxIssue> {
    let error = syntax_error(&statement.sql, dialect)?;
    let (start, end) = locate(&statement.sql, error.line, error.column)
        .unwrap_or_else(|| last_character(&statement.sql));
    Some(SyntaxIssue {
        start: statement.start + start,
        end: statement.start + end,
        message: format!("Query not understood by Prisme: {}", error.message),
    })
}

/// Passage of the word that starts at line `line`, column `column` (in
/// characters, from 1): start and end in UTF-16 units, from the start of
/// `sql`. `None` if the position is unknown or outside the text.
fn locate(sql: &str, line: usize, column: usize) -> Option<(usize, usize)> {
    let (mut current_line, mut current_column, mut offset) = (1, 1, 0);
    let mut chars = sql.chars().peekable();
    while let Some(&character) = chars.peek() {
        if (current_line, current_column) == (line, column) {
            let word: usize = chars
                .take_while(|character| !character.is_whitespace())
                .map(char::len_utf16)
                .sum();
            return Some((offset, offset + word.max(1)));
        }
        if character == '\n' {
            (current_line, current_column) = (current_line + 1, 1);
        } else {
            current_column += 1;
        }
        offset += character.len_utf16();
        chars.next();
    }
    None
}

/// Passage of the last character of `sql`, in UTF-16 units (empty if `sql` is empty).
fn last_character(sql: &str) -> (usize, usize) {
    let end: usize = sql.encode_utf16().count();
    let width = sql.chars().last().map_or(0, char::len_utf16);
    (end - width, end)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gives_the_place_of_each_statement() {
        let request = AnalyzeRequest {
            sql: "SELECT 1;\n-- é\nSELECT 2".to_string(),
            flavor: Flavor::MariaDb,
        };
        assert_eq!(
            analyze(&request).statements,
            vec![
                StatementSpan { start: 0, end: 9 },
                StatementSpan { start: 15, end: 23 },
            ]
        );
    }

    #[test]
    fn underlines_the_word_at_the_error() {
        // "é" counts for one UTF-16 unit, "😀" for two.
        let request = AnalyzeRequest {
            sql: "SELECT 'é';\nSELECT '😀'\nFROM t WHERE a = = 1;".to_string(),
            flavor: Flavor::MySql,
        };
        let errors = analyze(&request).errors;
        assert_eq!(errors.len(), 1);
        let start = request.sql.find("= 1").unwrap();
        let start = request.sql[..start].encode_utf16().count();
        assert_eq!((errors[0].start, errors[0].end), (start, start + 1));
        assert!(
            errors[0]
                .message
                .starts_with("Query not understood by Prisme")
        );
    }

    #[test]
    fn underlines_the_last_character_of_an_unfinished_statement() {
        let request = AnalyzeRequest {
            sql: "SELECT * FROM t WHERE".to_string(),
            flavor: Flavor::MySql,
        };
        let errors = analyze(&request).errors;
        assert_eq!((errors[0].start, errors[0].end), (20, 21));
    }
}
