//! Beautifier: rewrites the typed SQL as "beautiful SQL", rule by rule.
//!
//! Rules in place:
//!
//! 1. **Normalised whitespace.** No-break, thin and other exotic whitespace
//!    becomes normal spaces, a tab four spaces, outside strings and
//!    identifiers between backticks (`whitespace` module).
//!
//! 2. **`AS` before the column and table aliases** that lack it (`alias`
//!    module).
//!
//! 3. **Function names in capitals**, except the user's functions and
//!    procedures, written in the case of their definition (`functions`
//!    module).
//!
//! 4. **Keywords in capitals, names as written in the catalog**, from the
//!    analysis of the query (`analysis` module): a word the query uses as a
//!    keyword goes to capitals (`CAST`, `DATE '...'`, `INTERVAL 1 DAY`), a
//!    name (database, table, column) takes the catalog's spelling, an alias
//!    the spelling of its declaration; an unknown or ambiguous name stays as
//!    is, and so does a keyword unknown to the server version.
//!
//!    A query that `sqlparser` does not understand (procedure body, syntax
//!    specific to a version) is processed word by word: only the words of
//!    the `keywords/*.yaml` files known to the version go to capitals, plus
//!    time units (`second`, `day`...) where they can only be a unit, after
//!    `INTERVAL <value>` and as the first argument of `EXTRACT`,
//!    `TIMESTAMPADD` and `TIMESTAMPDIFF`. Table and column names (`name`,
//!    `status`, `date`...) then stay as they are.
//!
//! 5. **Layout**: one clause per line, one column and one predicate per line
//!    when there are several, spaces around operators (`layout` module).
//!
//! A text that contains the `DELIMITER` command (procedures, functions...) is
//! first cut into pieces by the query splitter: each query gets all the
//! rules, with its own separator, and the body of a stored program is laid
//! out block by block (`BEGIN ... END`, `IF ... END IF`...).
//!
//! The text of the tokens is kept identical: comments, strings, identifiers
//! between backticks. The text is tokenised by `sqlparser`; keywords are put
//! in capitals at their original position, then the layout only changes
//! spaces and line breaks.

use sqlparser::tokenizer::{Token, Tokenizer};

use prisme_splitter::{Chunk, chunks};

use crate::Dialect;
use crate::alias::add_alias_keyword;
use crate::analysis::{CatalogNames, Role, analyze};
use crate::functions::case_function_names;
use crate::keywords::is_keyword;
use crate::layout::{has_delimiter_command, layout};
use crate::offsets::OffsetIndex;
use crate::whitespace::normalize_blanks;

/// MySQL time units, in capitals in the positions where they can only be a
/// unit (compound units, `DAY_HOUR`..., are reserved).
const TIME_UNITS: &[&str] = &[
    "MICROSECOND",
    "SECOND",
    "MINUTE",
    "HOUR",
    "DAY",
    "WEEK",
    "MONTH",
    "QUARTER",
    "YEAR",
];

/// Functions whose first argument is a time unit.
const UNIT_FUNCTIONS: &[&str] = &["EXTRACT", "TIMESTAMPADD", "TIMESTAMPDIFF"];

/// Place of a token where a time unit can come: only the token right after
/// the mark is concerned, so a unit further on stays a name.
#[derive(Clone, Copy, PartialEq, Eq)]
enum UnitSlot {
    /// Not a place for a time unit.
    Elsewhere,
    /// First argument of a function of `UNIT_FUNCTIONS`, right after its
    /// parenthesis.
    FirstArgument,
    /// Value of an interval, right after `INTERVAL`: the unit follows it, once
    /// the value is over. `depth` counts the parentheses opened by the value
    /// (`INTERVAL (n + 1) MONTH`).
    IntervalValue { depth: usize },
    /// Unit of an interval, right after its value.
    IntervalUnit,
}

/// Context of beautifying: dialect, version and catalog of the server.
#[derive(Debug, Clone, Copy)]
pub struct BeautifyOptions<'a> {
    /// Dialect of the text.
    pub dialect: Dialect,
    /// Version announced by the server (`10.11.11-MariaDB`); when absent,
    /// all the dialect's keywords are known.
    pub version: Option<&'a str>,
    /// Names of the catalog, as defined: tables, columns, functions...
    pub catalog: &'a CatalogNames,
}

/// Beautifies `sql` without a catalog or version: names stay as they are,
/// all functions go to capitals. See `beautify_with`.
pub fn beautify(sql: &str, dialect: Dialect) -> String {
    let catalog = CatalogNames::default();
    beautify_with(
        sql,
        BeautifyOptions {
            dialect,
            version: None,
            catalog: &catalog,
        },
    )
}

/// Beautifies `sql`: normalised whitespace, `AS` before aliases, function
/// names in capitals, keywords in capitals and names as written in the
/// catalog, then layout (`layout` module).
///
/// The catalog's functions and procedures keep the case of their definition
/// instead of going to capitals.
///
/// A text that the tokenizer does not understand (unclosed string...) is
/// returned as is.
pub fn beautify_with(sql: &str, options: BeautifyOptions) -> String {
    let normalized = normalize_blanks(sql);
    if has_delimiter_command(&normalized) {
        beautify_chunks(&normalized, options)
    } else {
        beautify_part(&normalized, options, false)
    }
}

/// Beautifies a text that contains `DELIMITER` commands, piece by piece:
/// each command on its line, then each query with its separator, an empty
/// line between two queries.
fn beautify_chunks(sql: &str, options: BeautifyOptions) -> String {
    let mut result = String::new();
    // Does the previous piece call for a mere line break (command, comments
    // only) rather than an empty line? A command always sticks to what
    // precedes it.
    let mut tight = true;
    let mut delimiter = ";".to_string();
    for chunk in chunks(sql) {
        let command = matches!(chunk, Chunk::Command { .. });
        let (text, next_tight) = match chunk {
            Chunk::Command {
                delimiter: new_delimiter,
                ..
            } => {
                let text = format!("DELIMITER {new_delimiter}");
                delimiter = new_delimiter;
                (text, true)
            }
            Chunk::Sql {
                text,
                delimiter: end,
            } => {
                if text.trim().is_empty() {
                    continue;
                }
                // Outside the usual ";", the query can be a stored program.
                let program = delimiter != ";";
                let mut part = beautify_part(&text, options, program);
                let only_comments = end.is_empty() && is_comment_only(&part, options.dialect);
                match end.as_str() {
                    "" => {}
                    ";" => part.push(';'),
                    _ => {
                        part.push(' ');
                        part.push_str(&end);
                    }
                }
                (part, only_comments)
            }
        };
        if !result.is_empty() {
            result.push_str(if tight || command { "\n" } else { "\n\n" });
        }
        result.push_str(&text);
        tight = next_tight;
    }
    result
}

/// Beautifies a part of text without a `DELIMITER` command; with `program`,
/// a single query ended by a separator other than `;`, which can be a stored
/// program.
fn beautify_part(sql: &str, options: BeautifyOptions, program: bool) -> String {
    let dialect = options.dialect;
    let aliased = add_alias_keyword(sql, dialect);
    let functions = case_function_names(&aliased, dialect, &options.catalog.routines);
    let cased = match analyze(&functions, dialect, options.version, options.catalog) {
        Some(words) => apply_roles(&functions, &words),
        None => uppercase_keywords(&functions, dialect, options.version),
    };
    layout(&cased, dialect, program)
}

/// Rewrites each word from its role: keyword in capitals, name in its
/// spelling; a keyword unknown to the server version stays as is.
fn apply_roles(sql: &str, words: &[crate::analysis::Word]) -> String {
    let mut result = String::with_capacity(sql.len());
    let mut copied = 0;
    for word in words {
        result.push_str(&sql[copied..word.start]);
        let text = &sql[word.start..word.end];
        match &word.role {
            Role::Keyword => result.push_str(&text.to_ascii_uppercase()),
            Role::UnknownKeyword => result.push_str(text),
            Role::Name { spelling } => result.push_str(spelling),
        }
        copied = word.end;
    }
    result.push_str(&sql[copied..]);
    result
}

/// Is the text made only of comments (and whitespace)?
fn is_comment_only(sql: &str, dialect: Dialect) -> bool {
    let parser_dialect = dialect.parser_dialect();
    Tokenizer::new(parser_dialect.as_ref(), sql)
        .tokenize()
        .is_ok_and(|tokens| {
            tokens
                .iter()
                .all(|token| matches!(token, Token::Whitespace(_)))
        })
}

/// Puts keywords in capitals, without changing anything else, word by word:
/// fallback for a text the analysis does not understand. Only the words of
/// the keyword files known to the version go to capitals.
fn uppercase_keywords(sql: &str, dialect: Dialect, version: Option<&str>) -> String {
    let parser_dialect = dialect.parser_dialect();
    let Ok(tokens) = Tokenizer::new(parser_dialect.as_ref(), sql).tokenize_with_location() else {
        return sql.to_string();
    };

    let index = OffsetIndex::new(sql);
    let mut result = String::with_capacity(sql.len());
    let mut copied = 0;
    // Where the next token must be for a time unit to be expected.
    let mut slot = UnitSlot::Elsewhere;
    // Last word read, to recognise EXTRACT( and the like.
    let mut previous_word = String::new();
    let mut tokens = tokens
        .into_iter()
        .filter(|token| !matches!(token.token, Token::Whitespace(_)))
        .peekable();
    while let Some(token) = tokens.next() {
        // A word followed by a parenthesis is a function call, whose value
        // goes on until its closing parenthesis.
        let call = matches!(tokens.peek().map(|next| &next.token), Some(Token::LParen));
        let current = std::mem::replace(&mut slot, UnitSlot::Elsewhere);
        let word = match &token.token {
            Token::Word(word) if word.quote_style.is_none() => word,
            Token::LParen => {
                slot = match current {
                    UnitSlot::IntervalValue { depth } => {
                        UnitSlot::IntervalValue { depth: depth + 1 }
                    }
                    _ if UNIT_FUNCTIONS.contains(&previous_word.as_str()) => {
                        UnitSlot::FirstArgument
                    }
                    _ => UnitSlot::Elsewhere,
                };
                previous_word.clear();
                continue;
            }
            _ => {
                slot = match (current, &token.token) {
                    (UnitSlot::IntervalValue { depth: 0 }, _) => UnitSlot::IntervalUnit,
                    (UnitSlot::IntervalValue { depth: 1 }, Token::RParen) => UnitSlot::IntervalUnit,
                    (UnitSlot::IntervalValue { depth }, Token::RParen) => {
                        UnitSlot::IntervalValue { depth: depth - 1 }
                    }
                    (UnitSlot::IntervalValue { depth }, _) => UnitSlot::IntervalValue { depth },
                    _ => UnitSlot::Elsewhere,
                };
                previous_word.clear();
                continue;
            }
        };
        let upper = word.value.to_ascii_uppercase();
        let is_unit = matches!(current, UnitSlot::FirstArgument | UnitSlot::IntervalUnit)
            && TIME_UNITS.contains(&upper.as_str());
        slot = match (upper.as_str(), current) {
            ("INTERVAL", _) => UnitSlot::IntervalValue { depth: 0 },
            (_, UnitSlot::IntervalValue { depth: 0 }) if call => {
                UnitSlot::IntervalValue { depth: 0 }
            }
            (_, UnitSlot::IntervalValue { depth: 0 }) => UnitSlot::IntervalUnit,
            (_, UnitSlot::IntervalValue { depth }) => UnitSlot::IntervalValue { depth },
            _ => UnitSlot::Elsewhere,
        };
        previous_word = upper;
        if !is_unit && !is_keyword(&word.value, dialect, version) {
            continue;
        }
        let (Some(start), Some(end)) =
            (index.offset(token.span.start), index.offset(token.span.end))
        else {
            continue;
        };
        result.push_str(&sql[copied..start]);
        result.push_str(&sql[start..end].to_ascii_uppercase());
        copied = end;
    }
    result.push_str(&sql[copied..]);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pretty(sql: &str) -> String {
        uppercase_keywords(sql, Dialect::MariaDb, None)
    }

    #[test]
    fn uppercases_keywords() {
        assert_eq!(
            pretty(
                "select id, name from video where status = 1 order by id desc limit 10 offset 5"
            ),
            "SELECT id, name FROM video WHERE status = 1 ORDER BY id DESC LIMIT 10 OFFSET 5"
        );
    }

    #[test]
    fn keeps_everything_else_untouched() {
        let sql = "select `select`, 'from x' -- where\nfrom t /* join */\n\twhere a is not null;";
        assert_eq!(
            pretty(sql),
            "SELECT `select`, 'from x' -- where\nFROM t /* join */\n\tWHERE a IS NOT NULL;"
        );
    }

    #[test]
    fn handles_case_and_multibyte_text() {
        assert_eq!(
            pretty("select case when x then 'café' else 'tea' end as drink from t"),
            "SELECT CASE WHEN x THEN 'café' ELSE 'tea' END AS drink FROM t"
        );
    }

    #[test]
    fn leaves_functions_and_identifiers_as_written() {
        assert_eq!(
            pretty("select count(*), date, user from log"),
            "SELECT count(*), date, user FROM log"
        );
    }

    #[test]
    fn uppercases_time_units_where_they_can_only_be_units() {
        assert_eq!(
            pretty(
                "select all * from video where created_at >= current_date - interval 120 second order by id desc;"
            ),
            "SELECT ALL * FROM video WHERE created_at >= CURRENT_DATE - INTERVAL 120 SECOND ORDER BY id DESC;"
        );
        assert_eq!(
            pretty(
                "select extract(year from d), timestampdiff(day, a, b), date_add(d, interval (n + 1) month)"
            ),
            "SELECT extract(YEAR FROM d), timestampdiff(DAY, a, b), date_add(d, INTERVAL (n + 1) MONTH)"
        );
        // Elsewhere, these words can be column names.
        assert_eq!(pretty("select day, year from t"), "SELECT day, year FROM t");
        // A unit is expected only right after its place: a later `day` is a name.
        assert_eq!(
            pretty("select extract(isoyear from d), day from t"),
            "SELECT extract(isoyear FROM d), day FROM t"
        );
        assert_eq!(
            pretty("select d + interval (a + b) day, day from t"),
            "SELECT d + INTERVAL (a + b) DAY, day FROM t"
        );
        assert_eq!(
            pretty("select d + interval f(x) day, day from t"),
            "SELECT d + INTERVAL f(x) DAY, day FROM t"
        );
    }

    #[test]
    fn beautifies_with_layout() {
        assert_eq!(
            beautify("select a, b from t where x=1 and y=2", Dialect::MariaDb),
            "SELECT ALL\n    a,\n    b\nFROM t\nWHERE TRUE\n    AND x = 1\n    AND y = 2"
        );
    }

    #[test]
    fn normalizes_exotic_blanks_before_layout() {
        assert_eq!(
            beautify(
                "select\u{00A0}a\tfrom\u{202F}t where s = 'x\u{00A0}y'",
                Dialect::MySql
            ),
            "SELECT ALL a\nFROM t\nWHERE s = 'x\u{00A0}y'"
        );
    }

    #[test]
    fn adds_as_before_aliases() {
        assert_eq!(
            beautify("select count(*) n from video v", Dialect::MySql),
            "SELECT ALL COUNT(*) AS n\nFROM video AS v"
        );
    }

    #[test]
    fn beautifies_each_chunk_around_delimiter_commands() {
        let sql = "-- purge\ndelimiter //\ncreate procedure purge_logs()\nbegin\n  delete from logs where created_at < now() - interval 30 day;\n  select count(*) n from logs l;\nend //\ndelimiter ;\nselect id, my_function(x) total from video v where a=1 and b=2;";
        assert_eq!(
            beautify_with(
                sql,
                BeautifyOptions {
                    dialect: Dialect::MySql,
                    version: None,
                    catalog: &CatalogNames {
                        routines: vec!["My_Function".to_string()],
                        ..CatalogNames::default()
                    },
                },
            ),
            "-- purge\n\
             DELIMITER //\n\
             CREATE PROCEDURE purge_logs()\n\
             BEGIN\n    \
                 DELETE FROM logs\n    \
                 WHERE created_at < NOW() - INTERVAL 30 DAY;\n    \
                 SELECT ALL COUNT(*) AS n\n    \
                 FROM logs AS l;\n\
             END //\n\
             DELIMITER ;\n\
             SELECT ALL\n    \
                 id,\n    \
                 My_Function(x) AS total\n\
             FROM video AS v\n\
             WHERE TRUE\n    \
                 AND a = 1\n    \
                 AND b = 2;"
        );
    }

    #[test]
    fn beautifying_a_program_twice_changes_nothing() {
        let sql = "delimiter $$\ncreate function f(x int) returns int deterministic\nbegin\n  declare y int;\n  if x > 0 then set y = x; else set y = -x; end if;\n  return y;\nend$$\ndelimiter ;\nselect f(1) a;";
        let once = beautify(sql, Dialect::MySql);
        assert_eq!(beautify(&once, Dialect::MySql), once);
        assert!(once.contains("\nEND $$\nDELIMITER ;\n"), "{once}");
    }

    #[test]
    fn spells_names_from_the_catalog_and_keywords_from_the_query() {
        let catalog = CatalogNames {
            tables: vec!["operations".to_string()],
            columns: vec!["operation_id".to_string(), "cast".to_string()],
            ..CatalogNames::default()
        };
        let options = BeautifyOptions {
            dialect: Dialect::MySql,
            version: Some("8.0.36"),
            catalog: &catalog,
        };
        assert_eq!(
            beautify_with(
                "select cast(CAST as char) c, OPERATION_ID from Operations",
                options
            ),
            "SELECT ALL
    CAST(cast AS CHAR) AS c,
    operation_id
FROM operations"
        );
    }

    #[test]
    fn beautifying_twice_changes_nothing() {
        let sql = "select a, b from t1 join t2 on x and y where p or q and r order by a";
        let once = beautify(sql, Dialect::MariaDb);
        assert_eq!(beautify(&once, Dialect::MariaDb), once);
    }

    /// Runs a query on an in-memory SQLite database and returns its rows.
    fn rows_of(db: &rusqlite::Connection, sql: &str) -> Vec<Vec<Option<i64>>> {
        let mut statement = db.prepare(sql).unwrap();
        let columns = statement.column_count();
        statement
            .query_map([], |row| (0..columns).map(|index| row.get(index)).collect())
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    }

    /// The TRUE / FALSE prefixes do not change the result, NULL included:
    /// each query is run as is and beautified, on all the combinations of 0,
    /// 1 and NULL.
    #[test]
    fn prefixes_keep_query_results_identical() {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE t (id INTEGER, a INTEGER, b INTEGER, c INTEGER);
             WITH v(x) AS (VALUES (0), (1), (NULL))
             INSERT INTO t SELECT row_number() OVER (), v1.x, v2.x, v3.x FROM v v1, v v2, v v3;",
        )
        .unwrap();
        let filters = [
            "a = 1 OR b = 1",
            "a = 1 AND b = 1",
            "a = 1 AND b = 1 OR c = 1",
            "a = 1 OR b = 1 AND c = 1",
            "NOT a = 1 AND b = 1 OR NOT c = 1",
            "a BETWEEN 0 AND 1 AND b = 1 OR c IS NULL",
            "CASE WHEN a = 1 AND b = 1 THEN 1 END = 1 OR c = 0",
            "(a = 1 OR b = 1) AND (c = 1 OR c IS NULL)",
        ];
        for filter in filters {
            let original = format!("SELECT id FROM t WHERE {filter} ORDER BY id");
            let beautified = beautify(&original, Dialect::MariaDb);
            assert_eq!(
                rows_of(&db, &beautified),
                rows_of(&db, &original),
                "{beautified}"
            );

            let joined = format!(
                "SELECT t.id, u.uid FROM t JOIN (SELECT id AS uid FROM t) AS u ON t.id = u.uid AND {filter} ORDER BY t.id"
            );
            let beautified = beautify(&joined, Dialect::MariaDb);
            assert_eq!(
                rows_of(&db, &beautified),
                rows_of(&db, &joined),
                "{beautified}"
            );
        }
    }

    #[test]
    fn returns_unreadable_text_unchanged() {
        assert_eq!(pretty("select 'not closed"), "select 'not closed");
    }
}
