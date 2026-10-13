//! Enjoliveur : réécrit le SQL saisi en « beau SQL », règle par règle.
//!
//! Règle en place :
//!
//! 1. **Mots-clefs en capitales.** Seuls les mots réservés de MySQL passent
//!    en capitales, plus quelques mots-clefs sans ambiguïté (`EXTRA_KEYWORDS`).
//!    Un mot réservé ne peut jamais servir d'identifiant sans accents graves :
//!    les noms de tables et de colonnes (`name`, `status`, `date`...) restent
//!    donc tels quels, même quand ils ressemblent à un mot-clef.
//!
//! Tout le reste du texte est conservé à l'identique : espaces, retours à la
//! ligne, commentaires, chaînes, identifiants entre accents graves. Le texte
//! est découpé en jetons par `sqlparser`, et seuls les mots concernés sont
//! remplacés, à leur position d'origine.

use sqlparser::tokenizer::{Token, Tokenizer};

use crate::Dialect;
use crate::limit::byte_offset;

/// Mots réservés de MySQL 8 : ils ne peuvent pas servir d'identifiant sans accents graves.
const RESERVED_WORDS: &[&str] = &[
    "ACCESSIBLE",
    "ADD",
    "ALL",
    "ALTER",
    "ANALYZE",
    "AND",
    "AS",
    "ASC",
    "ASENSITIVE",
    "BEFORE",
    "BETWEEN",
    "BIGINT",
    "BINARY",
    "BLOB",
    "BOTH",
    "BY",
    "CALL",
    "CASCADE",
    "CASE",
    "CHANGE",
    "CHAR",
    "CHARACTER",
    "CHECK",
    "COLLATE",
    "COLUMN",
    "CONDITION",
    "CONSTRAINT",
    "CONTINUE",
    "CONVERT",
    "CREATE",
    "CROSS",
    "CUBE",
    "CUME_DIST",
    "CURRENT_DATE",
    "CURRENT_TIME",
    "CURRENT_TIMESTAMP",
    "CURRENT_USER",
    "CURSOR",
    "DATABASE",
    "DATABASES",
    "DAY_HOUR",
    "DAY_MICROSECOND",
    "DAY_MINUTE",
    "DAY_SECOND",
    "DEC",
    "DECIMAL",
    "DECLARE",
    "DEFAULT",
    "DELAYED",
    "DELETE",
    "DENSE_RANK",
    "DESC",
    "DESCRIBE",
    "DETERMINISTIC",
    "DISTINCT",
    "DISTINCTROW",
    "DIV",
    "DOUBLE",
    "DROP",
    "DUAL",
    "EACH",
    "ELSE",
    "ELSEIF",
    "EMPTY",
    "ENCLOSED",
    "ESCAPED",
    "EXCEPT",
    "EXISTS",
    "EXIT",
    "EXPLAIN",
    "FALSE",
    "FETCH",
    "FIRST_VALUE",
    "FLOAT",
    "FLOAT4",
    "FLOAT8",
    "FOR",
    "FORCE",
    "FOREIGN",
    "FROM",
    "FULLTEXT",
    "FUNCTION",
    "GENERATED",
    "GET",
    "GRANT",
    "GROUP",
    "GROUPING",
    "GROUPS",
    "HAVING",
    "HIGH_PRIORITY",
    "HOUR_MICROSECOND",
    "HOUR_MINUTE",
    "HOUR_SECOND",
    "IF",
    "IGNORE",
    "IN",
    "INDEX",
    "INFILE",
    "INNER",
    "INOUT",
    "INSENSITIVE",
    "INSERT",
    "INT",
    "INT1",
    "INT2",
    "INT3",
    "INT4",
    "INT8",
    "INTEGER",
    "INTERSECT",
    "INTERVAL",
    "INTO",
    "IO_AFTER_GTIDS",
    "IO_BEFORE_GTIDS",
    "IS",
    "ITERATE",
    "JOIN",
    "JSON_TABLE",
    "KEY",
    "KEYS",
    "KILL",
    "LAG",
    "LAST_VALUE",
    "LATERAL",
    "LEAD",
    "LEADING",
    "LEAVE",
    "LEFT",
    "LIKE",
    "LIMIT",
    "LINEAR",
    "LINES",
    "LOAD",
    "LOCALTIME",
    "LOCALTIMESTAMP",
    "LOCK",
    "LONG",
    "LONGBLOB",
    "LONGTEXT",
    "LOOP",
    "LOW_PRIORITY",
    "MASTER_BIND",
    "MASTER_SSL_VERIFY_SERVER_CERT",
    "MATCH",
    "MAXVALUE",
    "MEDIUMBLOB",
    "MEDIUMINT",
    "MEDIUMTEXT",
    "MIDDLEINT",
    "MINUTE_MICROSECOND",
    "MINUTE_SECOND",
    "MOD",
    "MODIFIES",
    "NATURAL",
    "NOT",
    "NO_WRITE_TO_BINLOG",
    "NTH_VALUE",
    "NTILE",
    "NULL",
    "NUMERIC",
    "OF",
    "ON",
    "OPTIMIZE",
    "OPTIMIZER_COSTS",
    "OPTION",
    "OPTIONALLY",
    "OR",
    "ORDER",
    "OUT",
    "OUTER",
    "OUTFILE",
    "OVER",
    "PARTITION",
    "PERCENT_RANK",
    "PRECISION",
    "PRIMARY",
    "PROCEDURE",
    "PURGE",
    "RANGE",
    "RANK",
    "READ",
    "READS",
    "READ_WRITE",
    "REAL",
    "RECURSIVE",
    "REFERENCES",
    "REGEXP",
    "RELEASE",
    "RENAME",
    "REPEAT",
    "REPLACE",
    "REQUIRE",
    "RESIGNAL",
    "RESTRICT",
    "RETURN",
    "REVOKE",
    "RIGHT",
    "RLIKE",
    "ROW",
    "ROWS",
    "ROW_NUMBER",
    "SCHEMA",
    "SCHEMAS",
    "SECOND_MICROSECOND",
    "SELECT",
    "SENSITIVE",
    "SEPARATOR",
    "SET",
    "SHOW",
    "SIGNAL",
    "SMALLINT",
    "SPATIAL",
    "SPECIFIC",
    "SQL",
    "SQLEXCEPTION",
    "SQLSTATE",
    "SQLWARNING",
    "SQL_BIG_RESULT",
    "SQL_CALC_FOUND_ROWS",
    "SQL_SMALL_RESULT",
    "SSL",
    "STARTING",
    "STORED",
    "STRAIGHT_JOIN",
    "SYSTEM",
    "TABLE",
    "TERMINATED",
    "THEN",
    "TINYBLOB",
    "TINYINT",
    "TINYTEXT",
    "TO",
    "TRAILING",
    "TRIGGER",
    "TRUE",
    "UNDO",
    "UNION",
    "UNIQUE",
    "UNLOCK",
    "UNSIGNED",
    "UPDATE",
    "USAGE",
    "USE",
    "USING",
    "UTC_DATE",
    "UTC_TIME",
    "UTC_TIMESTAMP",
    "VALUES",
    "VARBINARY",
    "VARCHAR",
    "VARCHARACTER",
    "VARYING",
    "VIRTUAL",
    "WHEN",
    "WHERE",
    "WHILE",
    "WINDOW",
    "WITH",
    "WRITE",
    "XOR",
    "YEAR_MONTH",
    "ZEROFILL",
];

/// Mots-clefs non réservés, mais sans ambiguïté en pratique, mis aussi en capitales.
const EXTRA_KEYWORDS: &[&str] = &[
    "BEGIN",
    "COLUMNS",
    "COMMIT",
    "END",
    "OFFSET",
    "ROLLBACK",
    "TABLES",
    "TRANSACTION",
    "TRUNCATE",
    "VIEW",
];

/// Enjolive `sql` : applique les règles de l'enjoliveur, sans rien changer d'autre.
///
/// Un texte que le découpeur en jetons ne comprend pas (chaîne non fermée...)
/// est renvoyé tel quel.
pub fn beautify(sql: &str, dialect: Dialect) -> String {
    let parser_dialect = dialect.parser_dialect();
    let Ok(tokens) = Tokenizer::new(parser_dialect.as_ref(), sql).tokenize_with_location() else {
        return sql.to_string();
    };

    let mut result = String::with_capacity(sql.len());
    let mut copied = 0;
    for token in tokens {
        let Token::Word(word) = &token.token else {
            continue;
        };
        if word.quote_style.is_some() || !is_keyword(&word.value) {
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

/// Indique si un mot non protégé passe en capitales.
fn is_keyword(word: &str) -> bool {
    let upper = word.to_ascii_uppercase();
    RESERVED_WORDS.contains(&upper.as_str()) || EXTRA_KEYWORDS.contains(&upper.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pretty(sql: &str) -> String {
        beautify(sql, Dialect::MariaDb)
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
    fn returns_unreadable_text_unchanged() {
        assert_eq!(pretty("select 'pas fermée"), "select 'pas fermée");
    }
}
