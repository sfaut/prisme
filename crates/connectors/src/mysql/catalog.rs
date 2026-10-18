//! Catalog of a MySQL or MariaDB server: databases, objects and table details.
//!
//! Serves the object explorer, which shows the elements as soon as they
//! arrive, in three steps:
//!
//! 1. `schemas`: database names;
//! 2. `objects`: tables, views, procedures, functions and events of a database;
//! 3. `details`: columns, indexes, foreign keys and triggers of each table of
//!    a database.
//!
//! The transpiler needs the columns of the tables a query reads, with their
//! type: `table_columns`, on the connection that runs the query; and the
//! types of the user variables it reads: `user_variable_types`. The
//! metadata of a prepared statement gives a `JSON` column the type of a
//! `LONGTEXT`, and a string the collation of the results of the connection:
//! `SHOW FULL COLUMNS` gives the collation of each column, and tells a
//! `JSON` apart in MySQL; in MariaDB, the check `json_valid` of the column.
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

use super::{Connection, Flavor, MysqlError, column_type};

/// Errors of the server for a table that does not exist: unknown table
/// (`ER_NO_SUCH_TABLE`), unknown database (`ER_BAD_DB_ERROR`).
const MISSING_TABLE: [u16; 2] = [1146, 1049];

/// Length in bytes of a `LONGTEXT` column, and of a `JSON` column, in the
/// metadata of a prepared statement.
const LONG_LENGTH: u32 = u32::MAX;

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

/// Scheduled event of a database (`CREATE EVENT`, MySQL and MariaDB).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct EventInfo {
    /// Name of the event.
    pub name: String,
    /// Running on its schedule (`ENABLED`), or paused (`DISABLED`).
    pub enabled: bool,
}

/// Objects of a database: tables and views, procedures and functions, events, by name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct SchemaObjects {
    /// Tables and views.
    pub tables: Vec<TableInfo>,
    /// Procedures and functions.
    pub routines: Vec<RoutineInfo>,
    /// Scheduled events.
    pub events: Vec<EventInfo>,
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

/// Columns of the table or view `table`, of the database `database` (the
/// current database when absent), with their internal type, in order; `None`
/// when the table or the database does not exist.
///
/// The server describes the columns of `SELECT * FROM t` when preparing it,
/// without running it: a temporary table of the connection is seen as well,
/// and a view is not computed (`docs/QUESTIONS.md`, point 44). The metadata
/// give a string the collation of the results (`utf8mb4_0900_ai_ci` for a
/// column of `utf8mb4_bin`, measured on MySQL 8.4.11 and MariaDB 11.8.9):
/// `SHOW FULL COLUMNS` gives its own (`declared_columns`). A column of the
/// length of a `LONGTEXT` may be a `JSON`, which `SHOW FULL COLUMNS` names in
/// MySQL, `SHOW CREATE TABLE` in MariaDB (`checked_json_columns`), as
/// `flavor` tells.
pub async fn table_columns(
    connection: &mut Connection,
    flavor: Flavor,
    database: Option<&str>,
    table: &str,
) -> Result<Option<Vec<(String, prisme_types::Type)>>, MysqlError> {
    let name = match database {
        Some(database) => format!("{}.{}", quote(database), quote(table)),
        None => quote(table),
    };
    let statement = match connection.prep(format!("SELECT * FROM {name}")).await {
        Ok(statement) => statement,
        Err(mysql_async::Error::Server(error)) if MISSING_TABLE.contains(&error.code) => {
            return Ok(None);
        }
        Err(error) => return Err(error.into()),
    };
    let long = statement
        .columns()
        .iter()
        .any(|column| column.column_length() == LONG_LENGTH);
    let mut columns: Vec<(String, prisme_types::Type)> = statement
        .columns()
        .iter()
        .map(|column| {
            let ty = column_type::plan_type(
                column.column_type(),
                column.flags(),
                column.column_length(),
                column.decimals(),
                column.character_set(),
            );
            (column.name_str().into_owned(), ty)
        })
        .collect();
    connection.close(statement).await?;
    let declared = declared_columns(connection, &name).await?;
    let json = match flavor {
        Flavor::MySql => declared
            .iter()
            .filter(|column| column.declared.eq_ignore_ascii_case("json"))
            .map(|column| column.field.clone())
            .collect(),
        Flavor::MariaDb if long => checked_json_columns(connection, &name).await?,
        Flavor::MariaDb => Vec::new(),
    };
    for (column, ty) in &mut columns {
        if json.iter().any(|field| field.eq_ignore_ascii_case(column)) {
            ty.data_type = prisme_types::DataType::Json;
        }
        if let prisme_types::DataType::Text(text) = &mut ty.data_type {
            text.collation = declared
                .iter()
                .find(|declared| declared.field.eq_ignore_ascii_case(column))
                .and_then(|declared| declared.collation.clone());
        }
    }
    Ok(Some(columns))
}

/// Column as `SHOW FULL COLUMNS` declares it.
struct DeclaredColumn {
    /// Name of the column.
    field: String,
    /// Declared type: `varchar(20)`, `json`...
    declared: String,
    /// Collation of a string; `None` for another type.
    collation: Option<String>,
}

/// Columns of the table or view `name`, quoted, as `SHOW FULL COLUMNS`
/// declares them: a temporary table and a view as well.
async fn declared_columns(
    connection: &mut Connection,
    name: &str,
) -> Result<Vec<DeclaredColumn>, MysqlError> {
    let rows: Vec<mysql_async::Row> = connection
        .query(format!("SHOW FULL COLUMNS FROM {name}"))
        .await?;
    Ok(rows
        .into_iter()
        .filter_map(|row| {
            Some(DeclaredColumn {
                field: row.get(0)?,
                declared: row.get(1)?,
                collation: row.get::<Option<String>, _>(2)?,
            })
        })
        .collect())
}

/// Names of the `JSON` columns of the table `name`, quoted, as MariaDB reads
/// them: the columns whose check is a call of `json_valid`, which `JSON`
/// adds to its `LONGTEXT`, a temporary table as well (`SHOW CREATE TABLE`).
/// `SHOW COLUMNS` of MariaDB declares its `JSON` as a `LONGTEXT`, its
/// functions read such a column as a `JSON` (`JSON_ARRAY(j)` gives
/// `[{"a": 1}]`), and any other column as a string, a `JSON` with a check of
/// its own as well (measured on MariaDB 10.6.28 and 11.8.9). A view shows
/// no check: its columns are read as strings.
async fn checked_json_columns(
    connection: &mut Connection,
    name: &str,
) -> Result<Vec<String>, MysqlError> {
    let rows: Vec<mysql_async::Row> = connection
        .query(format!("SHOW CREATE TABLE {name}"))
        .await?;
    let create: Option<String> = rows.into_iter().next().and_then(|row| row.get(1));
    Ok(create.map_or_else(Vec::new, |create| {
        create.lines().filter_map(checked_json_column).collect()
    }))
}

/// Name of the column that `line` of `SHOW CREATE TABLE` of MariaDB
/// defines, when its check is a call of `json_valid`, of any argument
/// (`JSON CHECK (json_valid(k))` is read as a `JSON`); `None` for any
/// other line. A column takes one line, its check last. Its name is between
/// backticks, between double quotes under `ANSI_QUOTES`, bare when
/// `sql_quote_show_create` is off and the name needs no quotes; a string
/// doubles its quote or escapes it by a backslash (`'it''s'`, `'x\'y'`).
fn checked_json_column(line: &str) -> Option<String> {
    const CHECK: &str = " CHECK ";
    const JSON_VALID: &str = "json_valid";
    let line = line.strip_prefix("  ")?;
    let line = line.strip_suffix(',').unwrap_or(line);
    let (name, definition) = column_name(line)?;
    let (start, _) = unquoted(definition)
        .into_iter()
        .rev()
        .find(|&(index, _)| definition[index..].starts_with(" CHECK ("))?;
    let clause = parenthesized(&definition[start + CHECK.len()..])?;
    let function = clause.get(..JSON_VALID.len())?;
    let args = &clause[JSON_VALID.len()..];
    (function.eq_ignore_ascii_case(JSON_VALID) && parenthesized(args).is_some()).then_some(name)
}

/// Name of the column that `line` of `SHOW CREATE TABLE` begins with, and
/// the rest of the line; `None` for a check of the table, which begins with
/// the keyword `CONSTRAINT`, a bare name never being a keyword.
fn column_name(line: &str) -> Option<(String, &str)> {
    let Some(quote) = line.chars().next().filter(|c| matches!(c, '`' | '"')) else {
        let end = line.find(' ')?;
        let name = &line[..end];
        return (name != "CONSTRAINT").then(|| (name.to_string(), &line[end..]));
    };
    let mut name = String::new();
    let mut chars = line.char_indices().skip(1).peekable();
    while let Some((index, c)) = chars.next() {
        if c != quote {
            name.push(c);
        } else if chars.next_if(|&(_, next)| next == quote).is_some() {
            name.push(quote);
        } else {
            return Some((name, &line[index + 1..]));
        }
    }
    None
}

/// Text between the parenthesis that begins `text` and the one that ends it,
/// when they pair; `None` otherwise.
fn parenthesized(text: &str) -> Option<&str> {
    let inner = text.strip_prefix('(')?.strip_suffix(')')?;
    let mut depth = 0usize;
    for (_, c) in unquoted(inner) {
        match c {
            '(' => depth += 1,
            ')' => depth = depth.checked_sub(1)?,
            _ => {}
        }
    }
    (depth == 0).then_some(inner)
}

/// Characters of `text` outside its strings and its quoted names, with
/// their offset. A string is between single quotes, a backslash escaping the
/// next character; a name between backticks or double quotes; both double
/// their quote inside, which closes and opens again.
fn unquoted(text: &str) -> Vec<(usize, char)> {
    let mut found = Vec::new();
    let mut quote = None;
    let mut chars = text.char_indices();
    while let Some((index, c)) = chars.next() {
        match quote {
            Some('\'') if c == '\\' => {
                chars.next();
            }
            Some(open) if c == open => quote = None,
            Some(_) => {}
            None if matches!(c, '\'' | '`' | '"') => quote = Some(c),
            None => found.push((index, c)),
        }
    }
    found
}

/// Name between backticks, read as a name whatever the SQL mode.
/// Types of the values of the user variables `names` (without `@`) on the
/// connection, from the metadata of the result of `SELECT @a, @b`, run just
/// before the query (`docs/QUESTIONS.md`, point 44): the type of an unset
/// variable is that of NULL, a byte string. An empty name is left out, which
/// MariaDB refuses from 10.7.1: the transpiler reports it.
pub async fn user_variable_types(
    connection: &mut Connection,
    names: &[String],
) -> Result<Vec<(String, prisme_types::Type)>, MysqlError> {
    let names: Vec<&String> = names.iter().filter(|name| !name.is_empty()).collect();
    if names.is_empty() {
        return Ok(Vec::new());
    }
    let items: Vec<String> = names
        .iter()
        .map(|name| format!("@{}", quote(name)))
        .collect();
    let result = connection
        .query_iter(format!("SELECT {}", items.join(", ")))
        .await?;
    let types: Vec<prisme_types::Type> = result
        .columns_ref()
        .iter()
        .map(|column| {
            column_type::plan_type(
                column.column_type(),
                column.flags(),
                column.column_length(),
                column.decimals(),
                column.character_set(),
            )
        })
        .collect();
    result.drop_result().await?;
    Ok(names.into_iter().cloned().zip(types).collect())
}

fn quote(name: &str) -> String {
    format!("`{}`", name.replace('`', "``"))
}

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
    let events: Vec<(String, String)> = connection
        .exec(
            "SELECT EVENT_NAME, STATUS FROM information_schema.EVENTS
             WHERE EVENT_SCHEMA = ? ORDER BY EVENT_NAME",
            (schema,),
        )
        .await?;
    Ok(SchemaObjects {
        events: events
            .into_iter()
            .map(|(name, status)| EventInfo {
                name,
                enabled: status == "ENABLED",
            })
            .collect(),
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
    use indoc::indoc;

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
    fn reads_the_json_columns_of_mariadb_by_their_check() {
        // Lines of `SHOW CREATE TABLE` measured on MariaDB 10.6.28 and 11.8.9.
        let create = indoc! {r#"
            CREATE TABLE `jc` (
              `a``b` longtext CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NOT NULL DEFAULT '{}' COMMENT 'c CHECK (json_valid(`x`))' CHECK (json_valid(`a``b`)),
              `c,d` longtext CHARACTER SET utf8mb4 COLLATE utf8mb4_bin DEFAULT NULL INVISIBLE CHECK (json_valid(`c,d`)),
              `e` longtext DEFAULT NULL COMMENT 'CHECK (json_valid(`e`))',
              `f` longtext CHARACTER SET utf8mb4 COLLATE utf8mb4_bin DEFAULT NULL CHECK (json_valid(`f`) and `f` <> '1'),
              `g` longtext CHARACTER SET utf8mb4 COLLATE utf8mb4_bin GENERATED ALWAYS AS (json_object('b',`b`)) VIRTUAL CHECK (json_valid(`g`)),
              `h` longtext CHARACTER SET utf8mb4 COLLATE utf8mb4_bin DEFAULT NULL COMMENT 'it''s ) CHECK (' CHECK (json_valid(`h`)),
              `i` longtext /*M!100301 COMPRESSED*/ CHARACTER SET utf8mb4 COLLATE utf8mb4_bin DEFAULT 'x\'y' CHECK (json_valid(`i`)),
              `k` longtext CHARACTER SET utf8mb4 COLLATE utf8mb4_bin DEFAULT NULL CHECK (json_valid(`g`)),
              "l m" longtext CHARACTER SET utf8mb4 COLLATE utf8mb4_bin DEFAULT NULL CHECK (json_valid("l m")),
              n longtext CHARACTER SET utf8mb4 COLLATE utf8mb4_bin DEFAULT json_array() CHECK (json_valid(n)),
              `o` longtext DEFAULT NULL COMMENT 'a\\' CHECK (json_valid(`o`) or `o` is null),
              `p` longtext DEFAULT NULL,
              CONSTRAINT `cp` CHECK (json_valid(`p`))
            ) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci
        "#};
        let names: Vec<String> = create.lines().filter_map(checked_json_column).collect();
        assert_eq!(names, ["a`b", "c,d", "g", "h", "i", "k", "l m", "n"]);
    }

    #[test]
    fn translates_table_types() {
        assert_eq!(table_kind("BASE TABLE"), TableKind::Table);
        assert_eq!(table_kind("SYSTEM VERSIONED"), TableKind::Table);
        assert_eq!(table_kind("VIEW"), TableKind::View);
        assert_eq!(table_kind("SYSTEM VIEW"), TableKind::System);
    }
}
