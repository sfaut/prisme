//! Reference of each dialect: what Prisme knows of it, version by version.
//!
//! One file per dialect, `crates/dialects/reference/<dialect>.yaml`, the only
//! source of these facts: the keywords (`keywords` module), the constructs
//! of the grammar whose availability depends on the version (`versions`
//! module) and the built-in functions (`builtins` module). Edited directly,
//! embedded in the executable, read only once. The format is described at
//! the top of each file.

use std::sync::OnceLock;

use serde::Deserialize;

use crate::Dialect;
use crate::builtins::Family;
use crate::keywords::KeywordFile;
use crate::versions::{self, ConstructEntry, Version};

/// Content of the reference file of a dialect.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Reference {
    /// Oldest version covered.
    #[serde(deserialize_with = "versions::deserialize_version")]
    pub(crate) floor: Version,
    /// Last version inspected, by section.
    #[allow(
        dead_code,
        reason = "a mark for the updates of the file, read by the tests"
    )]
    pub(crate) inspected: Inspected,
    /// Keywords, by category.
    pub(crate) keywords: KeywordFile,
    /// Constructs of the grammar, with their period of validity.
    pub(crate) constructs: Vec<ConstructEntry>,
    /// Built-in functions, by family.
    pub(crate) functions: Vec<Family>,
}

/// Last version inspected in each section of a reference file: the section
/// is complete up to that version; `None` when the section has not been
/// inspected version by version yet.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code, reason = "a mark for the updates of the file, read by the tests")]
pub(crate) struct Inspected {
    /// Keywords.
    #[serde(default, deserialize_with = "versions::deserialize_optional_version")]
    pub(crate) keywords: Option<Version>,
    /// Constructs of the grammar.
    #[serde(default, deserialize_with = "versions::deserialize_optional_version")]
    pub(crate) constructs: Option<Version>,
    /// Built-in functions.
    #[serde(default, deserialize_with = "versions::deserialize_optional_version")]
    pub(crate) functions: Option<Version>,
}

/// Reference of a dialect, read only once.
pub(crate) fn reference(dialect: Dialect) -> &'static Reference {
    static MYSQL: OnceLock<Reference> = OnceLock::new();
    static MARIADB: OnceLock<Reference> = OnceLock::new();
    match dialect {
        Dialect::MySql => MYSQL.get_or_init(|| parse(include_str!("../reference/mysql.yaml"))),
        Dialect::MariaDb => {
            MARIADB.get_or_init(|| parse(include_str!("../reference/mariadb.yaml")))
        }
    }
}

/// Reads a reference file. The files are part of the program: a format error
/// is a defect of Prisme, reported by the tests.
fn parse(text: &str) -> Reference {
    yaml_serde::from_str(text).expect("unreadable reference file of a dialect")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_inspected_versions_follow_the_floor() {
        for dialect in [Dialect::MySql, Dialect::MariaDb] {
            let reference = reference(dialect);
            let inspected = &reference.inspected;
            assert!(inspected.functions.is_some(), "{dialect:?}");
            for version in [
                inspected.keywords,
                inspected.constructs,
                inspected.functions,
            ]
            .into_iter()
            .flatten()
            {
                assert!(version >= reference.floor, "{dialect:?}: {version}");
            }
        }
    }

    #[test]
    fn reads_the_file_of_each_dialect() {
        assert_eq!(reference(Dialect::MySql).floor, Version::new(8, 0, 11));
        assert_eq!(reference(Dialect::MariaDb).floor, Version::new(10, 6, 3));
    }
}
