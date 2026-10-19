//! Built-in functions of each dialect, read from the `functions` section of
//! its reference file (`reference` module): their family, their period, the
//! forms of their name that call a stored function instead (between
//! backticks, followed by a space), the type of their result
//! (`function_types` module) and their translation to the other dialects
//! (`docs/DEV-DIALECTS.md`, step 7).
//!
//! A name the dialect does not know, or does not know in the chosen version,
//! calls a stored function of the database. The emitter writes a built-in
//! function from the verdict of the file (`emit::builtin` module): the same
//! function of the output, the same function but for the arguments of some
//! types, which the dialects read otherwise, another name, a rule of Prisme,
//! or an explicit error.

use std::collections::HashMap;
use std::sync::OnceLock;

use prisme_plan::Expr;
use prisme_types::{DataType, Type};
use serde::Deserialize;

use crate::Dialect;
use crate::function_types::{Nulls, ResultType};
use crate::reference::reference;
use crate::versions::{self, Version};

/// Family of functions of a reference file, with the page of the manual that
/// describes it.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Family {
    /// Name of the family.
    #[allow(dead_code, reason = "read by the people who maintain the file")]
    family: String,
    /// Page of the manual.
    #[allow(dead_code, reason = "read by the people who maintain the file")]
    manual: String,
    /// Role of the functions of the family.
    #[serde(default)]
    kind: Kind,
    /// Servers the translations of the family were measured on.
    #[serde(default)]
    #[allow(dead_code, reason = "read by the people who maintain the file")]
    checked: Vec<String>,
    /// Functions.
    functions: Vec<Function>,
}

/// Role of a function in a query.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Kind {
    /// One value per row.
    #[default]
    Scalar,
    /// One value per group.
    Aggregate,
    /// One value per row, computed over a window.
    Window,
}

/// Built-in function of a reference file.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Function {
    /// Name, in capitals.
    pub(crate) name: String,
    /// First version that has it.
    #[serde(default, deserialize_with = "versions::deserialize_optional_version")]
    pub(crate) since: Option<Version>,
    /// First version that no longer has it.
    #[serde(default, deserialize_with = "versions::deserialize_optional_version")]
    pub(crate) until: Option<Version>,
    /// Document that attests the period.
    #[serde(default)]
    #[allow(dead_code, reason = "read by the people who maintain the file")]
    source: Option<String>,
    /// Function of the grammar of the server: its name between backticks
    /// calls a stored function; otherwise the quoted name is the built-in
    /// function.
    #[serde(default)]
    pub(crate) grammar: bool,
    /// With a space before `(`, the name calls a stored function, unless the
    /// SQL mode `IGNORE_SPACE` is set.
    #[serde(default)]
    pub(crate) touching: bool,
    /// Type of the result, when Prisme knows it.
    #[serde(default, rename = "type")]
    result: Option<ResultType>,
    /// When the result is `NULL`.
    #[serde(default)]
    nulls: Nulls,
    /// Translation to MySQL.
    #[serde(default)]
    mysql: Option<Translation>,
    /// Translation to MariaDB.
    #[serde(default)]
    mariadb: Option<Translation>,
}

/// Translation of a built-in function to another dialect.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(try_from = "RawTranslation")]
pub(crate) enum Translation {
    /// The function of the same name of the output gives the same results.
    Same,
    /// The function of the same name gives the same results, but for an
    /// argument of one of these types, or of a type unknown to Prisme: an
    /// error, for this reason.
    SameExcept {
        /// Types of the arguments the dialects read otherwise.
        types: Vec<ArgumentType>,
        /// Reason of the error.
        reason: String,
    },
    /// Another name of the output, same arguments and results.
    Name(String),
    /// A rule of Prisme writes it.
    Rule(Rule),
    /// Untranslatable, for this reason.
    Error(String),
}

/// Rule of Prisme that writes a built-in function in another dialect, named
/// by its identifier in the reference files (`emit::builtin` module).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub(crate) enum Rule {
    /// `ADD_MONTHS` of MariaDB.
    #[serde(rename = "function.add-months")]
    AddMonths,
    /// `AES_ENCRYPT`, `AES_DECRYPT`: their optional arguments.
    #[serde(rename = "function.aes")]
    Aes,
    /// `REPLACE`, `SUBSTRING_INDEX`, `LOCATE`, `INSTR`, `TRIM` and
    /// `POSITION` with a binary argument (`emit::charset` module).
    #[serde(rename = "function.binary-argument")]
    BinaryArgument,
    /// `BIT_AND`, `BIT_OR`, `BIT_XOR` of a byte string, of a time with a
    /// fraction of a second, of a `JSON` (`emit::bitwise` module).
    #[serde(rename = "function.bit-aggregate")]
    BitAggregate,
    /// `BIT_COUNT` of a binary string.
    #[serde(rename = "function.bit-count")]
    BitCount,
    /// `EXTRACT` of a `TIME`, of the day of a date, of a string or a number
    /// (`emit::extract` module).
    #[serde(rename = "function.extract")]
    Extract,
    /// `FROM_UNIXTIME`: its type and its range.
    #[serde(rename = "function.from-unixtime")]
    FromUnixtime,
    /// `GREATEST`, `LEAST` over numbers and strings.
    #[serde(rename = "function.greatest")]
    Greatest,
    /// `IF` of a `JSON` condition (`emit::truth` module).
    #[serde(rename = "function.if")]
    If,
    /// `NOW`, `CURRENT_TIMESTAMP`, `SYSDATE`, `LOCALTIME`: their type.
    #[serde(rename = "function.now")]
    Now,
    /// `LPAD`, `RPAD` with an empty padding.
    #[serde(rename = "function.pad")]
    Pad,
    /// `REGEXP_INSTR`, `REGEXP_LIKE`, `REGEXP_REPLACE`, `REGEXP_SUBSTR`: the
    /// libraries of MySQL (ICU) and MariaDB (PCRE) read the patterns
    /// differently.
    #[serde(rename = "function.regexp")]
    Regexp,
    /// `ABS`, `CEIL`, `CEILING`, `FLOOR`, `ROUND` and `TRUNCATE` of a date
    /// or a time (`emit::rounding` module).
    #[serde(rename = "function.rounding")]
    Rounding,
    /// `TIME_TO_SEC` of a fractional time.
    #[serde(rename = "function.time-to-sec")]
    TimeToSec,
    /// `UNIX_TIMESTAMP`: its range.
    #[serde(rename = "function.unix-timestamp")]
    UnixTimestamp,
    /// `UUID`: its type.
    #[serde(rename = "function.uuid")]
    Uuid,
}

/// Type of an argument that a function reads otherwise in the other
/// dialect (`Translation::SameExcept`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ArgumentType {
    /// `DATE`, `TIME`, `DATETIME` and `TIMESTAMP`.
    Temporal,
    /// `FLOAT` and `DOUBLE`.
    Float,
    /// `DECIMAL` of more than 15 digits, which a `DOUBLE` holds no longer
    /// exactly.
    WideDecimal,
}

impl ArgumentType {
    /// Tells whether a value of `data_type` is of this type.
    pub(crate) fn matches(self, data_type: &DataType) -> bool {
        match self {
            Self::Temporal => matches!(
                data_type,
                DataType::Date
                    | DataType::Time { .. }
                    | DataType::DateTime { .. }
                    | DataType::Timestamp { .. }
            ),
            Self::Float => matches!(data_type, DataType::Float(_)),
            Self::WideDecimal => matches!(
                data_type,
                DataType::Decimal(decimal) if decimal.precision() > 15
            ),
        }
    }
}

impl Rule {
    /// Identifier, in the reference files and the report.
    pub(crate) fn id(self) -> &'static str {
        match self {
            Self::AddMonths => "function.add-months",
            Self::Aes => "function.aes",
            Self::BinaryArgument => "function.binary-argument",
            Self::BitAggregate => "function.bit-aggregate",
            Self::BitCount => "function.bit-count",
            Self::Extract => "function.extract",
            Self::FromUnixtime => "function.from-unixtime",
            Self::Greatest => "function.greatest",
            Self::If => "function.if",
            Self::Now => "function.now",
            Self::Pad => "function.pad",
            Self::Regexp => "function.regexp",
            Self::Rounding => "function.rounding",
            Self::TimeToSec => "function.time-to-sec",
            Self::UnixTimestamp => "function.unix-timestamp",
            Self::Uuid => "function.uuid",
        }
    }
}

/// Translation as written in a reference file: `same`, or a map of one key.
#[derive(Deserialize)]
#[serde(untagged)]
enum RawTranslation {
    /// `same`.
    Word(String),
    /// `{ name: X }`, `{ rule: id }`, `{ error: reason }` or
    /// `{ same_except: [types], reason: reason }`.
    Map(TranslationMap),
}

/// Map of a translation.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TranslationMap {
    /// Another name.
    name: Option<String>,
    /// Rule of Prisme.
    rule: Option<Rule>,
    /// Reason of an error.
    error: Option<String>,
    /// Types of the arguments refused.
    same_except: Option<Vec<ArgumentType>>,
    /// Reason the arguments of `same_except` are refused.
    reason: Option<String>,
}

impl TryFrom<RawTranslation> for Translation {
    type Error = String;

    fn try_from(raw: RawTranslation) -> Result<Self, String> {
        match raw {
            RawTranslation::Word(word) if word == "same" => Ok(Self::Same),
            RawTranslation::Word(word) => Err(format!("unknown translation {word:?}")),
            RawTranslation::Map(map) => {
                match (map.name, map.rule, map.error, map.same_except, map.reason) {
                    (Some(name), None, None, None, None) => Ok(Self::Name(name)),
                    (None, Some(rule), None, None, None) => Ok(Self::Rule(rule)),
                    (None, None, Some(reason), None, None) => Ok(Self::Error(reason)),
                    (None, None, None, Some(types), Some(reason)) if !types.is_empty() => {
                        Ok(Self::SameExcept { types, reason })
                    }
                    _ => Err(
                        "a translation has one key, name, rule or error, or same_except and reason"
                            .to_string(),
                    ),
                }
            }
        }
    }
}

/// Built-in function found in a dialect, with its role.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Builtin {
    /// Role.
    pub(crate) kind: Kind,
    /// Function.
    pub(crate) function: &'static Function,
}

impl Function {
    /// Tells whether `version` has the function.
    pub(crate) fn available(&self, version: Version) -> bool {
        within(self.since, self.until, version)
    }

    /// Type of the result of a scalar call with `args` in `version`: of a
    /// type unknown to Prisme when the file gives none.
    pub(crate) fn result_type(&self, args: &[Expr], version: Version) -> Type {
        let data_type = (self.result.as_ref())
            .map_or(DataType::Unknown, |result| result.data_type(args, version));
        Type {
            data_type,
            nullable: self.nulls.nullable(args),
        }
    }

    /// Translation to `dialect`, when the file gives one.
    pub(crate) fn translation(&self, dialect: Dialect) -> Option<&Translation> {
        match dialect {
            Dialect::MySql => self.mysql.as_ref(),
            Dialect::MariaDb => self.mariadb.as_ref(),
        }
    }
}

/// Built-in functions of a dialect by name, built only once.
fn index(dialect: Dialect) -> &'static HashMap<&'static str, Builtin> {
    static MYSQL: OnceLock<HashMap<&'static str, Builtin>> = OnceLock::new();
    static MARIADB: OnceLock<HashMap<&'static str, Builtin>> = OnceLock::new();
    let build = || {
        let mut index = HashMap::new();
        for family in families(dialect) {
            for function in &family.functions {
                let builtin = Builtin {
                    kind: family.kind,
                    function,
                };
                index.insert(function.name.as_str(), builtin);
            }
        }
        index
    };
    match dialect {
        Dialect::MySql => MYSQL.get_or_init(build),
        Dialect::MariaDb => MARIADB.get_or_init(build),
    }
}

/// Families of functions of a dialect.
pub(crate) fn families(dialect: Dialect) -> &'static [Family] {
    &reference(dialect).functions
}

/// Versions where a built-in function of `dialect` appears, disappears or
/// changes the type of its result.
pub(crate) fn changes(dialect: Dialect) -> impl Iterator<Item = Version> {
    families(dialect)
        .iter()
        .flat_map(|family| &family.functions)
        .flat_map(|function| {
            let types = function.result.iter().flat_map(ResultType::versions);
            function
                .since
                .into_iter()
                .chain(function.until)
                .chain(types)
        })
}

/// Built-in function of a reference file, as the servers must show it:
/// checked on real servers by `prisme-probe`.
#[derive(Debug, Clone, Copy)]
pub struct ListedFunction {
    /// Name, in capitals.
    pub name: &'static str,
    /// First version that has it.
    pub since: Option<Version>,
    /// First version that no longer has it.
    pub until: Option<Version>,
    /// Its name between backticks calls a stored function.
    pub grammar: bool,
    /// With a space before `(`, its name calls a stored function.
    pub touching: bool,
}

impl ListedFunction {
    /// Tells whether `version` has the function.
    pub fn available(&self, version: Version) -> bool {
        within(self.since, self.until, version)
    }
}

/// Tells whether `version` lies in the period from `since` (included) to
/// `until` (excluded).
fn within(since: Option<Version>, until: Option<Version>, version: Version) -> bool {
    since.is_none_or(|since| version >= since) && until.is_none_or(|until| version < until)
}

/// Built-in functions of the reference file of `dialect`.
pub fn listed_functions(dialect: Dialect) -> impl Iterator<Item = ListedFunction> {
    families(dialect)
        .iter()
        .flat_map(|family| &family.functions)
        .map(|function| ListedFunction {
            name: function.name.as_str(),
            since: function.since,
            until: function.until,
            grammar: function.grammar,
            touching: function.touching,
        })
}

/// Built-in function `name`, in capitals, of `dialect`, whatever its period.
pub(crate) fn lookup(dialect: Dialect, name: &str) -> Option<Builtin> {
    index(dialect).get(name).copied()
}

/// Built-in function `name`, in capitals, of `dialect` in `version`.
pub(crate) fn builtin(dialect: Dialect, version: Version, name: &str) -> Option<Builtin> {
    lookup(dialect, name).filter(|builtin| builtin.function.available(version))
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::sql::ast::AGGREGATES;

    /// Tells whether `name` followed by a space and `(` calls a stored
    /// function in `version` of `dialect`.
    fn touching(dialect: Dialect, version: Version, name: &str) -> bool {
        builtin(dialect, version, name).is_some_and(|builtin| builtin.function.touching)
    }

    /// Each dialect, with the other one.
    const PAIRS: [(Dialect, Dialect); 2] = [
        (Dialect::MySql, Dialect::MariaDb),
        (Dialect::MariaDb, Dialect::MySql),
    ];

    #[test]
    fn each_function_appears_once_with_its_source() {
        for (dialect, other) in PAIRS {
            let mut seen = HashSet::new();
            for family in families(dialect) {
                for function in &family.functions {
                    let name = &function.name;
                    assert!(seen.insert(name.clone()), "{name} twice in {dialect:?}");
                    assert_eq!(*name, name.to_ascii_uppercase(), "{name}");
                    let dated = function.since.is_some() || function.until.is_some();
                    assert_eq!(
                        dated,
                        function.source.is_some(),
                        "{name}: period and source"
                    );
                    if let (Some(since), Some(until)) = (function.since, function.until) {
                        assert!(since < until, "{name}");
                    }
                    assert!(function.grammar || !function.touching, "{name}");
                    // A file translates to the other dialects only.
                    assert!(function.translation(dialect).is_none(), "{name}");
                    let _ = function.translation(other);
                }
            }
        }
    }

    #[test]
    fn the_aggregate_functions_of_the_parser_are_in_the_files() {
        for dialect in [Dialect::MySql, Dialect::MariaDb] {
            for name in AGGREGATES {
                let builtin = lookup(dialect, name).expect(name);
                assert_eq!(builtin.kind, Kind::Aggregate, "{name} in {dialect:?}");
            }
        }
    }

    #[test]
    fn functions_have_their_period() {
        let format = lookup(Dialect::MySql, "FORMAT_PICO_TIME").unwrap();
        assert!(!format.function.available(Version::new(8, 0, 15)));
        assert!(format.function.available(Version::new(8, 0, 16)));
        assert!(builtin(Dialect::MySql, Version::new(8, 0, 11), "FORMAT_PICO_TIME").is_none());
        let removed = lookup(Dialect::MySql, "WAIT_UNTIL_SQL_THREAD_AFTER_GTIDS").unwrap();
        assert!(removed.function.available(Version::new(8, 0, 11)));
        assert!(!removed.function.available(Version::new(8, 4, 0)));
        assert!(lookup(Dialect::MySql, "ADD_MONTHS").is_none());
        assert!(lookup(Dialect::MariaDb, "ADD_MONTHS").is_some());
    }

    #[test]
    fn a_space_turns_some_names_into_stored_functions() {
        let mysql = Version::new(8, 4, 0);
        assert!(touching(Dialect::MySql, mysql, "NOW"));
        assert!(touching(Dialect::MySql, mysql, "SUM"));
        // Measured: `AVG (a)` and `CONCAT (a)` stay built-in functions.
        assert!(!touching(Dialect::MySql, mysql, "AVG"));
        assert!(!touching(Dialect::MySql, mysql, "CONCAT"));
        // `ST_COLLECT` appeared in MySQL 8.0.24.
        assert!(!touching(
            Dialect::MySql,
            Version::new(8, 0, 11),
            "ST_COLLECT"
        ));
        let mariadb = Version::new(11, 8, 0);
        assert!(!touching(Dialect::MariaDb, mariadb, "SYSDATE"));
        assert!(touching(Dialect::MariaDb, mariadb, "RANK"));
    }

    #[test]
    fn translations_read_their_forms() {
        let now = lookup(Dialect::MySql, "NOW").unwrap().function;
        assert_eq!(
            now.translation(Dialect::MariaDb),
            Some(&Translation::Rule(Rule::Now))
        );
        let concat = lookup(Dialect::MySql, "CONCAT").unwrap().function;
        assert_eq!(
            concat.translation(Dialect::MariaDb),
            Some(&Translation::Same)
        );
        let weight = lookup(Dialect::MySql, "WEIGHT_STRING").unwrap().function;
        assert!(matches!(
            weight.translation(Dialect::MariaDb),
            Some(Translation::Error(_))
        ));
        let raw: Result<Translation, _> = yaml_serde::from_str("{ name: X, rule: y }");
        assert!(raw.is_err());
        let raw: Result<Translation, _> = yaml_serde::from_str("other");
        assert!(raw.is_err());
        let raw: Result<Translation, _> = yaml_serde::from_str("{ rule: function.unknown }");
        assert!(raw.is_err());
        let raw: Translation =
            yaml_serde::from_str("{ same_except: [temporal, float], reason: r }").unwrap();
        assert_eq!(
            raw,
            Translation::SameExcept {
                types: vec![ArgumentType::Temporal, ArgumentType::Float],
                reason: "r".to_string()
            }
        );
        for wrong in [
            "{ same_except: [temporal] }",
            "{ same_except: [], reason: r }",
            "{ same_except: [other], reason: r }",
            "{ name: X, reason: r }",
        ] {
            let raw: Result<Translation, _> = yaml_serde::from_str(wrong);
            assert!(raw.is_err(), "{wrong}");
        }
        let rule: Rule = yaml_serde::from_str(Rule::TimeToSec.id()).unwrap();
        assert_eq!(rule, Rule::TimeToSec);
    }

    #[test]
    fn a_function_translated_as_is_exists_in_the_other_dialect() {
        for (dialect, other) in PAIRS {
            for family in families(dialect) {
                for function in &family.functions {
                    let name = match function.translation(other) {
                        Some(Translation::Same | Translation::SameExcept { .. }) => &function.name,
                        Some(Translation::Name(name)) => name,
                        _ => continue,
                    };
                    assert!(lookup(other, name).is_some(), "{name} in {other:?}");
                }
            }
        }
    }
}
