//! Registry of the versions of each dialect: the period of validity of each
//! construct of the grammar (`docs/DEV-DIALECTS.md`, step 2).
//!
//! The constructs come from the reference file of each dialect
//! (`crates/dialects/reference/<dialect>.yaml`, `reference` module). The user
//! chooses the version of the dialect they write in; the parser accepts a
//! construct only within its period, and the emitter checks the version of
//! the server the same way.
//! A construct missing from the file of a dialect is absent from it: each
//! dialect stays in its domain.
//!
//! The versions offered to the user are those where the dialect changed (a
//! construct or a built-in function, `builtins` module), from the oldest
//! version covered (`floor`, the first GA release of MySQL 8.0 and of
//! MariaDB 10.6).

use std::fmt;
use std::sync::OnceLock;

use serde::Deserialize;

use crate::Dialect;
use crate::builtins;
use crate::reference::reference;

/// Version of a dialect: `8.0.19`, `10.6.3`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version {
    /// Major number.
    pub major: u16,
    /// Minor number.
    pub minor: u16,
    /// Patch number.
    pub patch: u16,
}

/// Construct of the grammar whose availability depends on the dialect and
/// its version. The identifier of each one is its name in snake case in the
/// registry files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Construct {
    /// `WITH` (common table expressions).
    With,
    /// `WITH RECURSIVE`.
    WithRecursive,
    /// Window functions (`OVER`).
    WindowFunctions,
    /// `JSON_TABLE`.
    JsonTable,
    /// `column->path` (MySQL).
    JsonArrow,
    /// `column->>path` (MySQL).
    JsonDoubleArrow,
    /// `LATERAL` derived tables.
    Lateral,
    /// Column list of a derived table: `(SELECT 1, 2) AS d (a, b)`.
    DerivedColumnList,
    /// Derived table that reads columns of an enclosing query.
    DerivedOuterReference,
    /// Set operation between parentheses as the right operand of another:
    /// `a UNION (b UNION c)`.
    NestedSetOperation,
    /// `ORDER BY` of a query grouped `WITH ROLLUP`.
    RollupOrderBy,
    /// `EXPLAIN FORMAT=TREE`.
    ExplainFormatTree,
    /// `value MEMBER OF (array)`.
    MemberOf,
    /// `EXPLAIN ANALYZE`.
    ExplainAnalyze,
    /// `VALUES ROW (...)` statement (MySQL).
    ValuesRow,
    /// `VALUES (...)` table value constructor (MariaDB).
    ValuesBare,
    /// `TABLE t` statement.
    TableStatement,
    /// Row alias of `INSERT ... AS new ON DUPLICATE KEY UPDATE`.
    InsertRowAlias,
    /// `INTERSECT` and `EXCEPT`.
    IntersectExcept,
    /// `INTERSECT ALL` and `EXCEPT ALL`.
    IntersectExceptAll,
    /// System-versioned tables (`FOR SYSTEM_TIME`).
    SystemVersioning,
    /// `DELETE ... RETURNING`.
    DeleteReturning,
    /// `INSERT ... RETURNING`.
    InsertReturning,
    /// `OFFSET ... FETCH`.
    OffsetFetch,
}

/// Construct refused in a dialect or a version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionError {
    /// The dialect has never had the construct.
    Absent {
        /// Construct.
        construct: Construct,
        /// Dialect.
        dialect: Dialect,
    },
    /// The construct appeared after the chosen version.
    Before {
        /// Construct.
        construct: Construct,
        /// Dialect.
        dialect: Dialect,
        /// Version that brought it.
        since: Version,
        /// Version chosen.
        chosen: Version,
    },
    /// The construct was removed in or before the chosen version.
    Removed {
        /// Construct.
        construct: Construct,
        /// Dialect.
        dialect: Dialect,
        /// Version that removed it.
        until: Version,
        /// Version chosen.
        chosen: Version,
    },
}

/// Registry of a dialect: its floor and its constructs, read from its
/// reference file.
#[derive(Debug)]
pub struct Registry {
    /// Dialect.
    dialect: Dialect,
    /// Oldest version covered.
    floor: Version,
    /// Constructs, with their period of validity.
    constructs: &'static [ConstructEntry],
}

/// Construct of a reference file, with its period of validity.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ConstructEntry {
    /// Construct.
    id: Construct,
    /// First version that has it.
    #[serde(deserialize_with = "deserialize_version")]
    since: Version,
    /// First version that no longer has it.
    #[serde(default, deserialize_with = "deserialize_optional_version")]
    until: Option<Version>,
    /// Reference manual or release notes that attest it.
    #[allow(dead_code, reason = "read by the people who maintain the file")]
    source: String,
}

impl Version {
    /// Version `major.minor.patch`.
    pub const fn new(major: u16, minor: u16, patch: u16) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    /// Reads the numbers at the start of a version: `8.0.19`, `10.6`,
    /// `10.6.28-MariaDB-ubu2204` (as `SELECT VERSION()` gives it). Missing
    /// numbers count as 0; `None` without a first number.
    pub fn parse(text: &str) -> Option<Self> {
        let mut numbers = text
            .split(|character: char| !character.is_ascii_digit())
            .take(3)
            .take_while(|part| !part.is_empty())
            .map(|part| part.parse::<u16>().ok());
        let major = numbers.next()??;
        let minor = numbers.next().flatten().unwrap_or(0);
        let patch = numbers.next().flatten().unwrap_or(0);
        Some(Self::new(major, minor, patch))
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

impl Construct {
    /// Name of the construct in the messages.
    pub const fn label(self) -> &'static str {
        match self {
            Self::With => "`WITH`",
            Self::WithRecursive => "`WITH RECURSIVE`",
            Self::WindowFunctions => "window functions",
            Self::JsonTable => "`JSON_TABLE`",
            Self::JsonArrow => "`->` operator",
            Self::JsonDoubleArrow => "`->>` operator",
            Self::Lateral => "`LATERAL`",
            Self::DerivedColumnList => "column list of a derived table",
            Self::DerivedOuterReference => "outer reference in a derived table",
            Self::NestedSetOperation => "set operation between parentheses as a right operand",
            Self::RollupOrderBy => "`ORDER BY` with `WITH ROLLUP`",
            Self::ExplainFormatTree => "`EXPLAIN FORMAT=TREE`",
            Self::MemberOf => "`MEMBER OF`",
            Self::ExplainAnalyze => "`EXPLAIN ANALYZE`",
            Self::ValuesRow => "`VALUES ROW` statement",
            Self::ValuesBare => "`VALUES` without `ROW`",
            Self::TableStatement => "`TABLE` statement",
            Self::InsertRowAlias => "row alias of `INSERT`",
            Self::IntersectExcept => "`INTERSECT` and `EXCEPT`",
            Self::IntersectExceptAll => "`INTERSECT ALL` and `EXCEPT ALL`",
            Self::SystemVersioning => "`FOR SYSTEM_TIME`",
            Self::DeleteReturning => "`DELETE ... RETURNING`",
            Self::InsertReturning => "`INSERT ... RETURNING`",
            Self::OffsetFetch => "`OFFSET ... FETCH`",
        }
    }
}

impl fmt::Display for VersionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Absent { construct, dialect } => {
                write!(f, "{}: absent from {}", construct.label(), dialect.label())
            }
            Self::Before {
                construct,
                dialect,
                since,
                chosen,
            } => write!(
                f,
                "{}: from {} {since}, chosen version {chosen}",
                construct.label(),
                dialect.label()
            ),
            Self::Removed {
                construct,
                dialect,
                until,
                chosen,
            } => write!(
                f,
                "{}: removed in {} {until}, chosen version {chosen}",
                construct.label(),
                dialect.label()
            ),
        }
    }
}

impl std::error::Error for VersionError {}

impl Registry {
    /// Oldest version covered.
    pub fn floor(&self) -> Version {
        self.floor
    }

    /// Checks that `construct` exists in the `version` of `dialect`.
    ///
    /// # Errors
    ///
    /// `VersionError` when the dialect never had the construct, or not in
    /// this version.
    pub fn check(
        &self,
        dialect: Dialect,
        construct: Construct,
        version: Version,
    ) -> Result<(), VersionError> {
        let Some(entry) = self.constructs.iter().find(|entry| entry.id == construct) else {
            return Err(VersionError::Absent { construct, dialect });
        };
        if version < entry.since {
            return Err(VersionError::Before {
                construct,
                dialect,
                since: entry.since,
                chosen: version,
            });
        }
        match entry.until {
            Some(until) if version >= until => Err(VersionError::Removed {
                construct,
                dialect,
                until,
                chosen: version,
            }),
            _ => Ok(()),
        }
    }

    /// Versions offered to the user: the floor, then each version that adds
    /// or removes a construct or a built-in function, in order.
    pub fn offered_versions(&self) -> Vec<Version> {
        let changes = self
            .constructs
            .iter()
            .flat_map(|entry| std::iter::once(entry.since).chain(entry.until))
            .chain(builtins::changes(self.dialect));
        let mut versions: Vec<Version> = std::iter::once(self.floor)
            .chain(changes.filter(|&version| version > self.floor))
            .collect();
        versions.sort_unstable();
        versions.dedup();
        versions
    }
}

/// Registry of a dialect, built only once.
pub fn registry(dialect: Dialect) -> &'static Registry {
    static MYSQL: OnceLock<Registry> = OnceLock::new();
    static MARIADB: OnceLock<Registry> = OnceLock::new();
    let build = || {
        let reference = reference(dialect);
        Registry {
            dialect,
            floor: reference.floor,
            constructs: &reference.constructs,
        }
    };
    match dialect {
        Dialect::MySql => MYSQL.get_or_init(build),
        Dialect::MariaDb => MARIADB.get_or_init(build),
    }
}

/// Reads a version written as a string in a reference file.
pub(crate) fn deserialize_version<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Version, D::Error> {
    let text = String::deserialize(deserializer)?;
    Version::parse(&text)
        .ok_or_else(|| serde::de::Error::custom(format!("invalid version {text:?}")))
}

/// Reads an optional version of a reference file.
pub(crate) fn deserialize_optional_version<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Version>, D::Error> {
    deserialize_version(deserializer).map(Some)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn version_reads_the_numbers_of_the_server() {
        assert_eq!(Version::parse("8.0.19"), Some(Version::new(8, 0, 19)));
        assert_eq!(Version::parse("10.6"), Some(Version::new(10, 6, 0)));
        assert_eq!(
            Version::parse("10.6.28-MariaDB-ubu2204"),
            Some(Version::new(10, 6, 28))
        );
        assert_eq!(Version::parse("MariaDB"), None);
        assert!(Version::new(8, 0, 19) < Version::new(8, 0, 31));
        assert!(Version::new(8, 4, 0) > Version::new(8, 0, 31));
    }

    #[test]
    fn registries_are_readable_without_duplicates() {
        for dialect in [Dialect::MySql, Dialect::MariaDb] {
            let registry = registry(dialect);
            let mut seen = HashSet::new();
            for entry in registry.constructs {
                assert!(seen.insert(entry.id), "{:?} twice in {dialect:?}", entry.id);
                assert!(entry.until.is_none_or(|until| until > entry.since));
                assert!(entry.source.starts_with("https://"));
            }
        }
    }

    #[test]
    fn values_row_comes_with_mysql_8_0_19() {
        let mysql = registry(Dialect::MySql);
        assert_eq!(
            mysql.check(Dialect::MySql, Construct::ValuesRow, Version::new(8, 0, 19)),
            Ok(())
        );
        let refused = mysql.check(Dialect::MySql, Construct::ValuesRow, Version::new(8, 0, 18));
        assert_eq!(
            refused.unwrap_err().to_string(),
            "`VALUES ROW` statement: from MySQL 8.0.19, chosen version 8.0.18"
        );
    }

    #[test]
    fn each_dialect_stays_in_its_domain() {
        // The example of the dialects: `VALUES ROW` is MySQL, `VALUES` alone is MariaDB.
        let mariadb = registry(Dialect::MariaDb);
        let latest = Version::new(11, 8, 0);
        assert_eq!(
            mariadb
                .check(Dialect::MariaDb, Construct::ValuesRow, latest)
                .unwrap_err()
                .to_string(),
            "`VALUES ROW` statement: absent from MariaDB"
        );
        assert_eq!(
            mariadb.check(Dialect::MariaDb, Construct::ValuesBare, latest),
            Ok(())
        );
        assert!(
            registry(Dialect::MySql)
                .check(Dialect::MySql, Construct::ValuesBare, latest)
                .is_err()
        );
    }

    #[test]
    fn offered_versions_start_at_the_floor_and_follow_the_changes() {
        let mysql = registry(Dialect::MySql).offered_versions();
        assert_eq!(mysql.first(), Some(&Version::new(8, 0, 11)));
        assert!(mysql.contains(&Version::new(8, 0, 19)));
        assert!(mysql.contains(&Version::new(8, 0, 31)));
        // Built-in functions: `FORMAT_PICO_TIME` appears, `WAIT_UNTIL_SQL_THREAD_AFTER_GTIDS` goes.
        assert!(mysql.contains(&Version::new(8, 0, 16)));
        assert!(mysql.contains(&Version::new(8, 2, 0)));
        // Constructs older than the floor give no version of their own.
        assert!(!mysql.contains(&Version::new(8, 0, 1)));
        assert!(mysql.windows(2).all(|pair| pair[0] < pair[1]));
        let mariadb = registry(Dialect::MariaDb).offered_versions();
        assert_eq!(mariadb.first(), Some(&Version::new(10, 6, 3)));
        assert!(mariadb.contains(&Version::new(10, 7, 0)));
    }
}
