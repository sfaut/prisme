//! Lexical rules of a dialect: what the splitter needs to tell a separator
//! from a character inside a string, a name or a comment, and what the
//! editor's highlighting needs to read the same text.
//!
//! Each dialect declares its rules (`prisme_dialects`): the splitter has no
//! code of its own per dialect.

use serde::Serialize;
use ts_rs::TS;

/// Lexical rules of a dialect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct LexicalRules {
    /// Does `#` open a comment up to the end of the line (MySQL)?
    pub hash_comments: bool,
    /// Does `--` open a comment only when a space, a control character or
    /// the end of the text follows it (MySQL: `5--2` is a subtraction)?
    pub space_after_dashes: bool,
    /// Does a backslash escape the next character of a string (MySQL)?
    pub backslash_escapes: bool,
    /// Are double quotes those of a string (MySQL), rather than those of a
    /// name (SQLite)? The splitter skips their content either way.
    pub double_quoted_strings: bool,
    /// Characters that quote a name, square brackets apart (`` ` `` for
    /// MySQL, `"` and `` ` `` for SQLite).
    pub identifier_quotes: &'static str,
    /// Do square brackets quote a name (`[a b]`, SQLite)? A `]` ends it: it
    /// does not double.
    pub bracket_identifiers: bool,
    /// Is the `DELIMITER` command of the `mysql` client recognised?
    pub delimiter_command: bool,
    /// Does the body of a `CREATE TRIGGER` hold statements ended by `;`, up
    /// to a `END` that follows a `;` (SQLite, as `sqlite3_complete` reads it)?
    pub trigger_bodies: bool,
}

/// Where a statement stands with respect to the body of a trigger, word after
/// word, as `sqlite3_complete` follows it: a separator ends the statement,
/// except inside the body of a `CREATE [TEMP] TRIGGER`, which only `; END ;`
/// closes. A `CASE ... END;` of the body does not, its `END` following no `;`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Body {
    /// Start of the statement.
    #[default]
    Start,
    /// After `EXPLAIN`, at the start.
    Explain,
    /// After `CREATE`, and `TEMP` or `TEMPORARY`.
    Create,
    /// A statement without a body: its separator ends it.
    Plain,
    /// Inside the body of a trigger.
    Trigger,
    /// Right after a `;` of the body.
    Semicolon,
    /// After `; END`: the next separator ends the trigger.
    End,
}

impl Body {
    /// State after the word `word`.
    pub(crate) fn word(self, word: &str) -> Self {
        let is = |keyword: &str| word.eq_ignore_ascii_case(keyword);
        match self {
            Self::Start if is("EXPLAIN") => Self::Explain,
            Self::Start | Self::Explain if is("CREATE") => Self::Create,
            Self::Create if is("TEMP") || is("TEMPORARY") => Self::Create,
            Self::Create if is("TRIGGER") => Self::Trigger,
            Self::Semicolon if is("END") => Self::End,
            Self::Trigger | Self::Semicolon | Self::End => Self::Trigger,
            _ => Self::Plain,
        }
    }

    /// State after a token that is no word (string, name, number sign...).
    pub(crate) fn other(self) -> Self {
        match self {
            Self::Trigger | Self::Semicolon | Self::End => Self::Trigger,
            _ => Self::Plain,
        }
    }

    /// State after a separator, `None` if the separator ends the statement.
    pub(crate) fn separator(self) -> Option<Self> {
        matches!(self, Self::Trigger | Self::Semicolon).then_some(Self::Semicolon)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Does the separator after these words end the statement?
    fn ends_after(words: &[&str]) -> bool {
        let state = words.iter().fold(Body::default(), |state, word| {
            if *word == ";" {
                state.separator().unwrap_or_default()
            } else {
                state.word(word)
            }
        });
        state.separator().is_none()
    }

    #[test]
    fn closes_a_trigger_only_after_a_semicolon_and_end() {
        assert!(ends_after(&["SELECT", "1"]));
        assert!(!ends_after(&[
            "CREATE", "TRIGGER", "tr", "BEGIN", "SELECT", "1"
        ]));
        assert!(!ends_after(&[
            "CREATE", "temp", "TRIGGER", "tr", "BEGIN", "SELECT", "1", ";", "SELECT", "2"
        ]));
        assert!(ends_after(&[
            "CREATE", "TRIGGER", "tr", "BEGIN", "SELECT", "1", ";", "END"
        ]));
        assert!(ends_after(&[
            "EXPLAIN",
            "CREATE",
            "TEMPORARY",
            "TRIGGER",
            "tr",
            "BEGIN",
            "SELECT",
            "1",
            ";",
            "end"
        ]));
        // The END of a CASE follows no `;`.
        assert!(!ends_after(&[
            "CREATE", "TRIGGER", "tr", "BEGIN", "SELECT", "CASE", "1", "END"
        ]));
        assert!(ends_after(&["CREATE", "TABLE", "trigger_log"]));
    }
}
