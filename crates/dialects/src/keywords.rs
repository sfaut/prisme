//! Keywords of each dialect, shared by the beautifier (capitals, layout) and
//! by the SQL editor's highlighting (API route).
//!
//! One file per dialect, `crates/dialects/keywords/<dialect>.yaml`, edited
//! directly: it is embedded in the executable at compile time. Words are
//! sorted by category (reserved, non-reserved keywords, types, values); a
//! word can carry the version it appeared in (`since`) or was removed in
//! (`until`). The format is described at the top of each file.

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use serde::Deserialize;

use crate::Dialect;

/// Content of a keyword file.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct KeywordFile {
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
        let (Self::Versioned { since, until, .. }, Some(version)) = (self, version) else {
            return true;
        };
        let version = version_number(version);
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

/// Tells whether an unquoted word is a keyword that does not designate a value
/// (`SELECT`, `AND`, `IN`...): it cannot precede a binary operator.
pub(crate) fn is_structural_keyword(word: &str, dialect: Dialect) -> bool {
    is_keyword(word, dialect, None)
        && !index_of(dialect)
            .values
            .contains(&word.to_ascii_uppercase())
}

/// Lookup tables of a dialect's file, built once: a word is found by its name
/// in capitals, instead of a comparison with each entry of the file.
#[derive(Default)]
struct NameIndex {
    /// Entries of each name, in capitals, all categories together.
    entries: HashMap<String, Vec<&'static Entry>>,
    /// Names of the values (`literals`), in capitals.
    values: HashSet<String>,
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

/// Builds the lookup tables of a keyword file, read once for the whole program.
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
    index.values = file
        .literals
        .iter()
        .map(|entry| entry.name().to_ascii_uppercase())
        .collect();
    index
}

/// Keyword file of a dialect, read only once.
fn file_of(dialect: Dialect) -> &'static KeywordFile {
    static MYSQL: OnceLock<KeywordFile> = OnceLock::new();
    static MARIADB: OnceLock<KeywordFile> = OnceLock::new();
    match dialect {
        Dialect::MySql => MYSQL.get_or_init(|| parse(include_str!("../keywords/mysql.yaml"))),
        Dialect::MariaDb => MARIADB.get_or_init(|| parse(include_str!("../keywords/mariadb.yaml"))),
    }
}

/// Reads a keyword file. The files are part of the program: a format error is
/// a defect of Prisme, reported by the tests.
fn parse(text: &str) -> KeywordFile {
    yaml_serde::from_str(text).expect("unreadable keyword file")
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
            // CREATE TEMPORARY TABLE, DROP TEMPORARY TABLE.
            assert!(all.keywords.contains(&"TEMPORARY".to_string()));
        }
    }

    #[test]
    fn knows_every_word_of_the_transaction_statements() {
        // Syntax of START TRANSACTION, BEGIN, COMMIT and ROLLBACK, the same
        // in MySQL and MariaDB.
        let statements = [
            "START TRANSACTION WITH CONSISTENT SNAPSHOT, READ WRITE, READ ONLY",
            "BEGIN WORK",
            "COMMIT WORK AND NO CHAIN NO RELEASE",
            "ROLLBACK WORK AND NO CHAIN NO RELEASE",
        ];
        for dialect in [Dialect::MySql, Dialect::MariaDb] {
            for word in statements
                .iter()
                .flat_map(|statement| statement.split([' ', ',']))
                .filter(|word| !word.is_empty())
            {
                assert!(is_keyword(word, dialect, None), "{word} in {dialect:?}");
            }
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
