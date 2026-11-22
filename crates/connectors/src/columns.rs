//! Columns and cells of a result, whatever the engine: the family and the
//! native name of a column's type, the column of a table it reads, and the
//! text of its cells. Each connector describes its columns in these terms
//! (`mysql::column_type` for MySQL and MariaDB).

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Value of a result cell: `None` for NULL, otherwise text.
///
/// Everything is sent as text, numbers included: no loss of precision on a
/// `BIGINT UNSIGNED` or a `DECIMAL`; numbers are written in plain decimal
/// (`number_text`). Fine typing will come with the `types` crate.
pub type Cell = Option<String>;

/// Family of a column type, for the icon of the grid's header.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum TypeFamily {
    /// `TINYINT` to `BIGINT`.
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
    /// `GEOMETRY` and its derivatives.
    Geometry,
    /// Other type, or `NULL` (column of a `NULL` expression).
    Other,
}

/// Type of a result column: its family and its native name, and the column
/// of a table it reads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ColumnTypeInfo {
    /// Family of the type.
    pub family: TypeFamily,
    /// Name of the type in the server's dialect: `INT UNSIGNED`,
    /// `DECIMAL(10,2)`, `VARCHAR(20)`, `DATETIME(6)`...
    pub name: String,
    /// Column of a table the result column reads; absent for an expression, a
    /// `UNION`, and a result saved before the origin was recorded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub origin: Option<ColumnOrigin>,
}

/// Column of a table a result column reads, as the metadata of the result
/// tell it (`docs/SQL-COMPARISON.md`, section 3.15).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ColumnOrigin {
    /// Database of the table.
    pub schema: String,
    /// Table. A view gives its own name, and MariaDB the alias of a derived
    /// table.
    pub table: String,
    /// Column of the table.
    pub column: String,
    /// Does the column belong to the primary key of its table? The values of
    /// the result can still repeat, through a join.
    pub primary_key: bool,
}
