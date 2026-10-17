//! `CAST` and `CONVERT`: the operand converted to the data type of the
//! expression, with the target as written, since a type has several
//! spellings (`SIGNED`, `SIGNED INTEGER`, `INT` in MariaDB) and a dialect may
//! lack some of them.

use crate::Expr;

/// `CAST` or `CONVERT`.
#[derive(Debug, Clone, PartialEq)]
pub struct Cast {
    /// Value converted.
    pub operand: Expr,
    /// Type written after `AS`, after the comma of `CONVERT`, or the
    /// character set of `CONVERT ... USING` (`CHAR CHARACTER SET name`).
    pub target: CastTarget,
    /// Form written.
    pub form: CastForm,
}

/// Form of a conversion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CastForm {
    /// `CAST(x AS type)`.
    Cast,
    /// `CAST(x AT TIME ZONE [INTERVAL] 'zone' AS DATETIME)` (MySQL): the
    /// date and time in UTC of a `TIMESTAMP`.
    AtTimeZone {
        /// Time zone, as written between quotes: `+00:00` or `UTC`.
        zone: String,
        /// `INTERVAL` written before the zone.
        interval: bool,
    },
    /// `CONVERT(x, type)`.
    Convert,
    /// `CONVERT(x USING charset)`, of target `CHAR CHARACTER SET charset`.
    ConvertUsing,
}

/// Target of a conversion, as written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CastTarget {
    /// `SIGNED`, followed by `INTEGER` or `INT` when written.
    Signed(Option<IntegerWord>),
    /// `UNSIGNED`, followed by `INTEGER` or `INT` when written.
    Unsigned(Option<IntegerWord>),
    /// `INTEGER` or `INT` alone (MariaDB): `SIGNED`.
    Integer(IntegerWord),
    /// `DECIMAL[(precision[, scale])]` or `DEC[(...)]`.
    Decimal {
        /// `DEC` written instead of `DECIMAL`.
        dec: bool,
        /// Precision written; 0 stands for the default, 10.
        precision: Option<u8>,
        /// Scale written.
        scale: Option<u8>,
    },
    /// `DOUBLE`, `DOUBLE PRECISION` (MySQL), `DOUBLE(m, d)` (MariaDB).
    Double {
        /// `PRECISION` written after `DOUBLE`.
        precision: bool,
        /// Digits and decimals written (`DOUBLE(5, 2)`), which round the
        /// value.
        digits: Option<(u8, u8)>,
    },
    /// `FLOAT` or `FLOAT(p)` (MySQL): a `DOUBLE` from 25 bits.
    Float(Option<u8>),
    /// `REAL` (MySQL): a `DOUBLE`, a `FLOAT` under `REAL_AS_FLOAT`.
    Real {
        /// The SQL mode `REAL_AS_FLOAT` reads it as a `FLOAT`.
        float: bool,
    },
    /// Character string: `CHAR`, `NCHAR`, `VARCHAR(n)` (MariaDB)...
    Char(CharTarget),
    /// `BINARY[(n)]`: a byte string, padded with zero bytes to `n`.
    Binary(Option<u32>),
    /// `DATE`.
    Date,
    /// `DATETIME[(n)]`.
    DateTime(Option<u8>),
    /// `TIME[(n)]`.
    Time(Option<u8>),
    /// `JSON` (MySQL).
    Json,
    /// `YEAR` (MySQL).
    Year,
    /// Spatial type (MySQL): `POINT`, `LINESTRING`, `POLYGON`,
    /// `MULTIPOINT`, `MULTILINESTRING`, `MULTIPOLYGON`,
    /// `GEOMETRYCOLLECTION`, in capitals.
    Spatial(String),
    /// `INTERVAL DAY_SECOND(n)` (MariaDB): a duration written as a string.
    Interval(u8),
    /// Type of a plugin of MariaDB, in capitals: `INET4`, `INET6`, `UUID`.
    Named(String),
}

/// Word written after `SIGNED` or `UNSIGNED`, or alone in MariaDB.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntegerWord {
    /// `INTEGER`.
    Integer,
    /// `INT`.
    Int,
}

/// Character string target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CharTarget {
    /// Word written.
    pub word: CharWord,
    /// Length in characters, when written.
    pub length: Option<u32>,
    /// Character set, when written.
    pub charset: Option<Charset>,
    /// `BINARY` written after the type: the binary collation of the
    /// character set.
    pub binary: bool,
    /// `COLLATE name` written in the target (MariaDB).
    pub collation: Option<String>,
}

/// Word of a character string target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CharWord {
    /// `CHAR`.
    Char,
    /// `CHARACTER`.
    Character,
    /// `NCHAR`: the character set `utf8mb3`.
    NChar,
    /// `NATIONAL CHAR` (MySQL): the character set `utf8mb3`.
    NationalChar,
    /// `VARCHAR(n)` (MariaDB): `CHAR(n)`.
    VarChar,
}

/// Character set of a character string target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Charset {
    /// `CHARACTER SET name`, `CHARSET name`.
    Named {
        /// Name, as written.
        name: String,
        /// `CHARSET` written instead of `CHARACTER SET`.
        short: bool,
        /// Name between quotes (`CHARACTER SET 'latin1'`).
        quoted: bool,
    },
    /// `ASCII`: the character set `latin1`.
    Ascii,
    /// `UNICODE`: the character set `ucs2`.
    Unicode,
    /// `BYTE`: the character set `binary`, a byte string.
    Byte,
}

impl Charset {
    /// Name of the character set, in lower case.
    pub fn name(&self) -> String {
        match self {
            Self::Named { name, .. } => name.to_ascii_lowercase(),
            Self::Ascii => "latin1".to_string(),
            Self::Unicode => "ucs2".to_string(),
            Self::Byte => "binary".to_string(),
        }
    }
}
