//! Catalog of a MySQL or MariaDB server: databases, objects and table details.
//!
//! Serves the object explorer, which shows the elements as soon as they
//! arrive, in three steps:
//!
//! 1. `schemas`: database names;
//! 2. `objects`: tables, views, procedures and functions of a database;
//! 3. `details`: columns, indexes, foreign keys and triggers of each table of
//!    a database.
//!
//! Each read filters on a database, which saves the server from walking all
//! the tables at once. The queries only ask for dictionary information:
//! asking for the row count as well (`TABLE_ROWS`) would force the server to
//! open each table, which is very slow on a large server. Database names are
//! passed as parameters of prepared statements, never concatenated to the
//! SQL text.

use std::collections::BTreeMap;

use mysql_async::prelude::Queryable;
use serde::Serialize;
use ts_rs::TS;

use super::{Connection, MysqlError};

/// Table or view of a database.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct TableInfo {
    /// Name of the table.
    pub name: String,
    /// Nature of the object.
    pub kind: TableKind,
}

/// Nature of a table-like object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum TableKind {
    /// Data table.
    Table,
    /// View.
    View,
    /// Table or view provided by the server itself (`information_schema`...).
    System,
}

/// Stored procedure or function.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct RoutineInfo {
    /// Name of the routine.
    pub name: String,
    /// Procedure or function.
    pub kind: RoutineKind,
}

/// Nature of a stored routine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum RoutineKind {
    /// Procedure (`CALL`).
    Procedure,
    /// Function (called in an expression).
    Function,
}

/// Objects of a database: tables and views, procedures and functions, by name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct SchemaObjects {
    /// Tables and views.
    pub tables: Vec<TableInfo>,
    /// Procedures and functions.
    pub routines: Vec<RoutineInfo>,
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

/// Row of `information_schema.COLUMNS`: table, column, type, nullability,
/// index, extra information (`auto_increment`...).
type ColumnRow = (String, String, String, String, String, String);

/// Row of `information_schema.STATISTICS`: table, index, non unique, column.
type IndexRow = (String, String, i64, Option<String>);

/// Row of `information_schema.KEY_COLUMN_USAGE`: table, constraint, column,
/// referenced database, table and column.
type ForeignKeyRow = (String, String, String, String, String, String);

/// Row of `information_schema.TRIGGERS`: table, trigger, timing, event.
type TriggerRow = (String, String, String, String);

/// Lists the server's databases, by name.
pub async fn schemas(connection: &mut Connection) -> Result<Vec<String>, MysqlError> {
    Ok(connection
        .query("SELECT SCHEMA_NAME FROM information_schema.SCHEMATA ORDER BY SCHEMA_NAME")
        .await?)
}

/// Lists the objects of the database `schema`: tables, views, procedures and functions.
pub async fn objects(
    connection: &mut Connection,
    schema: &str,
) -> Result<SchemaObjects, MysqlError> {
    let tables: Vec<(String, String)> = connection
        .exec(
            "SELECT TABLE_NAME, TABLE_TYPE FROM information_schema.TABLES
             WHERE TABLE_SCHEMA = ? ORDER BY TABLE_NAME",
            (schema,),
        )
        .await?;
    let routines: Vec<(String, String)> = connection
        .exec(
            "SELECT ROUTINE_NAME, ROUTINE_TYPE FROM information_schema.ROUTINES
             WHERE ROUTINE_SCHEMA = ? ORDER BY ROUTINE_NAME",
            (schema,),
        )
        .await?;
    Ok(SchemaObjects {
        tables: tables
            .into_iter()
            .map(|(name, table_type)| TableInfo {
                name,
                kind: table_kind(&table_type),
            })
            .collect(),
        routines: routines
            .into_iter()
            .map(|(name, routine_type)| RoutineInfo {
                name,
                kind: if routine_type == "FUNCTION" {
                    RoutineKind::Function
                } else {
                    RoutineKind::Procedure
                },
            })
            .collect(),
    })
}

/// Reads the details of all the tables of the database `schema`, table by table.
pub async fn details(
    connection: &mut Connection,
    schema: &str,
) -> Result<Vec<TableDetails>, MysqlError> {
    let columns: Vec<ColumnRow> = connection
        .exec(
            "SELECT TABLE_NAME, COLUMN_NAME, COLUMN_TYPE, IS_NULLABLE, COLUMN_KEY, EXTRA
             FROM information_schema.COLUMNS
             WHERE TABLE_SCHEMA = ? ORDER BY TABLE_NAME, ORDINAL_POSITION",
            (schema,),
        )
        .await?;
    let indexes: Vec<IndexRow> = connection
        .exec(
            "SELECT TABLE_NAME, INDEX_NAME, NON_UNIQUE, COLUMN_NAME
             FROM information_schema.STATISTICS
             WHERE TABLE_SCHEMA = ? ORDER BY TABLE_NAME, INDEX_NAME, SEQ_IN_INDEX",
            (schema,),
        )
        .await?;
    let foreign_keys: Vec<ForeignKeyRow> = connection
        .exec(
            "SELECT TABLE_NAME, CONSTRAINT_NAME, COLUMN_NAME,
                    REFERENCED_TABLE_SCHEMA, REFERENCED_TABLE_NAME, REFERENCED_COLUMN_NAME
             FROM information_schema.KEY_COLUMN_USAGE
             WHERE TABLE_SCHEMA = ? AND REFERENCED_TABLE_NAME IS NOT NULL
             ORDER BY TABLE_NAME, CONSTRAINT_NAME, ORDINAL_POSITION",
            (schema,),
        )
        .await?;
    let triggers: Vec<TriggerRow> = connection
        .exec(
            "SELECT EVENT_OBJECT_TABLE, TRIGGER_NAME, ACTION_TIMING, EVENT_MANIPULATION
             FROM information_schema.TRIGGERS
             WHERE TRIGGER_SCHEMA = ?
             ORDER BY EVENT_OBJECT_TABLE, ACTION_TIMING, EVENT_MANIPULATION, ACTION_ORDER",
            (schema,),
        )
        .await?;
    Ok(assemble_details(columns, indexes, foreign_keys, triggers))
}

/// Sorts the rows read from `information_schema` table by table.
///
/// Rows arrive sorted by table then in the useful order (position of the
/// columns, order of the columns in the index or the key). The result is
/// sorted by table name.
fn assemble_details(
    columns: Vec<ColumnRow>,
    indexes: Vec<IndexRow>,
    foreign_keys: Vec<ForeignKeyRow>,
    triggers: Vec<TriggerRow>,
) -> Vec<TableDetails> {
    let mut tables: BTreeMap<String, TableDetails> = BTreeMap::new();
    for (name, column, column_type, is_nullable, key, extra) in columns {
        table(&mut tables, &name).columns.push(ColumnInfo {
            name: column,
            column_type,
            nullable: is_nullable == "YES",
            key,
            // EXTRA can combine several mentions ("auto_increment", "on update...").
            auto_increment: extra.to_lowercase().contains("auto_increment"),
        });
    }
    for (name, index, non_unique, column) in indexes {
        let column = column.unwrap_or_else(|| "(expression)".to_string());
        let indexes = &mut table(&mut tables, &name).indexes;
        match indexes.last_mut() {
            Some(last) if last.name == index => last.columns.push(column),
            _ => indexes.push(IndexInfo {
                name: index,
                columns: vec![column],
                unique: non_unique == 0,
            }),
        }
    }
    for (name, constraint, column, referenced_schema, referenced_table, referenced_column) in
        foreign_keys
    {
        let keys = &mut table(&mut tables, &name).foreign_keys;
        match keys.last_mut() {
            Some(last) if last.name == constraint => {
                last.columns.push(column);
                last.referenced_columns.push(referenced_column);
            }
            _ => keys.push(ForeignKeyInfo {
                name: constraint,
                columns: vec![column],
                referenced_schema,
                referenced_table,
                referenced_columns: vec![referenced_column],
            }),
        }
    }
    for (name, trigger, timing, event) in triggers {
        table(&mut tables, &name).triggers.push(TriggerInfo {
            name: trigger,
            timing,
            event,
        });
    }

    let mut tables: Vec<TableDetails> = tables.into_values().collect();
    for details in &mut tables {
        // Primary key first, as in SHOW INDEX; the sort is stable.
        details.indexes.sort_by_key(|index| index.name != "PRIMARY");
    }
    tables
}

/// Returns the details of the table `name`, created empty if they do not exist yet.
fn table<'a>(tables: &'a mut BTreeMap<String, TableDetails>, name: &str) -> &'a mut TableDetails {
    tables
        .entry(name.to_string())
        .or_insert_with(|| TableDetails {
            table: name.to_string(),
            ..TableDetails::default()
        })
}

/// Translates the `TABLE_TYPE` column of `information_schema.TABLES`.
fn table_kind(table_type: &str) -> TableKind {
    match table_type {
        "BASE TABLE" => TableKind::Table,
        "VIEW" => TableKind::View,
        // SYSTEM VIEW (MySQL, MariaDB), SYSTEM VERSIONED (MariaDB, versioned table)...
        "SYSTEM VERSIONED" => TableKind::Table,
        _ => TableKind::System,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(value: &str) -> String {
        value.to_string()
    }

    #[test]
    fn assembles_details_table_by_table() {
        let columns = vec![
            (
                text("t"),
                text("id"),
                text("int"),
                text("NO"),
                text("PRI"),
                text("auto_increment"),
            ),
            (
                text("t"),
                text("parent"),
                text("int"),
                text("YES"),
                text("MUL"),
                text(""),
            ),
            (
                text("u"),
                text("id"),
                text("int"),
                text("NO"),
                text("PRI"),
                text(""),
            ),
        ];
        let indexes = vec![
            (text("t"), text("idx_parent"), 1, Some(text("parent"))),
            (text("t"), text("PRIMARY"), 0, Some(text("id"))),
            (text("t"), text("uq_pair"), 0, Some(text("id"))),
            (text("t"), text("uq_pair"), 0, Some(text("parent"))),
            (text("u"), text("idx_expr"), 1, None),
        ];
        let foreign_keys = vec![(
            text("t"),
            text("fk_parent"),
            text("parent"),
            text("shop"),
            text("u"),
            text("id"),
        )];
        let triggers = vec![(text("t"), text("t_bi"), text("BEFORE"), text("INSERT"))];

        let tables = assemble_details(columns, indexes, foreign_keys, triggers);

        assert_eq!(tables.len(), 2);
        let t = &tables[0];
        assert_eq!(t.table, "t");
        assert_eq!(t.columns.len(), 2);
        assert!(t.columns[0].auto_increment);
        assert!(!t.columns[1].auto_increment);
        let names: Vec<_> = t.indexes.iter().map(|index| index.name.as_str()).collect();
        assert_eq!(names, ["PRIMARY", "idx_parent", "uq_pair"]);
        assert_eq!(t.indexes[2].columns, ["id", "parent"]);
        assert!(t.indexes[2].unique);
        assert!(!t.indexes[1].unique);
        assert_eq!(t.foreign_keys[0].referenced_table, "u");
        assert_eq!(t.triggers[0].name, "t_bi");
        assert_eq!(tables[1].indexes[0].columns, ["(expression)"]);
    }

    #[test]
    fn translates_table_types() {
        assert_eq!(table_kind("BASE TABLE"), TableKind::Table);
        assert_eq!(table_kind("SYSTEM VERSIONED"), TableKind::Table);
        assert_eq!(table_kind("VIEW"), TableKind::View);
        assert_eq!(table_kind("SYSTEM VIEW"), TableKind::System);
    }
}
