//! Catalog of a MySQL or MariaDB server: databases, objects and table details.
//!
//! Serves the object navigator through `MysqlCatalog`, the reader of the
//! contract (`crate::api::CatalogReader`), which shows the elements as soon
//! as they arrive, in three steps:
//!
//! 1. `schemas`: database names;
//! 2. `objects`: tables, views, procedures, functions and events of a database,
//!    in their neutral form (`crate::catalog::ObjectInfo`);
//! 3. `details`: columns, indexes, foreign keys and triggers of each table of
//!    a database.
//!
//! Each element carries the comment the server keeps for it (`COMMENT` of a
//! table, a column, an index, a routine or an event; of a database in
//! MariaDB, which MySQL does not have); a trigger has none. A view has no
//! comment either: `TABLE_COMMENT` reads `VIEW` there.
//!
//! Each read filters on a database, which saves the server from walking all
//! the tables at once. The objects carry the row count estimated by the
//! server (`TABLE_ROWS`): fast while the tables are warm, it costs seconds on
//! a server whose tables are cold, which MariaDB then opens one by one; this
//! cost is accepted (`docs/IDEAS-VIZ.md`, section 9.2). Database names are
//! passed as parameters of prepared statements, never concatenated to the
//! SQL text.

use std::collections::BTreeMap;

use futures_util::future::BoxFuture;
use mysql_async::prelude::{FromRow, Queryable};

use super::{Connection, MysqlError};
use crate::api::{CatalogReader, ConnectorError, Namespace};
use crate::catalog::{
    ColumnInfo, ForeignKeyInfo, IndexInfo, NamespaceInfo, ObjectInfo, ObjectKind, TableDetails,
    TriggerInfo,
};
use crate::sql_log;

/// Row of `information_schema.COLUMNS`: table, column, type, nullability,
/// index, extra information (`auto_increment`...), comment.
type ColumnRow = (String, String, String, String, String, String, String);

/// Row of `information_schema.STATISTICS`: table, index, non unique, column,
/// comment of the index.
type IndexRow = (String, String, i64, Option<String>, String);

/// Row of `information_schema.KEY_COLUMN_USAGE`: table, constraint, column,
/// referenced database, table and column.
type ForeignKeyRow = (String, String, String, String, String, String);

/// Row of `information_schema.TRIGGERS`: table, trigger, timing, event.
type TriggerRow = (String, String, String, String);

/// Databases provided by the server itself.
const SYSTEM_SCHEMAS: [&str; 4] = ["information_schema", "mysql", "performance_schema", "sys"];

/// Reader of the catalog of a MySQL or MariaDB server, on the connection of
/// a session. A namespace is a database.
pub struct MysqlCatalog<'a> {
    /// Connection of the session.
    connection: &'a mut Connection,
    /// Is the server a MariaDB, whose databases have a comment?
    mariadb: bool,
}

impl<'a> MysqlCatalog<'a> {
    /// Reader on the connection of a session, which it borrows, to a MariaDB
    /// server or not.
    pub fn new(connection: &'a mut Connection, mariadb: bool) -> Self {
        Self {
            connection,
            mariadb,
        }
    }
}

impl CatalogReader for MysqlCatalog<'_> {
    fn namespaces(&mut self) -> BoxFuture<'_, Result<Vec<NamespaceInfo>, ConnectorError>> {
        Box::pin(async move {
            let schemas = commented_schemas(self.connection, self.mariadb).await?;
            Ok(schemas
                .into_iter()
                .map(|(schema, comment)| NamespaceInfo {
                    system: SYSTEM_SCHEMAS.contains(&schema.as_str()),
                    namespace: Namespace::single(schema),
                    comment,
                })
                .collect())
        })
    }

    fn objects<'a>(
        &'a mut self,
        namespace: &'a Namespace,
    ) -> BoxFuture<'a, Result<Vec<ObjectInfo>, ConnectorError>> {
        Box::pin(async move { Ok(objects(self.connection, namespace.name()).await?) })
    }

    fn details<'a>(
        &'a mut self,
        namespace: &'a Namespace,
    ) -> BoxFuture<'a, Result<Vec<TableDetails>, ConnectorError>> {
        Box::pin(async move { Ok(details(self.connection, namespace.name()).await?) })
    }
}

/// Lists the server's databases, by name, with their comment: that of
/// `SCHEMA_COMMENT` in MariaDB, empty in MySQL, which has none.
pub async fn commented_schemas(
    connection: &mut Connection,
    mariadb: bool,
) -> Result<Vec<(String, String)>, MysqlError> {
    let sql = if mariadb {
        "SELECT SCHEMA_NAME, SCHEMA_COMMENT FROM information_schema.SCHEMATA ORDER BY SCHEMA_NAME"
    } else {
        "SELECT SCHEMA_NAME, '' FROM information_schema.SCHEMATA ORDER BY SCHEMA_NAME"
    };
    Ok(sql_log::traced(sql, connection.query(sql), |rows| Some(rows.len() as u64)).await?)
}

/// Runs a catalog query on the database `schema`, its only parameter, and
/// traces it.
async fn rows_of<T: FromRow + Send + 'static>(
    connection: &mut Connection,
    sql: &str,
    schema: &str,
) -> Result<Vec<T>, MysqlError> {
    let text = sql_log::bound(sql, &[schema]);
    Ok(
        sql_log::traced(&text, connection.exec(sql, (schema,)), |rows| {
            Some(rows.len() as u64)
        })
        .await?,
    )
}

/// Lists the objects of the database `schema`, in the order of the
/// navigator: tables and views, procedures and functions, then events, each
/// group by name.
pub async fn objects(
    connection: &mut Connection,
    schema: &str,
) -> Result<Vec<ObjectInfo>, MysqlError> {
    let tables: Vec<(String, String, Option<u64>, String)> = rows_of(
        connection,
        "SELECT TABLE_NAME, TABLE_TYPE, TABLE_ROWS, TABLE_COMMENT FROM information_schema.TABLES
             WHERE TABLE_SCHEMA = ? ORDER BY TABLE_NAME",
        schema,
    )
    .await?;
    let routines: Vec<(String, String, String)> = rows_of(
        connection,
        "SELECT ROUTINE_NAME, ROUTINE_TYPE, ROUTINE_COMMENT FROM information_schema.ROUTINES
             WHERE ROUTINE_SCHEMA = ? ORDER BY ROUTINE_NAME",
        schema,
    )
    .await?;
    let events: Vec<(String, String, String)> = rows_of(
        connection,
        "SELECT EVENT_NAME, STATUS, EVENT_COMMENT FROM information_schema.EVENTS
             WHERE EVENT_SCHEMA = ? ORDER BY EVENT_NAME",
        schema,
    )
    .await?;
    let tables = tables.into_iter().map(|(name, table_type, rows, comment)| {
        let kind = table_kind(&table_type);
        let view = kind == ObjectKind::View;
        ObjectInfo {
            name,
            kind,
            // A view has no estimate: NULL, but 0 up to MySQL 8.0.16.
            rows: rows.filter(|_| !view),
            enabled: None,
            // The comment of a view reads `VIEW`.
            comment: if view { String::new() } else { comment },
        }
    });
    let routines = routines
        .into_iter()
        .map(|(name, routine_type, comment)| ObjectInfo {
            name,
            kind: if routine_type == "FUNCTION" {
                ObjectKind::Function
            } else {
                ObjectKind::Procedure
            },
            rows: None,
            enabled: None,
            comment,
        });
    let events = events
        .into_iter()
        .map(|(name, status, comment)| ObjectInfo {
            name,
            kind: ObjectKind::Event,
            rows: None,
            enabled: Some(status == "ENABLED"),
            comment,
        });
    Ok(tables.chain(routines).chain(events).collect())
}

/// Reads the details of all the tables of the database `schema`, table by table.
pub async fn details(
    connection: &mut Connection,
    schema: &str,
) -> Result<Vec<TableDetails>, MysqlError> {
    let columns: Vec<ColumnRow> = rows_of(
        connection,
        "SELECT TABLE_NAME, COLUMN_NAME, COLUMN_TYPE, IS_NULLABLE, COLUMN_KEY, EXTRA,
                    COLUMN_COMMENT
             FROM information_schema.COLUMNS
             WHERE TABLE_SCHEMA = ? ORDER BY TABLE_NAME, ORDINAL_POSITION",
        schema,
    )
    .await?;
    let indexes: Vec<IndexRow> = rows_of(
        connection,
        "SELECT TABLE_NAME, INDEX_NAME, NON_UNIQUE, COLUMN_NAME, INDEX_COMMENT
             FROM information_schema.STATISTICS
             WHERE TABLE_SCHEMA = ? ORDER BY TABLE_NAME, INDEX_NAME, SEQ_IN_INDEX",
        schema,
    )
    .await?;
    let foreign_keys: Vec<ForeignKeyRow> = rows_of(
        connection,
        "SELECT TABLE_NAME, CONSTRAINT_NAME, COLUMN_NAME,
                    REFERENCED_TABLE_SCHEMA, REFERENCED_TABLE_NAME, REFERENCED_COLUMN_NAME
             FROM information_schema.KEY_COLUMN_USAGE
             WHERE TABLE_SCHEMA = ? AND REFERENCED_TABLE_NAME IS NOT NULL
             ORDER BY TABLE_NAME, CONSTRAINT_NAME, ORDINAL_POSITION",
        schema,
    )
    .await?;
    let triggers: Vec<TriggerRow> = rows_of(
        connection,
        "SELECT EVENT_OBJECT_TABLE, TRIGGER_NAME, ACTION_TIMING, EVENT_MANIPULATION
             FROM information_schema.TRIGGERS
             WHERE TRIGGER_SCHEMA = ?
             ORDER BY EVENT_OBJECT_TABLE, ACTION_TIMING, EVENT_MANIPULATION, ACTION_ORDER",
        schema,
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
    for (name, column, column_type, is_nullable, key, extra, comment) in columns {
        table(&mut tables, &name).columns.push(ColumnInfo {
            name: column,
            column_type,
            nullable: is_nullable == "YES",
            key,
            // EXTRA can combine several mentions ("auto_increment", "on update...").
            auto_increment: extra.to_lowercase().contains("auto_increment"),
            comment,
        });
    }
    for (name, index, non_unique, column, comment) in indexes {
        let column = column.unwrap_or_else(|| "(expression)".to_string());
        let indexes = &mut table(&mut tables, &name).indexes;
        match indexes.last_mut() {
            Some(last) if last.name == index => last.columns.push(column),
            _ => indexes.push(IndexInfo {
                name: index,
                columns: vec![column],
                unique: non_unique == 0,
                comment,
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
            comment: String::new(),
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
fn table_kind(table_type: &str) -> ObjectKind {
    match table_type {
        "BASE TABLE" => ObjectKind::Table,
        "VIEW" => ObjectKind::View,
        // SYSTEM VIEW (MySQL, MariaDB), SYSTEM VERSIONED (MariaDB, versioned table)...
        "SYSTEM VERSIONED" => ObjectKind::Table,
        _ => ObjectKind::SystemTable,
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
                text("Identifier"),
            ),
            (
                text("t"),
                text("parent"),
                text("int"),
                text("YES"),
                text("MUL"),
                text(""),
                text(""),
            ),
            (
                text("u"),
                text("id"),
                text("int"),
                text("NO"),
                text("PRI"),
                text(""),
                text(""),
            ),
        ];
        let indexes = vec![
            (
                text("t"),
                text("idx_parent"),
                1,
                Some(text("parent")),
                text("By parent"),
            ),
            (text("t"), text("PRIMARY"), 0, Some(text("id")), text("")),
            (text("t"), text("uq_pair"), 0, Some(text("id")), text("")),
            (
                text("t"),
                text("uq_pair"),
                0,
                Some(text("parent")),
                text(""),
            ),
            (text("u"), text("idx_expr"), 1, None, text("")),
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
        assert_eq!(t.columns[0].comment, "Identifier");
        assert_eq!(t.columns[1].comment, "");
        assert!(!t.columns[1].auto_increment);
        let names: Vec<_> = t.indexes.iter().map(|index| index.name.as_str()).collect();
        assert_eq!(names, ["PRIMARY", "idx_parent", "uq_pair"]);
        assert_eq!(t.indexes[2].columns, ["id", "parent"]);
        assert!(t.indexes[2].unique);
        assert!(!t.indexes[1].unique);
        assert_eq!(t.indexes[1].comment, "By parent");
        assert_eq!(t.foreign_keys[0].referenced_table, "u");
        assert_eq!(t.triggers[0].name, "t_bi");
        assert_eq!(tables[1].indexes[0].columns, ["(expression)"]);
    }

    #[test]
    fn translates_table_types() {
        assert_eq!(table_kind("BASE TABLE"), ObjectKind::Table);
        assert_eq!(table_kind("SYSTEM VERSIONED"), ObjectKind::Table);
        assert_eq!(table_kind("VIEW"), ObjectKind::View);
        assert_eq!(table_kind("SYSTEM VIEW"), ObjectKind::SystemTable);
    }
}
