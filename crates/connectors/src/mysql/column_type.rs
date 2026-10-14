//! Type des colonnes d'un résultat MySQL ou MariaDB, tel que l'interface
//! l'affiche dans l'en-tête de la grille.
//!
//! Le type vient des métadonnées que le serveur envoie avec le résultat :
//! code du type, indicateurs (`UNSIGNED`, `ENUM`, `SET`), longueur, nombre
//! de décimales et jeu de caractères. C'est le type du *résultat*, qui ne
//! dit pas toujours tout du type déclaré dans la table :
//!
//! - `BOOLEAN` arrive en `TINYINT(1)` ;
//! - le JSON de MariaDB arrive en `LONGTEXT` ;
//! - `ENUM` et `SET` se reconnaissent à un indicateur, sans leurs valeurs ;
//! - une expression calculée (`COUNT(*)`, `CONCAT(...)`) a le type choisi
//!   par le serveur.
//!
//! La longueur d'un texte arrive en octets, dans le jeu de caractères des
//! résultats de la connexion : elle est ramenée en caractères pour les jeux
//! connus (`charset_max_bytes`). Pour un autre jeu, la longueur est omise
//! plutôt que devinée.

use mysql_async::consts::{ColumnFlags, ColumnType};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Famille d'un type de colonne, pour l'icône de l'en-tête de la grille.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum TypeFamily {
    /// `TINYINT` à `BIGINT`.
    Integer,
    /// `DECIMAL`.
    Decimal,
    /// `FLOAT`, `DOUBLE`.
    Float,
    /// `CHAR`, `VARCHAR`, `TEXT`.
    Text,
    /// `BINARY`, `VARBINARY`, `BLOB`.
    Binary,
    /// `DATE`.
    Date,
    /// `TIME`.
    Time,
    /// `DATETIME`, `TIMESTAMP`.
    Datetime,
    /// `YEAR`.
    Year,
    /// `JSON` (MySQL).
    Json,
    /// `BIT`.
    Bit,
    /// `ENUM`.
    Enum,
    /// `SET`.
    Set,
    /// `GEOMETRY` et ses dérivés.
    Geometry,
    /// Autre type, ou `NULL` (colonne d'une expression `NULL`).
    Other,
}

/// Type d'une colonne de résultat : sa famille et son nom natif.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ColumnTypeInfo {
    /// Famille du type.
    pub family: TypeFamily,
    /// Nom du type dans le dialecte du serveur : `INT UNSIGNED`,
    /// `DECIMAL(10,2)`, `VARCHAR(20)`, `DATETIME(6)`...
    pub name: String,
}

/// Collation du jeu de caractères `binary` : une chaîne d'octets.
const BINARY_COLLATION: u16 = 63;

/// Nombre de décimales « non fixé » envoyé par le serveur (expression dont
/// la précision n'est pas connue).
const NOT_FIXED_DEC: u8 = 31;

/// Décrit le type d'une colonne à partir de ses métadonnées de résultat.
///
/// `length` est la longueur annoncée (en octets pour un texte), `charset`
/// l'identifiant de collation des résultats.
pub fn describe(
    column_type: ColumnType,
    flags: ColumnFlags,
    length: u32,
    decimals: u8,
    charset: u16,
) -> ColumnTypeInfo {
    let unsigned = flags.contains(ColumnFlags::UNSIGNED_FLAG);
    let binary = charset == BINARY_COLLATION;
    let (family, name) = if flags.contains(ColumnFlags::ENUM_FLAG) {
        (TypeFamily::Enum, "ENUM".to_string())
    } else if flags.contains(ColumnFlags::SET_FLAG) {
        (TypeFamily::Set, "SET".to_string())
    } else {
        match column_type {
            ColumnType::MYSQL_TYPE_TINY if length == 1 => {
                (TypeFamily::Integer, numeric("TINYINT(1)", unsigned))
            }
            ColumnType::MYSQL_TYPE_TINY => (TypeFamily::Integer, numeric("TINYINT", unsigned)),
            ColumnType::MYSQL_TYPE_SHORT => (TypeFamily::Integer, numeric("SMALLINT", unsigned)),
            ColumnType::MYSQL_TYPE_INT24 => (TypeFamily::Integer, numeric("MEDIUMINT", unsigned)),
            ColumnType::MYSQL_TYPE_LONG => (TypeFamily::Integer, numeric("INT", unsigned)),
            ColumnType::MYSQL_TYPE_LONGLONG => (TypeFamily::Integer, numeric("BIGINT", unsigned)),
            ColumnType::MYSQL_TYPE_DECIMAL | ColumnType::MYSQL_TYPE_NEWDECIMAL => {
                (TypeFamily::Decimal, decimal(length, decimals, unsigned))
            }
            ColumnType::MYSQL_TYPE_FLOAT => (TypeFamily::Float, numeric("FLOAT", unsigned)),
            ColumnType::MYSQL_TYPE_DOUBLE => (TypeFamily::Float, numeric("DOUBLE", unsigned)),
            ColumnType::MYSQL_TYPE_YEAR => (TypeFamily::Year, "YEAR".to_string()),
            ColumnType::MYSQL_TYPE_DATE | ColumnType::MYSQL_TYPE_NEWDATE => {
                (TypeFamily::Date, "DATE".to_string())
            }
            ColumnType::MYSQL_TYPE_TIME | ColumnType::MYSQL_TYPE_TIME2 => {
                (TypeFamily::Time, with_fraction("TIME", decimals))
            }
            ColumnType::MYSQL_TYPE_DATETIME | ColumnType::MYSQL_TYPE_DATETIME2 => {
                (TypeFamily::Datetime, with_fraction("DATETIME", decimals))
            }
            ColumnType::MYSQL_TYPE_TIMESTAMP | ColumnType::MYSQL_TYPE_TIMESTAMP2 => {
                (TypeFamily::Datetime, with_fraction("TIMESTAMP", decimals))
            }
            ColumnType::MYSQL_TYPE_BIT => (TypeFamily::Bit, format!("BIT({length})")),
            ColumnType::MYSQL_TYPE_JSON => (TypeFamily::Json, "JSON".to_string()),
            ColumnType::MYSQL_TYPE_ENUM => (TypeFamily::Enum, "ENUM".to_string()),
            ColumnType::MYSQL_TYPE_SET => (TypeFamily::Set, "SET".to_string()),
            ColumnType::MYSQL_TYPE_GEOMETRY => (TypeFamily::Geometry, "GEOMETRY".to_string()),
            ColumnType::MYSQL_TYPE_STRING if binary => {
                (TypeFamily::Binary, sized("BINARY", Some(length)))
            }
            ColumnType::MYSQL_TYPE_STRING => {
                (TypeFamily::Text, sized("CHAR", characters(length, charset)))
            }
            ColumnType::MYSQL_TYPE_VAR_STRING | ColumnType::MYSQL_TYPE_VARCHAR if binary => {
                (TypeFamily::Binary, sized("VARBINARY", Some(length)))
            }
            ColumnType::MYSQL_TYPE_VAR_STRING | ColumnType::MYSQL_TYPE_VARCHAR => (
                TypeFamily::Text,
                sized("VARCHAR", characters(length, charset)),
            ),
            ColumnType::MYSQL_TYPE_TINY_BLOB
            | ColumnType::MYSQL_TYPE_MEDIUM_BLOB
            | ColumnType::MYSQL_TYPE_LONG_BLOB
            | ColumnType::MYSQL_TYPE_BLOB => large_object(length, charset),
            ColumnType::MYSQL_TYPE_VECTOR => (TypeFamily::Other, "VECTOR".to_string()),
            ColumnType::MYSQL_TYPE_NULL => (TypeFamily::Other, "NULL".to_string()),
            _ => (TypeFamily::Other, "?".to_string()),
        }
    };
    ColumnTypeInfo { family, name }
}

/// Nom d'un type numérique, suivi de `UNSIGNED` s'il y a lieu.
fn numeric(name: &str, unsigned: bool) -> String {
    if unsigned {
        format!("{name} UNSIGNED")
    } else {
        name.to_string()
    }
}

/// `DECIMAL(précision,échelle)`. La longueur annoncée compte la virgule
/// (s'il y a des décimales) et le signe (sauf `UNSIGNED`).
fn decimal(length: u32, scale: u8, unsigned: bool) -> String {
    let precision = length
        .saturating_sub(u32::from(scale > 0))
        .saturating_sub(u32::from(!unsigned));
    numeric(&format!("DECIMAL({precision},{scale})"), unsigned)
}

/// Nom d'un type temporel, avec ses fractions de seconde s'il en a.
fn with_fraction(name: &str, decimals: u8) -> String {
    if decimals == 0 || decimals >= NOT_FIXED_DEC {
        name.to_string()
    } else {
        format!("{name}({decimals})")
    }
}

/// Nom d'un type avec sa longueur entre parenthèses, si elle est connue.
fn sized(name: &str, length: Option<u32>) -> String {
    match length {
        Some(length) => format!("{name}({length})"),
        None => name.to_string(),
    }
}

/// `TINYTEXT` à `LONGTEXT`, ou `TINYBLOB` à `LONGBLOB` pour le jeu `binary`,
/// d'après la longueur maximale ramenée en caractères.
fn large_object(length: u32, charset: u16) -> (TypeFamily, String) {
    let (family, suffix) = if charset == BINARY_COLLATION {
        (TypeFamily::Binary, "BLOB")
    } else {
        (TypeFamily::Text, "TEXT")
    };
    let Some(characters) = characters(length, charset) else {
        return (family, suffix.to_string());
    };
    let prefix = match characters {
        0..=255 => "TINY",
        256..=65_535 => "",
        65_536..=16_777_215 => "MEDIUM",
        _ => "LONG",
    };
    (family, format!("{prefix}{suffix}"))
}

/// Longueur d'un texte en caractères, à partir de sa longueur en octets ;
/// `None` pour un jeu de caractères inconnu.
fn characters(bytes: u32, charset: u16) -> Option<u32> {
    charset_max_bytes(charset).map(|max| bytes / max)
}

/// Nombre maximal d'octets par caractère des jeux de caractères courants,
/// d'après l'identifiant de collation ; `None` pour les autres.
fn charset_max_bytes(collation: u16) -> Option<u32> {
    match collation {
        // utf8mb4 : utf8mb4_general_ci, _bin, collations Unicode et 0900 de MySQL 8.
        45 | 46 | 224..=247 | 255..=323 => Some(4),
        // utf8mb3 (utf8 avant MySQL 8).
        33 | 76 | 83 | 192..=215 | 223 => Some(3),
        // binary, latin1, ascii.
        63 | 5 | 8 | 15 | 31 | 47..=49 | 94 | 11 | 65 => Some(1),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Collation utf8mb4_general_ci, celle des résultats de Prisme.
    const UTF8MB4: u16 = 45;

    fn name(
        column_type: ColumnType,
        flags: ColumnFlags,
        length: u32,
        decimals: u8,
        charset: u16,
    ) -> String {
        describe(column_type, flags, length, decimals, charset).name
    }

    #[test]
    fn names_numbers() {
        let none = ColumnFlags::empty();
        assert_eq!(name(ColumnType::MYSQL_TYPE_LONG, none, 11, 0, 63), "INT");
        assert_eq!(
            name(
                ColumnType::MYSQL_TYPE_LONGLONG,
                ColumnFlags::UNSIGNED_FLAG,
                20,
                0,
                63
            ),
            "BIGINT UNSIGNED"
        );
        assert_eq!(
            name(ColumnType::MYSQL_TYPE_TINY, none, 1, 0, 63),
            "TINYINT(1)"
        );
        // DECIMAL(10,2) signé : 10 chiffres, la virgule et le signe.
        assert_eq!(
            name(ColumnType::MYSQL_TYPE_NEWDECIMAL, none, 12, 2, 63),
            "DECIMAL(10,2)"
        );
        assert_eq!(
            name(
                ColumnType::MYSQL_TYPE_NEWDECIMAL,
                ColumnFlags::UNSIGNED_FLAG,
                5,
                0,
                63
            ),
            "DECIMAL(5,0) UNSIGNED"
        );
    }

    #[test]
    fn names_texts_in_characters() {
        let none = ColumnFlags::empty();
        assert_eq!(
            name(ColumnType::MYSQL_TYPE_VAR_STRING, none, 80, 0, UTF8MB4),
            "VARCHAR(20)"
        );
        assert_eq!(
            name(ColumnType::MYSQL_TYPE_STRING, none, 12, 0, 33),
            "CHAR(4)"
        );
        assert_eq!(
            name(ColumnType::MYSQL_TYPE_VAR_STRING, none, 16, 0, 63),
            "VARBINARY(16)"
        );
        assert_eq!(
            name(ColumnType::MYSQL_TYPE_BLOB, none, 262_140, 0, UTF8MB4),
            "TEXT"
        );
        assert_eq!(
            name(ColumnType::MYSQL_TYPE_BLOB, none, u32::MAX, 0, UTF8MB4),
            "LONGTEXT"
        );
        assert_eq!(
            name(ColumnType::MYSQL_TYPE_BLOB, none, 255, 0, 63),
            "TINYBLOB"
        );
        // Jeu de caractères inconnu : la longueur est omise.
        assert_eq!(
            name(ColumnType::MYSQL_TYPE_VAR_STRING, none, 80, 0, 2000),
            "VARCHAR"
        );
    }

    #[test]
    fn names_temporal_types_and_flags() {
        let none = ColumnFlags::empty();
        assert_eq!(
            name(ColumnType::MYSQL_TYPE_DATETIME, none, 26, 6, 63),
            "DATETIME(6)"
        );
        assert_eq!(
            name(ColumnType::MYSQL_TYPE_TIMESTAMP, none, 19, 0, 63),
            "TIMESTAMP"
        );
        assert_eq!(
            name(ColumnType::MYSQL_TYPE_TIME, none, 10, NOT_FIXED_DEC, 63),
            "TIME"
        );
        let enumeration = describe(
            ColumnType::MYSQL_TYPE_STRING,
            ColumnFlags::ENUM_FLAG,
            4,
            0,
            UTF8MB4,
        );
        assert_eq!(
            (enumeration.family, enumeration.name.as_str()),
            (TypeFamily::Enum, "ENUM")
        );
        assert_eq!(
            describe(ColumnType::MYSQL_TYPE_DATE, none, 10, 0, 63).family,
            TypeFamily::Date
        );
    }
}
