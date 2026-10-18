//! Character sets of each dialect, as names of an introducer (`_latin1'a'`):
//! a word `_` followed by one of them introduces the string, hexadecimal or
//! bit literal that follows, any other word is a name (`_foo 'a'` is the
//! column `_foo` and its alias).
//!
//! Both dialects have the same character sets but two: `gb18030` in MySQL
//! (from 5.7.4), `filename` in MariaDB, an introducer only, absent from
//! `information_schema.CHARACTER_SETS`. `utf8` names `utf8mb3` in both.
//! Their default collations are the same, those of the Unicode character
//! sets aside (`default_collation`). Measured on MySQL 8.0.11, 8.0.28, 8.4.11
//! and 9.3.0, MariaDB 10.5.29, 10.6.28, 10.11.19, 11.4.13 and 11.8.9
//! (`crates/probe/cases/charsets.sql`, `literals-introducers.txt`), the
//! collations from MariaDB 10.5.29 to 11.8.9
//! (`literals-collations.txt`).

use crate::Dialect;
use crate::versions::Construct;

/// Character sets of both dialects, in lower case.
const COMMON: [&str; 41] = [
    "armscii8", "ascii", "big5", "binary", "cp1250", "cp1251", "cp1256", "cp1257", "cp850",
    "cp852", "cp866", "cp932", "dec8", "eucjpms", "euckr", "gb2312", "gbk", "geostd8", "greek",
    "hebrew", "hp8", "keybcs2", "koi8r", "koi8u", "latin1", "latin2", "latin5", "latin7", "macce",
    "macroman", "sjis", "swe7", "tis620", "ucs2", "ujis", "utf16", "utf16le", "utf32", "utf8",
    "utf8mb3", "utf8mb4",
];

/// Tells whether `name`, in any case, is a character set of `dialect`.
pub(crate) fn is_charset(dialect: Dialect, name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    COMMON.contains(&name.as_str())
        || match dialect {
            Dialect::MySql => name == "gb18030",
            Dialect::MariaDb => name == "filename",
        }
}

/// Default collation of `charset`, in lower case, in `dialect`, of the
/// constructs `has` accepts, for a character set whose default collation
/// depends on the dialect or the version: `utf8mb4` and the other Unicode
/// character sets but `utf16le`. `None` for another character set, of the
/// same default collation in both dialects.
pub(crate) fn default_collation(
    charset: &str,
    dialect: Dialect,
    has: impl Fn(Construct) -> bool,
) -> Option<String> {
    let name = match charset {
        "utf8" => "utf8mb3",
        "utf8mb3" | "utf8mb4" | "ucs2" | "utf16" | "utf32" => charset,
        _ => return None,
    };
    let uca1400 = match name {
        "utf8mb4" => has(Construct::Utf8mb4Uca1400),
        _ => has(Construct::UnicodeUca1400),
    };
    Some(match (dialect, name) {
        (Dialect::MySql, "utf8mb4") => "utf8mb4_0900_ai_ci".to_string(),
        _ if uca1400 => format!("{name}_uca1400_ai_ci"),
        _ => format!("{name}_general_ci"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_dialect_has_its_own_character_sets() {
        assert!(is_charset(Dialect::MySql, "LATIN1"));
        assert!(is_charset(Dialect::MariaDb, "utf8mb4"));
        assert!(is_charset(Dialect::MySql, "gb18030"));
        assert!(!is_charset(Dialect::MariaDb, "gb18030"));
        assert!(is_charset(Dialect::MariaDb, "filename"));
        assert!(!is_charset(Dialect::MySql, "filename"));
        assert!(!is_charset(Dialect::MySql, "foo"));
    }

    #[test]
    fn the_unicode_character_sets_have_a_default_collation_by_version() {
        let none = |_| false;
        let all = |_| true;
        let collation =
            |charset, dialect, has: fn(Construct) -> bool| default_collation(charset, dialect, has);
        assert_eq!(
            collation("utf8mb4", Dialect::MySql, none).as_deref(),
            Some("utf8mb4_0900_ai_ci")
        );
        assert_eq!(
            collation("utf8mb4", Dialect::MariaDb, none).as_deref(),
            Some("utf8mb4_general_ci")
        );
        assert_eq!(
            collation("utf8", Dialect::MariaDb, all).as_deref(),
            Some("utf8mb3_uca1400_ai_ci")
        );
        assert_eq!(
            collation("ucs2", Dialect::MySql, none).as_deref(),
            Some("ucs2_general_ci")
        );
        assert_eq!(collation("latin1", Dialect::MariaDb, all), None);
        assert_eq!(collation("utf16le", Dialect::MariaDb, all), None);
    }
}
