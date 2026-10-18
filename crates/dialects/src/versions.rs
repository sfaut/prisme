//! Registry of the versions of each dialect: the period of validity of each
//! construct of the grammar (`docs/DEV-DIALECTS.md`, step 2).
//!
//! The constructs come from the reference file of each dialect
//! (`crates/dialects/reference/<dialect>.yaml`, `reference` module). The user
//! chooses the version of the dialect they write in; the parser accepts a
//! construct only within its period, and the emitter checks the version of
//! the server the same way.
//! A construct missing from the file of a dialect is absent from it: each
//! dialect stays in its domain.
//!
//! The versions offered to the user are those where the dialect changed (a
//! construct or a built-in function, `builtins` module), from the oldest
//! version covered (`floor`, the first GA release of MySQL 8.0 and of
//! MariaDB 10.6).

use std::fmt;
use std::sync::OnceLock;

use serde::Deserialize;

use crate::Dialect;
use crate::builtins;
use crate::reference::reference;

/// Version of a dialect: `8.0.19`, `10.6.3`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version {
    /// Major number.
    pub major: u16,
    /// Minor number.
    pub minor: u16,
    /// Patch number.
    pub patch: u16,
}

/// Construct of the grammar whose availability depends on the dialect and
/// its version. The identifier of each one is its name in snake case in the
/// registry files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Construct {
    /// `WITH` (common table expressions).
    With,
    /// `WITH RECURSIVE`.
    WithRecursive,
    /// `LIMIT` of a recursive common table expression, which stops its
    /// iterations.
    RecursiveLimit,
    /// Window functions (`OVER`).
    WindowFunctions,
    /// `JSON_TABLE`.
    JsonTable,
    /// `column->path` (MySQL).
    JsonArrow,
    /// `column->>path` (MySQL).
    JsonDoubleArrow,
    /// `LATERAL` derived tables.
    Lateral,
    /// Column list of a derived table: `(SELECT 1, 2) AS d (a, b)`.
    DerivedColumnList,
    /// Derived table that reads columns of an enclosing query.
    DerivedOuterReference,
    /// Set operation between parentheses as the right operand of another:
    /// `a UNION (b UNION c)`.
    NestedSetOperation,
    /// `ORDER BY` of a query grouped `WITH ROLLUP`.
    RollupOrderBy,
    /// `EXPLAIN FORMAT=TREE`.
    ExplainFormatTree,
    /// `value MEMBER OF (array)`.
    MemberOf,
    /// `EXPLAIN ANALYZE`.
    ExplainAnalyze,
    /// `VALUES ROW (...)` statement (MySQL).
    ValuesRow,
    /// `VALUES (...)` table value constructor (MariaDB).
    ValuesBare,
    /// `TABLE t` statement.
    TableStatement,
    /// Row alias of `INSERT ... AS new ON DUPLICATE KEY UPDATE`.
    InsertRowAlias,
    /// `INTERSECT` and `EXCEPT`.
    IntersectExcept,
    /// `INTERSECT ALL` and `EXCEPT ALL`.
    IntersectExceptAll,
    /// System-versioned tables (`FOR SYSTEM_TIME`).
    SystemVersioning,
    /// `DELETE ... RETURNING`.
    DeleteReturning,
    /// `INSERT ... RETURNING`.
    InsertReturning,
    /// `OFFSET ... FETCH`.
    OffsetFetch,
    /// `CAST(x AS DOUBLE)` and `CAST(x AS FLOAT)`.
    CastDouble,
    /// `CAST(x AS DOUBLE PRECISION)`, `AS REAL` and `AS FLOAT(p)` (MySQL).
    CastDoubleSynonyms,
    /// `CAST(x AS DOUBLE(m, d))` (MariaDB).
    CastDoubleDigits,
    /// `CAST(x AS INTEGER)` and `CAST(x AS INT)`, without `SIGNED` (MariaDB).
    CastInteger,
    /// `CAST(x AS VARCHAR(n))` (MariaDB).
    CastVarchar,
    /// `COLLATE` in the target of `CAST` (MariaDB).
    CastCollate,
    /// `CAST(x AS NATIONAL CHAR)` (MySQL).
    CastNationalChar,
    /// `CAST(x AS JSON)` (MySQL).
    CastJson,
    /// `CAST(x AS YEAR)` (MySQL).
    CastYear,
    /// `CAST(x AT TIME ZONE 'UTC' AS DATETIME)` (MySQL).
    CastAtTimeZone,
    /// `CAST(x AS POINT)` and the other spatial types (MySQL).
    CastSpatial,
    /// `CAST(x AS INTERVAL DAY_SECOND(n))` (MariaDB).
    CastInterval,
    /// Type `INET6` (MariaDB), the target of `CAST`.
    Inet6Type,
    /// Type `INET4` (MariaDB), the target of `CAST`.
    Inet4Type,
    /// `NOW()`, `CURRENT_TIMESTAMP`, `SYSDATE()` and `FROM_UNIXTIME()` of
    /// type `TIMESTAMP` (MariaDB), a `DATETIME` otherwise.
    TimestampNow,
    /// `LOCALTIME` of type `TIME` (MariaDB), a `DATETIME` otherwise.
    TimeLocaltime,
    /// Type `UUID` (MariaDB), the target of `CAST`; `UUID()` of type `UUID`,
    /// a `VARCHAR(36)` otherwise.
    UuidType,
    /// `LPAD` and `RPAD` with an empty pad, beyond the length of the string:
    /// an empty string (MySQL), NULL otherwise.
    PadEmptyString,
    /// `LPAD` and `RPAD` with an empty pad, beyond the length of the string:
    /// the string unchanged (MySQL), NULL otherwise.
    PadUnchanged,
    /// `CAST` of a negative decimal to `UNSIGNED`: the unsigned integer of
    /// the same bits (MySQL), 0 otherwise.
    CastUnsignedWrap,
    /// Time zone offset in a date and time literal (`TIMESTAMP '2024-05-06
    /// 01:00:00+02:00'`, MySQL), converted to the time zone of the session.
    DatetimeOffset,
    /// `TIME` moved by an interval of days or longer (`t + INTERVAL 1 DAY`),
    /// read on the current date: a `DATETIME` (MySQL), a `TIME` otherwise.
    TimeIntervalDatetime,
    /// `UNIX_TIMESTAMP` and `FROM_UNIXTIME` until 3001-01-18 23:59:59 UTC
    /// (MySQL), 2038-01-19 03:14:07 otherwise.
    #[serde(rename = "unixtime_3001")]
    Unixtime3001,
    /// `UNIX_TIMESTAMP` and `FROM_UNIXTIME` until 2106-02-07 06:28:15 UTC
    /// (MariaDB), 2038-01-19 03:14:07 otherwise.
    #[serde(rename = "unixtime_2106")]
    Unixtime2106,
    /// `UNIX_TIMESTAMP` and `FROM_UNIXTIME` giving NULL at the epoch
    /// (MariaDB), 0 and its date otherwise.
    UnixtimeEpochNull,
    /// `AES_ENCRYPT` and `AES_DECRYPT` with an initialization vector, third
    /// argument, and a mode, fourth argument (MariaDB); two arguments only
    /// otherwise.
    AesIv,
    /// `REPLACE`, `SUBSTRING_INDEX` and `TRIM` in the character set of their
    /// string (MySQL); the character sets of their arguments aggregated
    /// otherwise.
    StringCharset,
    /// `LOCATE`, `INSTR` and `POSITION` in the collation of their string
    /// (MySQL); the collations of their arguments aggregated otherwise.
    LocateCharset,
    /// `WHERE`, `HAVING`, `ON` and the operands of `NOT`, `AND`, `OR`,
    /// `XOR` and `IS TRUE` read a value as compared with 0 (MySQL); before,
    /// a fraction of `WHERE` and `XOR` as an integer, a `JSON` as a number.
    ConditionCompared,
    /// `XOR` reads its operands as compared with 0 (MariaDB); as integers
    /// otherwise.
    XorCompared,
    /// `IN` of a list compares all its values by the rules of JSON when one
    /// is a `JSON` (MySQL); a `JSON` as its text or as a number otherwise.
    JsonInCompared,
    /// `REGEXP` and `REGEXP_LIKE` give an integer (MySQL); a `DOUBLE`
    /// before.
    RegexpInteger,
    /// `REGEXP` reads a binary string byte by byte (MySQL); otherwise
    /// before.
    RegexpBinary,
    /// `REGEXP` refuses a binary string with a character string, error 3995
    /// (MySQL).
    RegexpBinaryError,
    /// `BIT_AND`, `BIT_OR` and `BIT_XOR` as window functions.
    BitWindow,
    /// A hexadecimal literal of more than 8 bytes read as the number 0
    /// (MySQL), 2^64 - 1 before.
    HexLongZero,
    /// A hexadecimal literal as a result of `CASE`, `IF`, `IFNULL`,
    /// `COALESCE` and `NULLIF`, all results being such literals, read as a
    /// number (MariaDB), as a string before.
    HybridResultNumber,
    /// An outer `LIMIT` keeping the `ORDER BY` of a query between
    /// parentheses (MySQL), dropped before.
    ParenthesizedOrderKept,
    /// An outer `LIMIT` keeping the `LIMIT` of a query between parentheses
    /// (MySQL), overridden before.
    ParenthesizedLimitKept,
    /// An outer `OFFSET` counted from the rows of a query between
    /// parentheses bounded by its own `LIMIT` (MariaDB).
    ParenthesizedOffsetKept,
    /// A set operation between parentheses with its own `ORDER BY` or
    /// `LIMIT`, followed by another (MySQL, refused from 8.0.19 to 8.0.30).
    ParenthesizedSetLimit,
    /// An outer `ORDER BY` sorting the rows of a set operation between
    /// parentheses after its own `LIMIT` (MySQL), before it earlier.
    ParenthesizedSetOrder,
    /// The names of an `ORDER BY` after a query between parentheses with its
    /// own `ORDER BY` or `LIMIT` read in the context of that query, a key
    /// other than a name alone or a position read otherwise than the columns
    /// of the result (MariaDB before the fix of MDEV-29681).
    ParenthesizedOrderInner,
    /// A window function read through its alias inside an expression of
    /// `ORDER BY`, computed once (MySQL: computed again for the sort from
    /// 8.0.16 to 8.0.28).
    WindowAliasOnce,
    /// The names of an `ORDER BY` after a `SELECT` between parentheses
    /// without `ORDER BY` or `LIMIT` of its own read as the columns of its
    /// result (MySQL 8.0.31 to 8.0.41, 8.1.0 to 8.4.4, 9.0.0 to 9.2.0); as
    /// those of an `ORDER BY` of that `SELECT` otherwise.
    ParenthesizedOrderResult,
    /// The names of a subquery in that `ORDER BY` read as the columns of the
    /// result (MySQL from 8.0.31).
    ParenthesizedOrderSubquery,
    /// The alias of an aggregate function inside an expression of `ORDER BY`
    /// or of a window, read as the value of the function (MySQL: as one value
    /// for all the rows before 8.0.29; MariaDB refuses it).
    AggregateAliasRead,
    /// A column named alone in a window when an alias of the select list has
    /// its name, read as the column (MySQL from 8.0.14; MariaDB reads the
    /// alias).
    WindowShadowedColumn,
    /// `utf8mb4_uca1400_ai_ci` as the default collation of `utf8mb4`
    /// (MariaDB).
    Utf8mb4Uca1400,
    /// `uca1400_ai_ci` as the default collation of `utf8mb3`, `ucs2`,
    /// `utf16` and `utf32` (MariaDB).
    UnicodeUca1400,
    /// `FOUND_ROWS()` after `SQL_CALC_FOUND_ROWS` on a `UNION ALL` with a
    /// `LIMIT` and without `ORDER BY` counting every row (MySQL), the rows up
    /// to the `LIMIT` before.
    UnionAllFoundRows,
    /// `FOR SHARE` (MySQL), `LOCK IN SHARE MODE` alone otherwise.
    LockShare,
    /// `OF` in a locking clause: the tables it locks (MySQL).
    LockOf,
    /// Several locking clauses after a query, each one on its own tables
    /// (MySQL).
    LockClauses,
    /// `NOWAIT` in a locking clause.
    LockNowait,
    /// `SKIP LOCKED` in a locking clause.
    LockSkipLocked,
    /// `WAIT n` in a locking clause (MariaDB).
    LockWaitSeconds,
    /// `NOWAIT`, `SKIP LOCKED` or `WAIT n` after `LOCK IN SHARE MODE`
    /// (MariaDB).
    ShareModeOptions,
    /// A locking clause right after a query between parentheses: `(SELECT a
    /// FROM t) FOR UPDATE`.
    ParenthesizedLock,
    /// A locking clause after a set operation whose last operand is between
    /// parentheses (MySQL; MariaDB refuses it, error 1221).
    ParenthesizedOperandLock,
    /// A locking clause after a query between parentheses with its own
    /// `LIMIT`, followed by a `LIMIT` or by nothing, locking the rows of that
    /// query (MySQL; none in MariaDB).
    ParenthesizedLimitLock,
    /// A locking clause after a set operation between parentheses with its
    /// own `LIMIT`, followed by a `LIMIT` or by nothing, locking the rows of
    /// its last operand (MySQL, but 8.0.18; none in MariaDB).
    ParenthesizedSetLimitLock,
    /// A locking clause after an `ORDER BY` that follows a query between
    /// parentheses with its own `ORDER BY` or `LIMIT`, locking the rows of
    /// that query (MySQL from 8.0.18; none before and in MariaDB).
    ParenthesizedOrderLock,
    /// A locking clause after an `ORDER BY` that follows a set operation
    /// between parentheses, locking the rows of its last operand (none in
    /// MySQL before 8.0.18).
    ParenthesizedSetOrderLock,
    /// A locking clause at the end of a query between parentheses followed by
    /// a `LIMIT` without `ORDER BY`, locking the rows of that query (dropped
    /// by MySQL before 8.0.19, Bug #30521098).
    ParenthesizedInnerLock,
    /// An outer `LIMIT` without `ORDER BY` bounding a `SELECT` between
    /// parentheses with its own `ORDER BY`, without `LIMIT`, and a locking
    /// clause, inside or after the parentheses (MySQL 8.0.19 to 8.0.25 give
    /// every row).
    ParenthesizedLockedLimit,
    /// The user variable of an empty name, `@''`, `@` alone (MySQL; MariaDB
    /// before 10.7.1).
    EmptyUserVariable,
    /// An assignment of the user variable of an empty name, `@'' := 1`
    /// (MariaDB before 10.7.1).
    EmptyUserVariableAssignment,
    /// The system variables `tx_isolation` and `tx_read_only` (MariaDB).
    TxVariables,
    /// The system variables `transaction_isolation` and
    /// `transaction_read_only` (MySQL; MariaDB from 11.1.1).
    TransactionVariables,
}

/// Construct refused in a dialect or a version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionError {
    /// The dialect has never had the construct.
    Absent {
        /// Construct.
        construct: Construct,
        /// Dialect.
        dialect: Dialect,
    },
    /// The construct appeared after the chosen version.
    Before {
        /// Construct.
        construct: Construct,
        /// Dialect.
        dialect: Dialect,
        /// Version that brought it.
        since: Version,
        /// Version chosen.
        chosen: Version,
    },
    /// The construct was removed in or before the chosen version.
    Removed {
        /// Construct.
        construct: Construct,
        /// Dialect.
        dialect: Dialect,
        /// Version that removed it.
        until: Version,
        /// Version chosen.
        chosen: Version,
    },
}

/// Registry of a dialect: its floor and its constructs, read from its
/// reference file.
#[derive(Debug)]
pub struct Registry {
    /// Dialect.
    dialect: Dialect,
    /// Oldest version covered.
    floor: Version,
    /// Constructs, with their period of validity.
    constructs: &'static [ConstructEntry],
}

/// Construct of a reference file, with its period of validity. A construct
/// that several branches received apart, as a fix of a patch version, has an
/// entry for each period, in order.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ConstructEntry {
    /// Construct.
    id: Construct,
    /// First version that has it.
    #[serde(deserialize_with = "deserialize_version")]
    since: Version,
    /// First version that no longer has it.
    #[serde(default, deserialize_with = "deserialize_optional_version")]
    until: Option<Version>,
    /// Reference manual or release notes that attest it.
    #[allow(dead_code, reason = "read by the people who maintain the file")]
    source: String,
    /// Context: what the construct does, how it was established.
    #[allow(dead_code, reason = "read by the people who maintain the file")]
    #[serde(default)]
    comment: String,
}

impl Version {
    /// Version `major.minor.patch`.
    pub const fn new(major: u16, minor: u16, patch: u16) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    /// Reads the numbers at the start of a version: `8.0.19`, `10.6`,
    /// `10.6.28-MariaDB-ubu2204` (as `SELECT VERSION()` gives it). Missing
    /// numbers count as 0; `None` without a first number.
    pub fn parse(text: &str) -> Option<Self> {
        let mut numbers = text
            .split(|character: char| !character.is_ascii_digit())
            .take(3)
            .take_while(|part| !part.is_empty())
            .map(|part| part.parse::<u16>().ok());
        let major = numbers.next()??;
        let minor = numbers.next().flatten().unwrap_or(0);
        let patch = numbers.next().flatten().unwrap_or(0);
        Some(Self::new(major, minor, patch))
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

impl Construct {
    /// Name of the construct in the messages.
    pub const fn label(self) -> &'static str {
        match self {
            Self::With => "`WITH`",
            Self::WithRecursive => "`WITH RECURSIVE`",
            Self::RecursiveLimit => "`LIMIT` in a recursive common table expression",
            Self::WindowFunctions => "window functions",
            Self::JsonTable => "`JSON_TABLE`",
            Self::JsonArrow => "`->` operator",
            Self::JsonDoubleArrow => "`->>` operator",
            Self::Lateral => "`LATERAL`",
            Self::DerivedColumnList => "column list of a derived table",
            Self::DerivedOuterReference => "outer reference in a derived table",
            Self::NestedSetOperation => "set operation between parentheses as a right operand",
            Self::RollupOrderBy => "`ORDER BY` with `WITH ROLLUP`",
            Self::ExplainFormatTree => "`EXPLAIN FORMAT=TREE`",
            Self::MemberOf => "`MEMBER OF`",
            Self::ExplainAnalyze => "`EXPLAIN ANALYZE`",
            Self::ValuesRow => "`VALUES ROW` statement",
            Self::ValuesBare => "`VALUES` without `ROW`",
            Self::TableStatement => "`TABLE` statement",
            Self::InsertRowAlias => "row alias of `INSERT`",
            Self::IntersectExcept => "`INTERSECT` and `EXCEPT`",
            Self::IntersectExceptAll => "`INTERSECT ALL` and `EXCEPT ALL`",
            Self::SystemVersioning => "`FOR SYSTEM_TIME`",
            Self::DeleteReturning => "`DELETE ... RETURNING`",
            Self::InsertReturning => "`INSERT ... RETURNING`",
            Self::OffsetFetch => "`OFFSET ... FETCH`",
            Self::CastDouble => "`CAST(x AS DOUBLE)` and `CAST(x AS FLOAT)`",
            Self::CastDoubleSynonyms => {
                "`CAST(x AS DOUBLE PRECISION)`, `AS REAL` and `AS FLOAT(p)`"
            }
            Self::CastDoubleDigits => "`CAST(x AS DOUBLE(m, d))`",
            Self::CastInteger => "`CAST(x AS INTEGER)` without `SIGNED`",
            Self::CastVarchar => "`CAST(x AS VARCHAR(n))`",
            Self::CastCollate => "`COLLATE` in the target of `CAST`",
            Self::CastNationalChar => "`CAST(x AS NATIONAL CHAR)`",
            Self::CastJson => "`CAST(x AS JSON)`",
            Self::CastYear => "`CAST(x AS YEAR)`",
            Self::CastAtTimeZone => "`CAST(x AT TIME ZONE ...)`",
            Self::CastSpatial => "`CAST` to a spatial type",
            Self::CastInterval => "`CAST(x AS INTERVAL DAY_SECOND(n))`",
            Self::Inet6Type => "type `INET6`",
            Self::Inet4Type => "type `INET4`",
            Self::TimestampNow => "`NOW()` of type `TIMESTAMP`",
            Self::TimeLocaltime => "`LOCALTIME` of type `TIME`",
            Self::UuidType => "type `UUID`",
            Self::PadEmptyString => "`LPAD` with an empty pad giving an empty string",
            Self::PadUnchanged => "`LPAD` with an empty pad giving the string unchanged",
            Self::CastUnsignedWrap => {
                "`CAST` of a negative decimal to `UNSIGNED` giving the integer of the same bits"
            }
            Self::DatetimeOffset => "time zone offset in a date and time literal",
            Self::TimeIntervalDatetime => "`TIME` moved by an interval of days giving a `DATETIME`",
            Self::Unixtime3001 => "`UNIX_TIMESTAMP` until 3001",
            Self::Unixtime2106 => "`UNIX_TIMESTAMP` until 2106",
            Self::UnixtimeEpochNull => "`UNIX_TIMESTAMP` giving NULL at the epoch",
            Self::AesIv => "`AES_ENCRYPT` with an initialization vector and a mode",
            Self::StringCharset => "`REPLACE` in the character set of its string",
            Self::LocateCharset => "`LOCATE` in the collation of its string",
            Self::ConditionCompared => "conditions read as compared with 0",
            Self::XorCompared => "`XOR` of operands read as compared with 0",
            Self::JsonInCompared => "`IN` comparing a `JSON` by the rules of JSON",
            Self::RegexpInteger => "`REGEXP` giving an integer",
            Self::RegexpBinary => "`REGEXP` reading a binary string byte by byte",
            Self::RegexpBinaryError => "`REGEXP` refusing a binary string with a character string",
            Self::BitWindow => "`BIT_AND`, `BIT_OR` and `BIT_XOR` as window functions",
            Self::HexLongZero => "a hexadecimal literal of more than 8 bytes read as 0",
            Self::HybridResultNumber => {
                "a hexadecimal literal as a result of a conditional function read as a number"
            }
            Self::ParenthesizedOrderKept => {
                "an outer `LIMIT` keeping the `ORDER BY` of a query between parentheses"
            }
            Self::ParenthesizedLimitKept => {
                "an outer `LIMIT` keeping the `LIMIT` of a query between parentheses"
            }
            Self::ParenthesizedOffsetKept => {
                "an outer `OFFSET` counted from the rows of a query between parentheses"
            }
            Self::ParenthesizedSetLimit => {
                "a set operation between parentheses with its own `ORDER BY` or `LIMIT`, followed by another"
            }
            Self::ParenthesizedSetOrder => {
                "an outer `ORDER BY` after the `LIMIT` of a set operation between parentheses"
            }
            Self::ParenthesizedOrderInner => {
                "the names of an `ORDER BY` after a query between parentheses with its own `ORDER BY` or `LIMIT` read in the context of that query"
            }
            Self::WindowAliasOnce => {
                "a window function read once through its alias inside an expression of `ORDER BY`"
            }
            Self::ParenthesizedOrderResult => {
                "the names of an `ORDER BY` after a `SELECT` between parentheses read as the columns of its result"
            }
            Self::ParenthesizedOrderSubquery => {
                "the names of a subquery in an `ORDER BY` after a `SELECT` between parentheses read as the columns of its result"
            }
            Self::AggregateAliasRead => {
                "the alias of an aggregate function read inside an expression of `ORDER BY`"
            }
            Self::WindowShadowedColumn => {
                "a column named alone in a window when an alias of the select list has its name"
            }
            Self::Utf8mb4Uca1400 => "`utf8mb4_uca1400_ai_ci` as the default collation of `utf8mb4`",
            Self::UnicodeUca1400 => {
                "`uca1400_ai_ci` as the default collation of `utf8mb3`, `ucs2`, `utf16` and `utf32`"
            }
            Self::UnionAllFoundRows => {
                "`FOUND_ROWS()` counting every row of a `UNION ALL` beyond its `LIMIT`"
            }
            Self::LockShare => "`FOR SHARE`",
            Self::LockOf => "`OF` in a locking clause",
            Self::LockClauses => "several locking clauses",
            Self::LockNowait => "`NOWAIT`",
            Self::LockSkipLocked => "`SKIP LOCKED`",
            Self::LockWaitSeconds => "`WAIT n` in a locking clause",
            Self::ShareModeOptions => {
                "`NOWAIT`, `SKIP LOCKED` or `WAIT` after `LOCK IN SHARE MODE`"
            }
            Self::ParenthesizedLock => "a locking clause right after a query between parentheses",
            Self::ParenthesizedOperandLock => {
                "a locking clause after a set operation whose last operand is between parentheses"
            }
            Self::ParenthesizedLimitLock => {
                "a locking clause after a query between parentheses with its own `LIMIT`, \
                 followed by a `LIMIT` or by nothing, locking the rows of that query"
            }
            Self::ParenthesizedSetLimitLock => {
                "a locking clause after a set operation between parentheses with its own \
                 `LIMIT`, followed by a `LIMIT` or by nothing, locking the rows of its last \
                 operand"
            }
            Self::ParenthesizedOrderLock => {
                "a locking clause after an `ORDER BY` that follows a query between parentheses \
                 with its own `ORDER BY` or `LIMIT`, locking the rows of that query"
            }
            Self::ParenthesizedSetOrderLock => {
                "a locking clause after an `ORDER BY` that follows a set operation between \
                 parentheses, locking the rows of its last operand"
            }
            Self::ParenthesizedInnerLock => {
                "a locking clause at the end of a query between parentheses followed by a \
                 `LIMIT` without `ORDER BY`, locking the rows of that query"
            }
            Self::ParenthesizedLockedLimit => {
                "an outer `LIMIT` bounding a locked `SELECT` between parentheses with its own \
                 `ORDER BY`"
            }
            Self::EmptyUserVariable => "the user variable of an empty name",
            Self::EmptyUserVariableAssignment => {
                "an assignment of the user variable of an empty name"
            }
            Self::TxVariables => "the system variables `tx_isolation` and `tx_read_only`",
            Self::TransactionVariables => {
                "the system variables `transaction_isolation` and `transaction_read_only`"
            }
        }
    }
}

impl fmt::Display for VersionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Absent { construct, dialect } => {
                write!(f, "{}: absent from {}", construct.label(), dialect.label())
            }
            Self::Before {
                construct,
                dialect,
                since,
                chosen,
            } => write!(
                f,
                "{}: from {} {since}, chosen version {chosen}",
                construct.label(),
                dialect.label()
            ),
            Self::Removed {
                construct,
                dialect,
                until,
                chosen,
            } => write!(
                f,
                "{}: removed in {} {until}, chosen version {chosen}",
                construct.label(),
                dialect.label()
            ),
        }
    }
}

impl std::error::Error for VersionError {}

impl Registry {
    /// Oldest version covered.
    pub fn floor(&self) -> Version {
        self.floor
    }

    /// Checks that `construct` exists in the `version` of `dialect`.
    ///
    /// # Errors
    ///
    /// `VersionError` when the dialect never had the construct, or not in
    /// this version: before the next period, or after the last one.
    pub fn check(
        &self,
        dialect: Dialect,
        construct: Construct,
        version: Version,
    ) -> Result<(), VersionError> {
        let mut removed = None;
        for entry in self.constructs.iter().filter(|entry| entry.id == construct) {
            if version < entry.since {
                return Err(VersionError::Before {
                    construct,
                    dialect,
                    since: entry.since,
                    chosen: version,
                });
            }
            match entry.until {
                Some(until) if version >= until => removed = Some(until),
                _ => return Ok(()),
            }
        }
        match removed {
            Some(until) => Err(VersionError::Removed {
                construct,
                dialect,
                until,
                chosen: version,
            }),
            None => Err(VersionError::Absent { construct, dialect }),
        }
    }

    /// Versions offered to the user: the floor, then each version that adds
    /// or removes a construct or a built-in function, in order.
    pub fn offered_versions(&self) -> Vec<Version> {
        let changes = self
            .constructs
            .iter()
            .flat_map(|entry| std::iter::once(entry.since).chain(entry.until))
            .chain(builtins::changes(self.dialect));
        let mut versions: Vec<Version> = std::iter::once(self.floor)
            .chain(changes.filter(|&version| version > self.floor))
            .collect();
        versions.sort_unstable();
        versions.dedup();
        versions
    }
}

/// Registry of a dialect, built only once.
pub fn registry(dialect: Dialect) -> &'static Registry {
    static MYSQL: OnceLock<Registry> = OnceLock::new();
    static MARIADB: OnceLock<Registry> = OnceLock::new();
    let build = || {
        let reference = reference(dialect);
        Registry {
            dialect,
            floor: reference.floor,
            constructs: &reference.constructs,
        }
    };
    match dialect {
        Dialect::MySql => MYSQL.get_or_init(build),
        Dialect::MariaDb => MARIADB.get_or_init(build),
    }
}

/// Reads a version written as a string in a reference file.
pub(crate) fn deserialize_version<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Version, D::Error> {
    let text = String::deserialize(deserializer)?;
    Version::parse(&text)
        .ok_or_else(|| serde::de::Error::custom(format!("invalid version {text:?}")))
}

/// Reads an optional version of a reference file.
pub(crate) fn deserialize_optional_version<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Version>, D::Error> {
    deserialize_version(deserializer).map(Some)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    #[test]
    fn version_reads_the_numbers_of_the_server() {
        assert_eq!(Version::parse("8.0.19"), Some(Version::new(8, 0, 19)));
        assert_eq!(Version::parse("10.6"), Some(Version::new(10, 6, 0)));
        assert_eq!(
            Version::parse("10.6.28-MariaDB-ubu2204"),
            Some(Version::new(10, 6, 28))
        );
        assert_eq!(Version::parse("MariaDB"), None);
        assert!(Version::new(8, 0, 19) < Version::new(8, 0, 31));
        assert!(Version::new(8, 4, 0) > Version::new(8, 0, 31));
    }

    #[test]
    fn registries_are_readable_with_periods_in_order() {
        for dialect in [Dialect::MySql, Dialect::MariaDb] {
            let registry = registry(dialect);
            // End of the last period of each construct seen.
            let mut seen: HashMap<Construct, Option<Version>> = HashMap::new();
            for entry in registry.constructs {
                if let Some(end) = seen.get(&entry.id) {
                    assert!(
                        end.is_some_and(|until| until < entry.since),
                        "{:?}: periods overlap in {dialect:?}",
                        entry.id
                    );
                }
                seen.insert(entry.id, entry.until);
                assert!(entry.until.is_none_or(|until| until > entry.since));
                assert!(entry.source.starts_with("https://"));
            }
        }
    }

    #[test]
    fn a_construct_of_several_periods_is_checked_in_each() {
        let entry = |since: Version, until: Option<Version>| ConstructEntry {
            id: Construct::ValuesBare,
            since,
            until,
            source: String::new(),
            comment: String::new(),
        };
        let constructs = Box::leak(Box::new([
            entry(Version::new(10, 11, 19), Some(Version::new(11, 0, 0))),
            entry(Version::new(11, 4, 13), None),
        ]));
        let registry = Registry {
            dialect: Dialect::MariaDb,
            floor: Version::new(10, 6, 3),
            constructs,
        };
        let check = |major, minor, patch| {
            registry
                .check(
                    Dialect::MariaDb,
                    Construct::ValuesBare,
                    Version::new(major, minor, patch),
                )
                .map_err(|error| error.to_string())
        };
        assert_eq!(check(10, 11, 19), Ok(()));
        assert_eq!(check(11, 8, 0), Ok(()));
        assert_eq!(
            check(10, 11, 18),
            Err(
                "`VALUES` without `ROW`: from MariaDB 10.11.19, chosen version 10.11.18"
                    .to_string()
            )
        );
        assert_eq!(
            check(11, 2, 6),
            Err("`VALUES` without `ROW`: from MariaDB 11.4.13, chosen version 11.2.6".to_string())
        );
        assert!(
            registry
                .offered_versions()
                .contains(&Version::new(11, 0, 0))
        );
    }

    #[test]
    fn values_row_comes_with_mysql_8_0_19() {
        let mysql = registry(Dialect::MySql);
        assert_eq!(
            mysql.check(Dialect::MySql, Construct::ValuesRow, Version::new(8, 0, 19)),
            Ok(())
        );
        let refused = mysql.check(Dialect::MySql, Construct::ValuesRow, Version::new(8, 0, 18));
        assert_eq!(
            refused.unwrap_err().to_string(),
            "`VALUES ROW` statement: from MySQL 8.0.19, chosen version 8.0.18"
        );
    }

    #[test]
    fn each_dialect_stays_in_its_domain() {
        // The example of the dialects: `VALUES ROW` is MySQL, `VALUES` alone is MariaDB.
        let mariadb = registry(Dialect::MariaDb);
        let latest = Version::new(11, 8, 0);
        assert_eq!(
            mariadb
                .check(Dialect::MariaDb, Construct::ValuesRow, latest)
                .unwrap_err()
                .to_string(),
            "`VALUES ROW` statement: absent from MariaDB"
        );
        assert_eq!(
            mariadb.check(Dialect::MariaDb, Construct::ValuesBare, latest),
            Ok(())
        );
        assert!(
            registry(Dialect::MySql)
                .check(Dialect::MySql, Construct::ValuesBare, latest)
                .is_err()
        );
    }

    #[test]
    fn offered_versions_start_at_the_floor_and_follow_the_changes() {
        let mysql = registry(Dialect::MySql).offered_versions();
        assert_eq!(mysql.first(), Some(&Version::new(8, 0, 11)));
        assert!(mysql.contains(&Version::new(8, 0, 19)));
        assert!(mysql.contains(&Version::new(8, 0, 31)));
        // Built-in functions: `FORMAT_PICO_TIME` appears, `WAIT_UNTIL_SQL_THREAD_AFTER_GTIDS` goes.
        assert!(mysql.contains(&Version::new(8, 0, 16)));
        assert!(mysql.contains(&Version::new(8, 2, 0)));
        // Constructs older than the floor give no version of their own.
        assert!(!mysql.contains(&Version::new(8, 0, 1)));
        assert!(mysql.windows(2).all(|pair| pair[0] < pair[1]));
        let mariadb = registry(Dialect::MariaDb).offered_versions();
        assert_eq!(mariadb.first(), Some(&Version::new(10, 6, 3)));
        assert!(mariadb.contains(&Version::new(10, 7, 0)));
    }
}
