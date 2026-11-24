//! Catalog of a source, whatever the engine: its namespaces (see
//! `api::Namespace`), the objects of each one, and the details of their
//! tables. The object navigator shows them; each connector reads them from
//! its source (`mysql::catalog` for MySQL and MariaDB).

use serde::Serialize;
use ts_rs::TS;

use crate::api::Namespace;

/// Namespace of a source, as the catalog lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct NamespaceInfo {
    /// Path of the namespace.
    pub namespace: Namespace,
    /// Is it provided by the source itself (`information_schema`, `mysql`,
    /// `performance_schema`, `sys` for MySQL)? The navigator reads it last,
    /// and shows it only when the connection asks for it.
    pub system: bool,
}

/// Object of a namespace: table, view, routine, event...
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct ObjectInfo {
    /// Name of the object.
    pub name: String,
    /// Kind of the object.
    pub kind: ObjectKind,
    /// Row count estimated by the source, for a table; `None` for another
    /// object, or a table without statistics.
    pub rows: Option<u64>,
    /// Is a scheduled object running on its schedule (`ENABLED` event), or
    /// paused? `None` for an object without a schedule.
    pub enabled: Option<bool>,
}

/// Kind of an object of a namespace. Kinds are only added, as the engines
/// come (materialized view, sequence, macro...).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum ObjectKind {
    /// Data table.
    Table,
    /// View.
    View,
    /// Table or view provided by the source itself (`information_schema`...).
    SystemTable,
    /// Stored procedure (`CALL`).
    Procedure,
    /// Stored function (called in an expression).
    Function,
    /// Scheduled event (`CREATE EVENT`).
    Event,
}

/// Column of a table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct ColumnInfo {
    /// Name of the column.
    pub name: String,
    /// Full type, as declared (`varchar(50)`, `int unsigned`...).
    pub column_type: String,
    /// Does the column accept NULL?
    pub nullable: bool,
    /// Role in the indexes: `PRI`, `UNI`, `MUL`, or empty string.
    pub key: String,
    /// Value assigned automatically on insert (`AUTO_INCREMENT`)?
    pub auto_increment: bool,
}

/// Index of a table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct IndexInfo {
    /// Name of the index (`PRIMARY` for the primary key).
    pub name: String,
    /// Indexed columns, in the order of the index. A computed part (MySQL
    /// functional index) appears as `(expression)`.
    pub columns: Vec<String>,
    /// Unique index?
    pub unique: bool,
}

/// Foreign key of a table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct ForeignKeyInfo {
    /// Name of the constraint.
    pub name: String,
    /// Columns of the table, in the order of the key.
    pub columns: Vec<String>,
    /// Database of the referenced table.
    pub referenced_schema: String,
    /// Referenced table.
    pub referenced_table: String,
    /// Referenced columns, in the same order as `columns`.
    pub referenced_columns: Vec<String>,
}

/// Trigger of a table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct TriggerInfo {
    /// Name of the trigger.
    pub name: String,
    /// Timing: `BEFORE` or `AFTER`.
    pub timing: String,
    /// Event: `INSERT`, `UPDATE` or `DELETE`.
    pub event: String,
}

/// Details of a table: columns, indexes, foreign keys and triggers.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct TableDetails {
    /// Name of the table.
    pub table: String,
    /// Columns, in declaration order.
    pub columns: Vec<ColumnInfo>,
    /// Indexes, the primary key first, then by name.
    pub indexes: Vec<IndexInfo>,
    /// Foreign keys, by name.
    pub foreign_keys: Vec<ForeignKeyInfo>,
    /// Triggers, by timing, event and execution order.
    pub triggers: Vec<TriggerInfo>,
}
