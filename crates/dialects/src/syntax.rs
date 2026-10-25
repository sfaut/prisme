//! Syntax error of a query, with its position, to underline it in the editor
//! before anything runs.
//!
//! The error is `sqlparser`'s, whose message ends with its position ("at
//! Line: 2, Column: 8"): the position is extracted from it. A query valid for
//! the server can be refused by `sqlparser` (procedure body, syntax specific
//! to a version): the error then says that Prisme does not understand it,
//! which stays true, since the session refuses queries it cannot parse. The
//! routines, events and index operations of MySQL and MariaDB that
//! `sqlparser` does not know are parsed by Prisme (`mysql_statements.rs`).

use sqlparser::parser::ParserError;

use crate::Dialect;
use crate::parse::parse_statements;

/// Syntax error of a query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxError {
    /// Message of `sqlparser`, without the position.
    pub message: String,
    /// Line of the error in the query, from 1; 0 if unknown (error at the
    /// end of the query, for example).
    pub line: usize,
    /// Column of the error, in characters, from 1; 0 if unknown.
    pub column: usize,
}

/// Marker of the position at the end of a `sqlparser` message.
const LOCATION_MARK: &str = " at Line: ";

/// Parses a single query and returns its syntax error, `None` if it is understood.
pub fn syntax_error(sql: &str, dialect: Dialect) -> Option<SyntaxError> {
    let error = parse_statements(sql, dialect).err()?;
    let text = match error {
        ParserError::ParserError(text) | ParserError::TokenizerError(text) => text,
        ParserError::RecursionLimitExceeded => "query nested too deeply".to_string(),
    };
    Some(with_location(&text))
}

/// Separates a `sqlparser` message from its final position, if it has one.
///
/// Workaround: `ParserError` only gives its position in its text, in
/// English; a change of the wording gives line 0 (`docs/DEVELOPER.md`, section "Workarounds").
fn with_location(text: &str) -> SyntaxError {
    let parsed = text
        .rsplit_once(LOCATION_MARK)
        .and_then(|(message, location)| {
            let (line, column) = location.split_once(", Column: ")?;
            Some(SyntaxError {
                message: message.to_string(),
                line: line.parse().ok()?,
                column: column.trim().parse().ok()?,
            })
        });
    parsed.unwrap_or_else(|| SyntaxError {
        message: text.to_string(),
        line: 0,
        column: 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locates_the_error() {
        let error = syntax_error("SELECT 1\nFROM t WHERE", Dialect::MySql).unwrap();
        assert_eq!((error.line, error.column), (0, 0));

        let error = syntax_error("SELECT *\nFROM t WHERE a = = 1", Dialect::MySql).unwrap();
        assert_eq!((error.line, error.column), (2, 18));
        assert!(!error.message.contains("Line"));

        let error = syntax_error("SELEC 1", Dialect::MariaDb).unwrap();
        assert_eq!((error.line, error.column), (1, 1));
    }

    #[test]
    fn accepts_a_valid_statement() {
        assert_eq!(
            syntax_error("SELECT * FROM t WHERE a = 1", Dialect::MySql),
            None
        );
    }
}
