//! Cases of `expr` generated from a reference file
//! (`crates/dialects/reference/<dialect>.yaml`): each built-in function
//! listed, called with each number of arguments the servers of its dialect
//! accept, from none to `MAX_ARGUMENTS` (the smallest one when all are
//! beyond, `INSERT(s, position, length, t)`), its arguments `1`, then with
//! each value of `VALUES` at each argument in turn, the others staying `1`. The
//! coverage of the measures no longer depends on lists written by hand: the
//! edges of the numbers, strings, dates and JSON, and the columns of each
//! type of `sample`, reach every function. A case reads `ABS(1)`
//! (`abs.1`), then `ABS(NULL)` (`abs.1.1.null`); the cases of a function
//! with a syntax of its own (`CAST(x AS t)`), which no call reaches, are
//! left out, as are the functions that wait or hold locks (`LEFT_OUT`).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

use prisme_dialects::{Dialect, listed_functions};

use crate::functions;
use crate::server::Server;

/// Most arguments of a call generated.
const MAX_ARGUMENTS: usize = 3;

/// Value of the arguments that do not vary.
const NEUTRAL: &str = "1";

/// Values each argument takes in turn, with their identifiers: the edges of
/// the numbers, strings, dates and JSON as literals, then the columns of
/// `sample`, one per type.
const VALUES: &[(&str, &str)] = &[
    ("null", "NULL"),
    ("zero", "0"),
    ("negative", "-1"),
    ("fraction", "1.5"),
    ("double", "1e1"),
    ("unsigned_max", "18446744073709551615"),
    ("beyond", "18446744073709551616"),
    ("signed_min", "-9223372036854775808"),
    ("empty", "''"),
    ("space", "' '"),
    ("accent", "'é'"),
    ("text", "'abc'"),
    ("number_text", "'12abc'"),
    ("date_text", "'2024-02-29'"),
    ("datetime_text", "'2024-02-29 10:20:30.123456'"),
    ("time_text", "'10:20:30'"),
    ("hexadecimal", "0x41"),
    ("json_text", r#"'{"a": [1, 2]}'"#),
    ("d", "d"),
    ("dt", "dt"),
    ("ts", "ts"),
    ("t", "t"),
    ("s", "s"),
    ("i", "i"),
    ("n", "n"),
    ("f", "f"),
    ("dn", "dn"),
    ("sn", "sn"),
    ("j", "j"),
];

/// Functions left out: they wait (`SLEEP(18446744073709551615)`), loop
/// (`BENCHMARK`), read the files of the server, or hold locks from one case
/// to the next.
const LEFT_OUT: &[&str] = &[
    "BENCHMARK",
    "GET_LOCK",
    "IS_FREE_LOCK",
    "IS_USED_LOCK",
    "LOAD_FILE",
    "MASTER_GTID_WAIT",
    "MASTER_POS_WAIT",
    "RELEASE_ALL_LOCKS",
    "RELEASE_LOCK",
    "SLEEP",
    "SOURCE_POS_WAIT",
    "WAIT_FOR_EXECUTED_GTID_SET",
    "WAIT_UNTIL_SQL_THREAD_AFTER_GTIDS",
];

/// Functions whose value changes from one run to the next, whatever their
/// arguments: only their type is compared.
const VOLATILE: &[&str] = &[
    "CONNECTION_ID",
    "CURDATE",
    "CURRENT_DATE",
    "CURRENT_TIME",
    "CURRENT_TIMESTAMP",
    "CURTIME",
    "FOUND_ROWS",
    "LOCALTIME",
    "LOCALTIMESTAMP",
    "NOW",
    "PS_CURRENT_THREAD_ID",
    "PS_THREAD_ID",
    "RANDOM_BYTES",
    "SYSDATE",
    "SYS_GUID",
    "UTC_DATE",
    "UTC_TIME",
    "UTC_TIMESTAMP",
    "UUID",
    "UUID_SHORT",
    "UUID_V4",
    "UUID_V7",
];

/// Functions whose value changes from one run to the next when called
/// without arguments only (`RAND(1)` is the same at each run).
const VOLATILE_WITHOUT_ARGUMENTS: &[&str] = &["RAND", "UNIX_TIMESTAMP"];

/// Numbers of arguments that reach each function of the reference file of
/// `dialect` on `server`, of that dialect.
pub fn arities(
    dialect: Dialect,
    server: &Server,
) -> anyhow::Result<BTreeMap<String, BTreeSet<usize>>> {
    let names: BTreeSet<String> = listed_functions(dialect)
        .map(|function| function.name.to_string())
        .collect();
    let codes = functions::probe(server, &names)?;
    Ok(codes
        .into_iter()
        .map(|(name, codes)| (name, codes.arities()))
        .collect())
}

/// File of cases of the functions of the reference file of `dialect`, with
/// the numbers of arguments `arities` measured on `servers`.
pub fn cases(
    dialect: Dialect,
    servers: &[String],
    arities: &BTreeMap<String, BTreeSet<usize>>,
) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "# Calls of the built-in functions of crates/dialects/reference/{}.yaml,\n\
         # written by `probe calls`, the numbers of arguments measured on {}.",
        dialect_file(dialect),
        servers.join(", ")
    );
    let mut seen = BTreeSet::new();
    let mut unreached = Vec::new();
    for function in listed_functions(dialect) {
        let name = function.name;
        if !seen.insert(name) || LEFT_OUT.contains(&name) {
            continue;
        }
        let counts = arities.get(name).map(counts_called).unwrap_or_default();
        if counts.is_empty() {
            unreached.push(name);
            continue;
        }
        let id = name.to_ascii_lowercase();
        for count in counts {
            let mark = if volatile(name, count) { "~" } else { "" };
            let mut arguments = vec![NEUTRAL; count];
            let _ = writeln!(out, "{mark}{id}.{count} {name}({})", arguments.join(", "));
            for position in 0..count {
                for (value_id, value) in VALUES {
                    arguments[position] = value;
                    let _ = writeln!(
                        out,
                        "{mark}{id}.{count}.{}.{value_id} {name}({})",
                        position + 1,
                        arguments.join(", ")
                    );
                }
                arguments[position] = NEUTRAL;
            }
        }
    }
    if !unreached.is_empty() {
        let _ = writeln!(
            out,
            "# Left out, reached by no call (a syntax of their own): {}",
            unreached.join(", ")
        );
    }
    out
}

/// Numbers of arguments called among those that reach a function: up to
/// `MAX_ARGUMENTS`, or the smallest one when all are beyond.
fn counts_called(counts: &BTreeSet<usize>) -> Vec<usize> {
    let called: Vec<usize> = counts
        .iter()
        .copied()
        .filter(|&count| count <= MAX_ARGUMENTS)
        .collect();
    if called.is_empty() {
        counts.first().copied().into_iter().collect()
    } else {
        called
    }
}

/// Name of the reference file of `dialect`, without its extension.
fn dialect_file(dialect: Dialect) -> &'static str {
    match dialect {
        Dialect::MySql => "mysql",
        Dialect::MariaDb => "mariadb",
    }
}

/// Tells whether the value of `name` with `count` arguments changes from
/// one run to the next.
fn volatile(name: &str, count: usize) -> bool {
    VOLATILE.contains(&name) || (count == 0 && VOLATILE_WITHOUT_ARGUMENTS.contains(&name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr::Cases;

    #[test]
    fn each_argument_takes_each_value_in_turn() {
        let arities = BTreeMap::from([
            ("ABS".to_string(), BTreeSet::from([1])),
            ("NOW".to_string(), BTreeSet::from([0, 1])),
            ("CONCAT".to_string(), BTreeSet::from([1, 2, 3, 4])),
            ("INSERT".to_string(), BTreeSet::from([4])),
        ]);
        let text = cases(Dialect::MySql, &["MySQL 8.4.6".to_string()], &arities);
        let cases = Cases::parse(&text).unwrap();
        let expression = |id: &str| {
            cases
                .cases
                .iter()
                .find(|case| case.id == id)
                .map(|case| (case.expression.as_str(), case.deterministic))
        };
        assert_eq!(expression("abs.1"), Some(("ABS(1)", true)));
        assert_eq!(expression("abs.1.1.null"), Some(("ABS(NULL)", true)));
        assert_eq!(
            expression("concat.3.2.empty"),
            Some(("CONCAT(1, '', 1)", true))
        );
        assert_eq!(expression("concat.4"), None);
        assert_eq!(
            expression("insert.4.3.null"),
            Some(("INSERT(1, 1, NULL, 1)", true))
        );
        assert_eq!(expression("now.0"), Some(("NOW()", false)));
        assert_eq!(expression("now.1.1.zero"), Some(("NOW(0)", false)));
        let per_value = VALUES.len();
        let abs = cases
            .cases
            .iter()
            .filter(|case| case.id.starts_with("abs."))
            .count();
        assert_eq!(abs, 1 + per_value);
        assert!(text.contains("# Left out, reached by no call"));
        assert!(text.contains("CAST"));
        assert!(!text.contains("SLEEP("));
    }

    #[test]
    fn the_values_without_arguments_change_for_some_functions_only() {
        assert!(volatile("RAND", 0));
        assert!(!volatile("RAND", 1));
        assert!(volatile("NOW", 1));
        assert!(!volatile("ABS", 0));
    }
}
