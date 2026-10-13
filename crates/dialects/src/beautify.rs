//! Enjoliveur : réécrit le SQL saisi en « beau SQL », règle par règle.
//!
//! Règles en place :
//!
//! 1. **Blancs normalisés.** Espaces insécables, fines et autres blancs
//!    exotiques deviennent des espaces normales, une tabulation quatre
//!    espaces, hors chaînes et identifiants entre accents graves (module
//!    `whitespace`).
//!
//! 2. **`AS` devant les alias** de colonne et de table qui n'en ont pas
//!    (module `alias`).
//!
//! 3. **Noms de fonctions en capitales**, sauf les fonctions et procédures
//!    de l'utilisateur, écrites dans la casse de leur définition (module
//!    `functions`).
//!
//! 4. **Mots-clefs en capitales.** Seuls les mots réservés de MySQL passent
//!    en capitales, plus quelques mots-clefs sans ambiguïté (`EXTRA_KEYWORDS`).
//!    Un mot réservé ne peut jamais servir d'identifiant sans accents graves :
//!    les noms de tables et de colonnes (`name`, `status`, `date`...) restent
//!    donc tels quels, même quand ils ressemblent à un mot-clef. Les unités
//!    de temps (`second`, `day`...) ne sont pas réservées : elles passent en
//!    capitales seulement là où elles ne peuvent être qu'une unité, après
//!    `INTERVAL <valeur>` et en premier argument d'`EXTRACT`,
//!    `TIMESTAMPADD` et `TIMESTAMPDIFF`.
//!
//! 5. **Mise en page** : une clause par ligne, une colonne et un prédicat
//!    par ligne quand il y en a plusieurs, espaces autour des opérateurs
//!    (module `layout`).
//!
//! Un texte qui contient la commande `DELIMITER` (procédures, fonctions...)
//! est d'abord découpé en morceaux par le découpeur de requêtes : chaque
//! requête reçoit toutes les règles, avec son propre séparateur, et le corps
//! d'un programme stocké est mis en page bloc par bloc (`BEGIN ... END`,
//! `IF ... END IF`...).
//!
//! Le texte des jetons est conservé à l'identique : commentaires, chaînes,
//! identifiants entre accents graves. Le texte est découpé en jetons par
//! `sqlparser` ; les mots-clefs sont mis en capitales à leur position
//! d'origine, puis la mise en page ne change que les espaces et les retours à
//! la ligne.

use sqlparser::tokenizer::{Token, Tokenizer};

use prisme_splitter::{Chunk, chunks};

use crate::Dialect;
use crate::alias::add_alias_keyword;
use crate::functions::case_function_names;
use crate::keywords::is_keyword;
use crate::layout::{has_delimiter_command, layout};
use crate::limit::byte_offset;
use crate::whitespace::normalize_blanks;

/// Unités de temps de MySQL, en capitales dans les positions où elles ne
/// peuvent être qu'une unité (les unités composées, `DAY_HOUR`..., sont réservées).
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

/// Fonctions dont le premier argument est une unité de temps.
const UNIT_FUNCTIONS: &[&str] = &["EXTRACT", "TIMESTAMPADD", "TIMESTAMPDIFF"];

/// Enjolive `sql` sans connaître les fonctions de l'utilisateur : toutes
/// les fonctions passent en capitales. Voir `beautify_with_routines`.
pub fn beautify(sql: &str, dialect: Dialect) -> String {
    beautify_with_routines(sql, dialect, &[])
}

/// Enjolive `sql` : blancs normalisés, `AS` devant les alias, noms de
/// fonctions en capitales, mots-clefs en capitales, puis mise en page
/// (module `layout`).
///
/// `routines` donne le nom, tel que défini, des fonctions et procédures de
/// l'utilisateur (catalogue de la connexion) : elles gardent la casse de
/// leur définition au lieu de passer en capitales.
///
/// Un texte que le découpeur en jetons ne comprend pas (chaîne non fermée...)
/// est renvoyé tel quel.
pub fn beautify_with_routines(sql: &str, dialect: Dialect, routines: &[String]) -> String {
    let normalized = normalize_blanks(sql);
    if has_delimiter_command(&normalized) {
        beautify_chunks(&normalized, dialect, routines)
    } else {
        beautify_part(&normalized, dialect, routines, false)
    }
}

/// Enjolive un texte qui contient des commandes `DELIMITER`, morceau par
/// morceau : chaque commande sur sa ligne, puis chaque requête avec son
/// séparateur, une ligne vide entre deux requêtes.
fn beautify_chunks(sql: &str, dialect: Dialect, routines: &[String]) -> String {
    let mut result = String::new();
    // Le morceau précédent appelle-t-il une simple fin de ligne (commande,
    // commentaires seuls) plutôt qu'une ligne vide ? Une commande est
    // toujours collée à ce qui la précède.
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
                // Hors du « ; » habituel, la requête peut être un programme stocké.
                let program = delimiter != ";";
                let mut part = beautify_part(&text, dialect, routines, program);
                let only_comments = end.is_empty() && is_comment_only(&part, dialect);
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

/// Enjolive une partie de texte sans commande `DELIMITER` ; avec `program`,
/// une seule requête terminée par un autre séparateur que `;`, qui peut être
/// un programme stocké.
fn beautify_part(sql: &str, dialect: Dialect, routines: &[String], program: bool) -> String {
    let aliased = add_alias_keyword(sql, dialect);
    let functions = case_function_names(&aliased, dialect, routines);
    layout(&uppercase_keywords(&functions, dialect), dialect, program)
}

/// Le texte n'est-il fait que de commentaires (et de blancs) ?
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

/// Met les mots-clefs en capitales, sans rien changer d'autre.
fn uppercase_keywords(sql: &str, dialect: Dialect) -> String {
    let parser_dialect = dialect.parser_dialect();
    let Ok(tokens) = Tokenizer::new(parser_dialect.as_ref(), sql).tokenize_with_location() else {
        return sql.to_string();
    };

    let mut result = String::with_capacity(sql.len());
    let mut copied = 0;
    // Une unité de temps est attendue : après INTERVAL, ou en premier argument d'EXTRACT...
    let mut unit_expected = false;
    // Dernier mot lu, pour reconnaître EXTRACT( et ses semblables.
    let mut previous_word = String::new();
    for token in tokens {
        let word = match &token.token {
            Token::Word(word) if word.quote_style.is_none() => word,
            Token::LParen => {
                unit_expected |= UNIT_FUNCTIONS.contains(&previous_word.as_str());
                previous_word.clear();
                continue;
            }
            Token::SemiColon => {
                unit_expected = false;
                previous_word.clear();
                continue;
            }
            Token::Whitespace(_) => continue,
            _ => {
                previous_word.clear();
                continue;
            }
        };
        let upper = word.value.to_ascii_uppercase();
        let is_unit = unit_expected && TIME_UNITS.contains(&upper.as_str());
        if is_unit {
            unit_expected = false;
        }
        if upper == "INTERVAL" {
            unit_expected = true;
        }
        previous_word = upper;
        if !is_unit && !is_keyword(&word.value) {
            continue;
        }
        let (Some(start), Some(end)) = (
            byte_offset(sql, token.span.start),
            byte_offset(sql, token.span.end),
        ) else {
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
        uppercase_keywords(sql, Dialect::MariaDb)
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
            pretty("select case when x then 'été' else 'hiver' end as saison from t"),
            "SELECT CASE WHEN x THEN 'été' ELSE 'hiver' END AS saison FROM t"
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
        // Ailleurs, ces mots peuvent être des noms de colonnes.
        assert_eq!(pretty("select day, year from t"), "SELECT day, year FROM t");
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
        let sql = "-- purge\ndelimiter //\ncreate procedure purge_logs()\nbegin\n  delete from logs where created_at < now() - interval 30 day;\n  select count(*) n from logs l;\nend //\ndelimiter ;\nselect id, ma_fonction(x) total from video v where a=1 and b=2;";
        assert_eq!(
            beautify_with_routines(sql, Dialect::MySql, &["Ma_Fonction".to_string()]),
            "-- purge\n\
             DELIMITER //\n\
             CREATE PROCEDURE purge_logs()\n\
             BEGIN\n    \
                 DELETE\n    \
                 FROM logs\n    \
                 WHERE created_at < NOW() - INTERVAL 30 DAY;\n    \
                 SELECT ALL COUNT(*) AS n\n    \
                 FROM logs AS l;\n\
             END //\n\
             DELIMITER ;\n\
             SELECT ALL\n    \
                 id,\n    \
                 Ma_Fonction(x) AS total\n\
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
    fn beautifying_twice_changes_nothing() {
        let sql = "select a, b from t1 join t2 on x and y where p or q and r order by a";
        let once = beautify(sql, Dialect::MariaDb);
        assert_eq!(beautify(&once, Dialect::MariaDb), once);
    }

    /// Exécute une requête sur une base SQLite en mémoire et renvoie ses lignes.
    fn rows_of(db: &rusqlite::Connection, sql: &str) -> Vec<Vec<Option<i64>>> {
        let mut statement = db.prepare(sql).unwrap();
        let columns = statement.column_count();
        statement
            .query_map([], |row| (0..columns).map(|index| row.get(index)).collect())
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    }

    /// Les préfixes TRUE / FALSE ne changent pas le résultat, NULL compris :
    /// chaque requête est exécutée telle quelle et enjolivée, sur toutes les
    /// combinaisons de 0, 1 et NULL.
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
        assert_eq!(pretty("select 'pas fermée"), "select 'pas fermée");
    }
}
