//! Check of the built-in functions of a reference file
//! (`crates/dialects/reference/<dialect>.yaml`) against the servers of its
//! dialect: each function listed must be a built-in function of the servers
//! whose version lies in its period, its name must call a stored function
//! between backticks (`grammar`) or followed by a space (`touching`) as the
//! file says, and the names the servers know as built-in functions must be
//! listed. This is how the file is brought up to a new version.
//!
//! The candidates are the names of the file and the names the server knows:
//! its help topics, its keywords, and the `SQL_FUNCTIONS` of MariaDB. Each
//! candidate is prepared, never executed, as `SELECT 0 + NAME(...)` with 0 to
//! 4 arguments, as `SELECT 0 + NAME (1)` with a space and as
//! ``SELECT 0 + `NAME`(...)``: inside an expression, so that the modifiers of
//! `SELECT` (`DISTINCT (1)`) are no calls. Error 1305 means no function of
//! that name; 1582, a built-in function with another number of arguments;
//! 1064, a syntax error (a function with a syntax of its own, or a keyword);
//! 1630, a call to a stored function named like a built-in one.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

use indoc::indoc;
use prisme_dialects::versions::Version;
use prisme_dialects::{Dialect, ListedFunction, listed_functions};

use crate::server::Server;

/// No function of that name.
const NO_FUNCTION: u32 = 1305;
/// Syntax error.
const SYNTAX: u32 = 1064;
/// Stored function that does not exist, the name being a built-in one.
const STORED: u32 = 1630;
/// Built-in function called with another number of arguments.
const WRONG_COUNT: u32 = 1582;

/// Arguments of the calls prepared.
const ARGUMENTS: [&str; 5] = ["", "1", "1, 1", "1, 1, 1", "1, 1, 1, 1"];

/// Procedure that prepares a statement and records the error code.
const PROCEDURE: &str = indoc! {r#"
    DROP TABLE IF EXISTS probe_builtins;
    CREATE TABLE probe_builtins (name VARCHAR(64) NOT NULL, form VARCHAR(10) NOT NULL, code INT NOT NULL);
    DROP PROCEDURE IF EXISTS probe_builtin;
    DELIMITER //
    CREATE PROCEDURE probe_builtin(IN function_name VARCHAR(64), IN form VARCHAR(10), IN statement TEXT)
    BEGIN
      DECLARE code INT DEFAULT 0;
      DECLARE CONTINUE HANDLER FOR SQLEXCEPTION
        BEGIN
          GET DIAGNOSTICS CONDITION 1 code = MYSQL_ERRNO;
        END;
      SET @probe_sql = statement;
      PREPARE probe_statement FROM @probe_sql;
      IF code = 0 THEN
        DEALLOCATE PREPARE probe_statement;
      END IF;
      INSERT INTO probe_builtins VALUES (function_name, form, code);
    END//
    DELIMITER ;
"#};

/// What a server makes of a name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Verdict {
    /// No function of that name.
    Absent,
    /// A built-in function.
    Builtin {
        /// Its name between backticks calls a stored function.
        grammar: bool,
        /// Followed by a space, its name calls a stored function.
        touching: bool,
    },
    /// Every call is a syntax error: a function with a syntax of its own,
    /// or a keyword.
    Syntax,
}

/// Error codes of the calls of a name.
#[derive(Debug, Default)]
pub struct Codes {
    /// `NAME(...)`, by number of arguments.
    plain: Vec<u32>,
    /// `NAME (1)`.
    spaced: Option<u32>,
    /// `` `NAME`(...) ``, by number of arguments.
    quoted: Vec<u32>,
}

impl Codes {
    /// Verdict of the codes: a built-in function when one call reaches it,
    /// whatever the others (`POINT` of MariaDB is a built-in function with
    /// two arguments, a stored one otherwise).
    fn verdict(&self) -> Verdict {
        let reached = |code: &u32| ![NO_FUNCTION, STORED, SYNTAX].contains(code);
        if !self.plain.iter().any(reached) {
            return if self.plain.iter().all(|&code| code == SYNTAX) {
                Verdict::Syntax
            } else {
                Verdict::Absent
            };
        }
        Verdict::Builtin {
            grammar: !self.quoted.is_empty()
                && self
                    .quoted
                    .iter()
                    .all(|&code| code == NO_FUNCTION || code == STORED),
            touching: self.spaced == Some(STORED),
        }
    }

    /// Numbers of arguments of the calls that reach a built-in function.
    pub fn arities(&self) -> BTreeSet<usize> {
        let missed = [NO_FUNCTION, STORED, SYNTAX, WRONG_COUNT];
        self.plain
            .iter()
            .enumerate()
            .filter(|(_, code)| !missed.contains(code))
            .map(|(count, _)| count)
            .collect()
    }
}

/// Checks the functions of the reference file of `dialect` against
/// `servers`, all of that dialect; gives the report.
pub fn check(dialect: Dialect, servers: &[Server]) -> anyhow::Result<String> {
    let listed: BTreeMap<&str, ListedFunction> = listed_functions(dialect)
        .map(|function| (function.name, function))
        .collect();
    let mut out = String::new();
    for server in servers {
        let version = server.parsed_version()?;
        let mut candidates: BTreeSet<String> = known_names(server)?;
        candidates.extend(listed.keys().map(|name| name.to_string()));
        let codes = probe(server, &candidates)?;
        let verdicts: BTreeMap<&str, Verdict> = candidates
            .iter()
            .map(|name| {
                let verdict = codes.get(name).map_or(Verdict::Absent, Codes::verdict);
                (name.as_str(), verdict)
            })
            .collect();
        report(&mut out, server, version, &listed, &verdicts);
    }
    Ok(out)
}

/// Names the server knows: help topics, keywords, functions of the SQL
/// standard (MariaDB), in capitals, those that may name a function.
fn known_names(server: &Server) -> anyhow::Result<BTreeSet<String>> {
    let mut script =
        "SELECT name FROM mysql.help_topic;\nSELECT WORD FROM information_schema.KEYWORDS;\n"
            .to_string();
    if server.dialect == Dialect::MariaDb {
        script.push_str("SELECT FUNCTION FROM information_schema.SQL_FUNCTIONS;\n");
    }
    let output = server.run(&script, &["--batch", "--skip-column-names", "--force"])?;
    let text = String::from_utf8_lossy(&output.stdout);
    Ok(text
        .lines()
        .map(|line| line.trim().trim_end_matches("()").to_ascii_uppercase())
        .filter(|name| is_name(name))
        .collect())
}

/// Tells whether `name` may name a function: a letter, then letters, digits
/// and `_`, in capitals.
fn is_name(name: &str) -> bool {
    let mut characters = name.chars();
    characters
        .next()
        .is_some_and(|first| first.is_ascii_uppercase())
        && characters.all(|character| {
            character.is_ascii_uppercase() || character.is_ascii_digit() || character == '_'
        })
}

/// Error codes of the calls of `names` on `server`.
pub fn probe(server: &Server, names: &BTreeSet<String>) -> anyhow::Result<BTreeMap<String, Codes>> {
    let mut script = PROCEDURE.to_string();
    for name in names {
        for (count, arguments) in ARGUMENTS.iter().enumerate() {
            let _ = writeln!(
                script,
                "CALL probe_builtin('{name}', 'plain{count}', 'SELECT 0 + {name}({arguments})');"
            );
            let _ = writeln!(
                script,
                "CALL probe_builtin('{name}', 'quoted{count}', 'SELECT 0 + `{name}`({arguments})');"
            );
        }
        let _ = writeln!(
            script,
            "CALL probe_builtin('{name}', 'spaced', 'SELECT 0 + {name} (1)');"
        );
    }
    script.push_str("SELECT name, form, code FROM probe_builtins;\n");
    let output = server.run_checked(&script, &["--batch", "--skip-column-names"])?;
    let mut codes: BTreeMap<String, Codes> = BTreeMap::new();
    for line in output.lines() {
        let [name, form, code] = line.split('\t').collect::<Vec<_>>()[..] else {
            continue;
        };
        let Ok(code) = code.parse::<u32>() else {
            continue;
        };
        let entry = codes.entry(name.to_string()).or_default();
        if form == "spaced" {
            entry.spaced = Some(code);
        } else if form.starts_with("quoted") {
            entry.quoted.push(code);
        } else {
            entry.plain.push(code);
        }
    }
    Ok(codes)
}

/// Writes the differences between the file and `server`.
fn report(
    out: &mut String,
    server: &Server,
    version: Version,
    listed: &BTreeMap<&str, ListedFunction>,
    verdicts: &BTreeMap<&str, Verdict>,
) {
    let mut lacking = Vec::new();
    let mut outside = Vec::new();
    let mut forms = Vec::new();
    let mut unverified = Vec::new();
    let mut unlisted = Vec::new();
    let mut confirmed = 0;
    for (&name, &verdict) in verdicts {
        let Some(function) = listed.get(name) else {
            if matches!(verdict, Verdict::Builtin { .. }) {
                unlisted.push(name);
            }
            continue;
        };
        let available = function.available(version);
        match verdict {
            Verdict::Syntax => unverified.push(name),
            Verdict::Absent if available => lacking.push(name),
            Verdict::Absent => confirmed += 1,
            Verdict::Builtin { .. } if !available => outside.push(period(name, function)),
            Verdict::Builtin { grammar, touching } => {
                if grammar != function.grammar {
                    forms.push(format!("{name}: grammar {grammar} on the server"));
                }
                if touching != function.touching {
                    forms.push(format!("{name}: touching {touching} on the server"));
                }
                confirmed += 1;
            }
        }
    }
    let _ = writeln!(
        out,
        "== {} ({}): {} functions listed, {confirmed} confirmed",
        server.label(),
        server.container,
        listed.len()
    );
    section(
        out,
        "listed for this version, absent from the server",
        &lacking,
    );
    section(out, "outside their period, present on the server", &outside);
    section(out, "names that call a stored function otherwise", &forms);
    section(out, "unverified, a syntax of their own", &unverified);
    section(
        out,
        "built-in functions of the server absent from the file (left out on purpose, or to add)",
        &unlisted,
    );
}

/// `name` with its period.
fn period(name: &str, function: &ListedFunction) -> String {
    let since = function.since.map(|since| format!(" since {since}"));
    let until = function.until.map(|until| format!(" until {until}"));
    format!(
        "{name}{}{}",
        since.unwrap_or_default(),
        until.unwrap_or_default()
    )
}

/// Writes a titled list, unless empty.
fn section<T: AsRef<str>>(out: &mut String, title: &str, items: &[T]) {
    if items.is_empty() {
        return;
    }
    let items: Vec<&str> = items.iter().map(AsRef::as_ref).collect();
    let _ = writeln!(out, "  {title} ({}): {}", items.len(), items.join(", "));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_codes_give_the_verdict() {
        let codes = |plain: &[u32], spaced, quoted: &[u32]| Codes {
            plain: plain.to_vec(),
            spaced,
            quoted: quoted.to_vec(),
        };
        let absent = codes(&[1305; 5], Some(1305), &[1305; 5]);
        assert_eq!(absent.verdict(), Verdict::Absent);
        let abs = codes(
            &[1582, 0, 1582, 1582, 1582],
            Some(0),
            &[1582, 0, 1582, 1582, 1582],
        );
        assert_eq!(
            abs.verdict(),
            Verdict::Builtin {
                grammar: false,
                touching: false
            }
        );
        let left = codes(&[1064, 1064, 0, 1064, 1064], Some(1630), &[1630; 5]);
        assert_eq!(
            left.verdict(),
            Verdict::Builtin {
                grammar: true,
                touching: true
            }
        );
        let point = codes(
            &[1630, 1630, 0, 1630, 1630],
            Some(0),
            &[1582, 1582, 0, 1582, 1582],
        );
        assert_eq!(
            point.verdict(),
            Verdict::Builtin {
                grammar: false,
                touching: false
            }
        );
        let keyword = codes(&[1064; 5], Some(1064), &[1305; 5]);
        assert_eq!(keyword.verdict(), Verdict::Syntax);
    }

    #[test]
    fn the_codes_give_the_numbers_of_arguments() {
        let codes = Codes {
            plain: vec![1582, 0, 1582, 1582, 1582],
            ..Codes::default()
        };
        assert_eq!(codes.arities(), BTreeSet::from([1]));
        let concat = Codes {
            plain: vec![1582, 0, 0, 0, 0],
            ..Codes::default()
        };
        assert_eq!(concat.arities(), BTreeSet::from([1, 2, 3, 4]));
        let absent = Codes {
            plain: vec![1305; 5],
            ..Codes::default()
        };
        assert!(absent.arities().is_empty());
    }

    #[test]
    fn only_some_names_may_name_a_function() {
        assert!(is_name("FORMAT_PICO_TIME"));
        assert!(is_name("ST_X"));
        assert!(!is_name("CREATE TABLE"));
        assert!(!is_name("1X"));
        assert!(!is_name("A'B"));
        assert!(!is_name(""));
    }
}
