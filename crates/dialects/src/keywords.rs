//! Keywords of each dialect, shared by the beautifier (capitals, layout) and
//! by the SQL editor's highlighting (API route).
//!
//! The keywords come from the reference file of each dialect
//! (`crates/dialects/reference/<dialect>.yaml`, `reference` module). Words are
//! sorted by category (reserved, non-reserved keywords, types, values); a
//! word can carry the version it appeared in (`since`) or was removed in
//! (`until`). The format is described at the top of each file.

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use serde::Deserialize;

use crate::Dialect;
use crate::reference::reference;
use crate::versions::Version;

/// Keywords of a reference file.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct KeywordFile {
    /// Reserved words.
    reserved: Vec<Entry>,
    /// Non-reserved keywords, unambiguous in practice.
    keywords: Vec<Entry>,
    /// Column types.
    types: Vec<Entry>,
    /// Words that designate a value (`NULL`, `CURRENT_DATE`...).
    literals: Vec<Entry>,
}

/// A word of the file: a plain name, or a name with its versions.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Entry {
    /// Word of all versions.
    Name(String),
    /// Word that appeared, or was removed, in a given version.
    Versioned {
        /// The word.
        name: String,
        /// First version that knows it (included).
        since: Option<String>,
        /// First version that no longer knows it.
        until: Option<String>,
    },
}

impl Entry {
    /// The word as written in the file. Comparisons ignore its case.
    fn name(&self) -> &str {
        match self {
            Self::Name(name) | Self::Versioned { name, .. } => name,
        }
    }

    /// Does the word exist in this server version? Without a known version,
    /// all words are kept.
    fn exists_in(&self, version: Option<&str>) -> bool {
        version.is_none_or(|version| self.exists_at(version_number(version)))
    }

    /// Does the word exist in the version with these numbers?
    fn exists_at(&self, version: [u32; 3]) -> bool {
        let Self::Versioned { since, until, .. } = self else {
            return true;
        };
        let after_since = since
            .as_deref()
            .is_none_or(|since| version >= version_number(since));
        let before_until = until
            .as_deref()
            .is_none_or(|until| version < version_number(until));
        after_since && before_until
    }
}

/// Keywords of a dialect, in capitals, by category.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Keywords {
    /// Reserved words.
    pub reserved: Vec<String>,
    /// Non-reserved keywords.
    pub keywords: Vec<String>,
    /// Column types.
    pub types: Vec<String>,
    /// Words that designate a value.
    pub literals: Vec<String>,
}

/// Keywords of a dialect, limited to those a server version knows; without
/// a version, all those of the file.
///
/// `version` is the one announced by the server (`8.0.36`,
/// `10.11.11-MariaDB`): only its numbers count.
pub fn keywords(dialect: Dialect, version: Option<&str>) -> Keywords {
    let file = file_of(dialect);
    let names = |entries: &[Entry]| -> Vec<String> {
        entries
            .iter()
            .filter(|entry| entry.exists_in(version))
            .map(|entry| entry.name().to_ascii_uppercase())
            .collect()
    };
    Keywords {
        reserved: names(&file.reserved),
        keywords: names(&file.keywords),
        types: names(&file.types),
        literals: names(&file.literals),
    }
}

/// Entries of a dialect's file, all categories together, whose name is `word`.
fn entries_named(word: &str, dialect: Dialect) -> &'static [&'static Entry] {
    index_of(dialect)
        .entries
        .get(&word.to_ascii_uppercase())
        .map_or(&[], Vec::as_slice)
}

/// Tells whether an unquoted word is a keyword of the dialect in this server
/// version (without a version: all of them): it goes to capitals when the
/// query could not be parsed.
pub(crate) fn is_keyword(word: &str, dialect: Dialect, version: Option<&str>) -> bool {
    entries_named(word, dialect)
        .iter()
        .any(|entry| entry.exists_in(version))
}

/// Tells whether a keyword recognised by the parser exists in this server
/// version: a word missing from the file (non-reserved keyword like `CAST`)
/// is taken as known; a word of the file must exist in the version.
pub(crate) fn is_known_in_version(word: &str, dialect: Dialect, version: Option<&str>) -> bool {
    let entries = entries_named(word, dialect);
    entries.is_empty() || entries.iter().any(|entry| entry.exists_in(version))
}

/// Tells whether `word` is a reserved word of the dialect in this version: it
/// cannot name a table, a column or an alias without quotes.
pub fn is_reserved(word: &str, dialect: Dialect, version: Version) -> bool {
    let numbers = [version.major, version.minor, version.patch].map(u32::from);
    index_of(dialect)
        .reserved
        .get(&word.to_ascii_uppercase())
        .is_some_and(|entries| entries.iter().any(|entry| entry.exists_at(numbers)))
}

/// Tells whether an unquoted word is a keyword that does not designate a value
/// (`SELECT`, `AND`, `IN`...): it cannot precede a binary operator.
pub(crate) fn is_structural_keyword(word: &str, dialect: Dialect) -> bool {
    is_keyword(word, dialect, None)
        && !index_of(dialect)
            .values
            .contains(&word.to_ascii_uppercase())
}

/// Tells whether the unquoted `word` calls a built-in function without
/// parentheses in the dialect (`CURRENT_DATE`, `LOCALTIME`): a word that
/// designates a value, `NULL`, `TRUE` and `FALSE` aside.
pub(crate) fn is_call_without_parentheses(word: &str, dialect: Dialect) -> bool {
    let upper = word.to_ascii_uppercase();
    !matches!(upper.as_str(), "NULL" | "TRUE" | "FALSE")
        && index_of(dialect).values.contains(&upper)
}

/// Lookup tables of a dialect's file, built once: a word is found by its name
/// in capitals, instead of a comparison with each entry of the file.
#[derive(Default)]
struct NameIndex {
    /// Entries of each name, in capitals, all categories together.
    entries: HashMap<String, Vec<&'static Entry>>,
    /// Names of the values (`literals`), in capitals.
    values: HashSet<String>,
    /// Entries of the reserved words, by name in capitals.
    reserved: HashMap<String, Vec<&'static Entry>>,
}

/// Lookup tables of a dialect, read only once.
fn index_of(dialect: Dialect) -> &'static NameIndex {
    static MYSQL: OnceLock<NameIndex> = OnceLock::new();
    static MARIADB: OnceLock<NameIndex> = OnceLock::new();
    match dialect {
        Dialect::MySql => MYSQL.get_or_init(|| name_index(file_of(dialect))),
        Dialect::MariaDb => MARIADB.get_or_init(|| name_index(file_of(dialect))),
    }
}

/// Builds the lookup tables of the keywords of a dialect, once for the whole
/// program.
fn name_index(file: &'static KeywordFile) -> NameIndex {
    let mut index = NameIndex::default();
    for entry in [&file.reserved, &file.keywords, &file.types, &file.literals]
        .into_iter()
        .flatten()
    {
        index
            .entries
            .entry(entry.name().to_ascii_uppercase())
            .or_default()
            .push(entry);
    }
    for entry in &file.reserved {
        index
            .reserved
            .entry(entry.name().to_ascii_uppercase())
            .or_default()
            .push(entry);
    }
    index.values = file
        .literals
        .iter()
        .map(|entry| entry.name().to_ascii_uppercase())
        .collect();
    index
}

/// Keywords of a dialect, from its reference file.
fn file_of(dialect: Dialect) -> &'static KeywordFile {
    &reference(dialect).keywords
}

/// Numbers of a version (`10.11.11-MariaDB` gives `[10, 11, 11]`), to compare
/// two versions; missing numbers count as 0.
fn version_number(version: &str) -> [u32; 3] {
    let mut parts = version
        .split(|character: char| !character.is_ascii_digit())
        .take_while(|part| !part.is_empty())
        .map(|part| part.parse().unwrap_or(0));
    [
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_every_dialect_file() {
        for dialect in [Dialect::MySql, Dialect::MariaDb] {
            let all = keywords(dialect, None);
            assert!(all.reserved.contains(&"SELECT".to_string()));
            assert!(all.types.contains(&"VARCHAR".to_string()));
            assert!(all.literals.contains(&"NULL".to_string()));
            assert!(all.keywords.contains(&"COMMIT".to_string()));
        }
    }

    #[test]
    fn keeps_only_the_words_of_the_server_version() {
        let reserved = |version| keywords(Dialect::MySql, version).reserved;
        assert!(reserved(Some("8.0.36")).contains(&"WINDOW".to_string()));
        assert!(!reserved(Some("5.7.44")).contains(&"WINDOW".to_string()));
        assert!(reserved(None).contains(&"WINDOW".to_string()));
        assert!(reserved(Some("5.7.44")).contains(&"SELECT".to_string()));
    }

    #[test]
    fn compares_versions_by_their_numbers() {
        assert_eq!(version_number("10.11.11-MariaDB"), [10, 11, 11]);
        assert_eq!(version_number("8.0"), [8, 0, 0]);
        assert!(version_number("8.0.36") >= version_number("8.0"));
        assert!(version_number("5.7.44") < version_number("8.0"));
    }

    #[test]
    fn tells_structural_keywords_from_values() {
        assert!(is_keyword("select", Dialect::MySql, None));
        assert!(is_keyword("null", Dialect::MySql, None));
        assert!(!is_keyword("window", Dialect::MySql, Some("5.7.44")));
        assert!(is_known_in_version("cast", Dialect::MySql, Some("5.7.44")));
        assert!(!is_known_in_version(
            "window",
            Dialect::MySql,
            Some("5.7.44")
        ));
        assert!(is_structural_keyword("and", Dialect::MySql));
        assert!(!is_structural_keyword("current_date", Dialect::MySql));
        assert!(!is_keyword("my_table", Dialect::MariaDb, None));
    }
}
