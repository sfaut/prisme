//! Enjoliveur : réécrit le SQL saisi en « beau SQL », règle par règle.
//!
//! Règles en place :
//!
//! 1. **Mots-clefs en capitales.** Seuls les mots réservés de MySQL passent
//!    en capitales, plus quelques mots-clefs sans ambiguïté (`EXTRA_KEYWORDS`).
//!    Un mot réservé ne peut jamais servir d'identifiant sans accents graves :
//!    les noms de tables et de colonnes (`name`, `status`, `date`...) restent
//!    donc tels quels, même quand ils ressemblent à un mot-clef. Les unités
//!    de temps (`second`, `day`...) ne sont pas réservées : elles passent en
//!    capitales seulement là où elles ne peuvent être qu'une unité, après
//!    `INTERVAL <valeur>` et en premier argument d'`EXTRACT`,
//!    `TIMESTAMPADD` et `TIMESTAMPDIFF`.
//!
//! 2. **Mise en page** : une clause par ligne, une colonne et un prédicat
//!    par ligne quand il y en a plusieurs, espaces autour des opérateurs
//!    (module `layout`).
//!
//! Le texte des jetons est conservé à l'identique : commentaires, chaînes,
//! identifiants entre accents graves. Le texte est découpé en jetons par
//! `sqlparser` ; les mots-clefs sont mis en capitales à leur position
//! d'origine, puis la mise en page ne change que les espaces et les retours à
//! la ligne.

use sqlparser::tokenizer::{Token, Tokenizer};

use crate::Dialect;
use crate::keywords::is_keyword;
use crate::layout::layout;
use crate::limit::byte_offset;

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

/// Enjolive `sql` : mots-clefs en capitales, puis mise en page (module `layout`).
///
/// Un texte que le découpeur en jetons ne comprend pas (chaîne non fermée...)
/// est renvoyé tel quel.
pub fn beautify(sql: &str, dialect: Dialect) -> String {
    layout(&uppercase_keywords(sql, dialect), dialect)
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
            "SELECT\n    a,\n    b\nFROM t\nWHERE x = 1\n    AND y = 2"
        );
    }

    #[test]
    fn returns_unreadable_text_unchanged() {
        assert_eq!(pretty("select 'pas fermée"), "select 'pas fermée");
    }
}
