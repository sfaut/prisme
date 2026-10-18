//! Translation of the patterns of `REGEXP` between the libraries of the
//! servers: ICU in MySQL, PCRE2 in MariaDB. A pattern is read into tokens of
//! the syntax that both libraries share (`token` module), a construct that
//! one of them reads otherwise, or refuses, being refused with its reason;
//! then each token is written for the other library (`write` module), with
//! the case folding of the literal characters (`fold` module).
//!
//! The case of the comparison comes from the collation of the operands: a
//! collation `_ci` ignores it, `_cs`, `_bin` and `binary` keep it, another
//! one, or none (a literal, the result of a function), leaves it unknown,
//! written so that both cases read it alike, or refused. The flags `(?i)`
//! and `(?-i)` set it in their scope.
//!
//! What both libraries read the same way was measured on MySQL 8.0.11 and
//! 8.4.11, MariaDB 10.6.28 and 11.8.9 (`crates/probe/cases/regexp-*.txt`,
//! `regexp-classes.sql`), the classes over all the code points of planes 0
//! to 3 and 14. Each library has its own version of Unicode, a limit of the
//! translation (`docs/DEV-DIALECTS.md`, section 6.3).

mod fold;
mod token;
mod write;

#[cfg(test)]
mod tests;

use crate::Dialect;

/// Library of regular expressions of a server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Library {
    /// ICU, of MySQL.
    Icu,
    /// PCRE2, of MariaDB.
    Pcre2,
}

impl Library {
    /// Library of the servers of `dialect`.
    pub(crate) const fn of(dialect: Dialect) -> Self {
        match dialect {
            Dialect::MySql => Self::Icu,
            Dialect::MariaDb => Self::Pcre2,
        }
    }

    /// Name of the library in the messages.
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Icu => "ICU",
            Self::Pcre2 => "PCRE2",
        }
    }
}

/// Case of the comparison of a pattern, from the collation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Case {
    /// The case counts.
    Sensitive,
    /// The case is ignored.
    Insensitive,
    /// Either.
    Unknown,
}

impl Case {
    /// Case of `collation`: by its suffix `_ci`, `_cs` or `_bin`, or
    /// `binary`.
    pub(crate) fn of_collation(collation: &str) -> Self {
        let collation = collation.to_ascii_lowercase();
        let mut parts = collation.split('_');
        if collation == "binary" || parts.clone().any(|part| part == "cs" || part == "bin") {
            Self::Sensitive
        } else if parts.any(|part| part == "ci") {
            Self::Insensitive
        } else {
            Self::Unknown
        }
    }
}

/// Pattern translated, and the emulations it carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Translation {
    /// The pattern for the output.
    pub pattern: String,
    /// Emulations, for the report.
    pub emulations: Vec<String>,
}

/// Translates `pattern` from `from` to `to`, compared with the case `case`;
/// the reason of a refusal otherwise.
pub(crate) fn translate(
    pattern: &str,
    from: Library,
    to: Library,
    case: Case,
) -> Result<Translation, String> {
    if from == to {
        return Ok(Translation {
            pattern: pattern.to_string(),
            emulations: Vec::new(),
        });
    }
    if pattern.is_empty() {
        return match from {
            Library::Icu => Err("an empty pattern, which ICU refuses".to_string()),
            Library::Pcre2 => Ok(Translation {
                pattern: "(?:)".to_string(),
                emulations: vec![
                    "ICU refuses an empty pattern, which PCRE2 matches everywhere: written `(?:)`"
                        .to_string(),
                ],
            }),
        };
    }
    let tokens = token::tokens(pattern, from)?;
    write::write(pattern, &tokens, to, case)
}
