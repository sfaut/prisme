//! Mots-clefs de chaque dialecte, partagés par l'enjoliveur (capitales,
//! mise en page) et par la coloration de l'éditeur SQL (route de l'API).
//!
//! Un fichier par dialecte, `crates/dialects/keywords/<dialecte>.yaml`, à
//! modifier directement : il est embarqué dans l'exécutable à la
//! compilation. Les mots y sont rangés par catégorie (réservés, mots-clefs
//! non réservés, types, valeurs) ; un mot peut porter la version de son
//! apparition (`since`) ou de son retrait (`until`). Le format est décrit en
//! tête de chaque fichier.

use std::sync::OnceLock;

use serde::Deserialize;

use crate::Dialect;

/// Contenu d'un fichier de mots-clefs.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct KeywordFile {
    /// Mots réservés.
    reserved: Vec<Entry>,
    /// Mots-clefs non réservés, sans ambiguïté en pratique.
    keywords: Vec<Entry>,
    /// Types de colonnes.
    types: Vec<Entry>,
    /// Mots qui désignent une valeur (`NULL`, `CURRENT_DATE`...).
    literals: Vec<Entry>,
}

/// Un mot du fichier : un simple nom, ou un nom avec ses versions.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Entry {
    /// Mot de toutes les versions.
    Name(String),
    /// Mot apparu, ou retiré, à une version donnée.
    Versioned {
        /// Le mot.
        name: String,
        /// Première version qui le connaît (incluse).
        since: Option<String>,
        /// Première version qui ne le connaît plus.
        until: Option<String>,
    },
}

impl Entry {
    /// Le mot, en capitales.
    fn name(&self) -> String {
        match self {
            Self::Name(name) | Self::Versioned { name, .. } => name.to_ascii_uppercase(),
        }
    }

    /// Le mot existe-t-il dans cette version du serveur ? Sans version
    /// connue, tous les mots sont retenus.
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

/// Mots-clefs d'un dialecte, en capitales, par catégorie.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Keywords {
    /// Mots réservés.
    pub reserved: Vec<String>,
    /// Mots-clefs non réservés.
    pub keywords: Vec<String>,
    /// Types de colonnes.
    pub types: Vec<String>,
    /// Mots qui désignent une valeur.
    pub literals: Vec<String>,
}

/// Mots-clefs d'un dialecte, limités à ceux qu'une version du serveur
/// connaît ; sans version, tous ceux du fichier.
///
/// `version` est celle annoncée par le serveur (`8.0.36`,
/// `10.11.11-MariaDB`) : seuls ses nombres comptent.
pub fn keywords(dialect: Dialect, version: Option<&str>) -> Keywords {
    let file = file_of(dialect);
    let names = |entries: &[Entry]| -> Vec<String> {
        entries
            .iter()
            .filter(|entry| entry.exists_in(version))
            .map(Entry::name)
            .collect()
    };
    Keywords {
        reserved: names(&file.reserved),
        keywords: names(&file.keywords),
        types: names(&file.types),
        literals: names(&file.literals),
    }
}

/// Indique si un mot non protégé est un mot-clef du dialecte, toutes
/// versions confondues : il passe en capitales.
pub(crate) fn is_keyword(word: &str, dialect: Dialect) -> bool {
    let file = file_of(dialect);
    let upper = word.to_ascii_uppercase();
    [&file.reserved, &file.keywords, &file.types, &file.literals]
        .iter()
        .any(|entries| entries.iter().any(|entry| entry.name() == upper))
}

/// Indique si un mot non protégé est un mot-clef qui ne désigne pas une valeur
/// (`SELECT`, `AND`, `IN`...) : il ne peut pas précéder un opérateur binaire.
pub(crate) fn is_structural_keyword(word: &str, dialect: Dialect) -> bool {
    let upper = word.to_ascii_uppercase();
    is_keyword(word, dialect)
        && !file_of(dialect)
            .literals
            .iter()
            .any(|entry| entry.name() == upper)
}

/// Fichier de mots-clefs d'un dialecte, lu une seule fois.
fn file_of(dialect: Dialect) -> &'static KeywordFile {
    static MYSQL: OnceLock<KeywordFile> = OnceLock::new();
    static MARIADB: OnceLock<KeywordFile> = OnceLock::new();
    match dialect {
        Dialect::MySql => MYSQL.get_or_init(|| parse(include_str!("../keywords/mysql.yaml"))),
        Dialect::MariaDb => MARIADB.get_or_init(|| parse(include_str!("../keywords/mariadb.yaml"))),
    }
}

/// Lit un fichier de mots-clefs. Les fichiers font partie du programme :
/// une erreur de format est un défaut de Prisme, signalé par les tests.
fn parse(text: &str) -> KeywordFile {
    yaml_serde::from_str(text).expect("fichier de mots-clefs illisible")
}

/// Nombres d'une version (`10.11.11-MariaDB` donne `[10, 11, 11]`), pour
/// comparer deux versions ; les nombres absents valent 0.
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
        assert!(is_keyword("select", Dialect::MySql));
        assert!(is_keyword("null", Dialect::MySql));
        assert!(is_structural_keyword("and", Dialect::MySql));
        assert!(!is_structural_keyword("current_date", Dialect::MySql));
        assert!(!is_keyword("ma_table", Dialect::MariaDb));
    }
}
