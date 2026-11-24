//! Catalog of a SQLite session, read on the thread of its connection by the
//! pragmas of the library: its schemas (`pragma_database_list`: `main`, the
//! attached databases, and `temp` once used), the tables and views of each
//! one (`pragma_table_list`), then their columns, indexes and foreign keys
//! (`pragma_table_xinfo`, `pragma_index_list`, `pragma_index_xinfo`,
//! `pragma_foreign_key_list`), and their triggers (`sqlite_schema`).
//!
//! It takes the neutral forms of `crate::catalog`, those of MySQL as a
//! model: the primary key is an index named `PRIMARY`, even that of a rowid
//! table, which has none; a column whose first place is in an index is `PRI`,
//! `UNI` or `MUL`; the column of the row id (`INTEGER PRIMARY KEY`) is the
//! one SQLite fills itself, `auto_increment`. SQLite keeps no name for a
//! foreign key: each one is named by its number in the table (`fk_0`), as
//! `pragma_foreign_key_list` numbers them.
//!
//! SQLite has no `COMMENT` clause: the comment of a table, a view, a column,
//! an index or a trigger is read in the text of its `CREATE` statement
//! (`sqlite_schema.sql`), where it is written as `prisme_dialects::sqlite_comments`
//! describes. A schema has none.

use std::collections::BTreeMap;

use rusqlite::{Connection, Row};

use super::statement::traced;
use crate::api::Namespace;
use crate::catalog::{
    ColumnInfo, ForeignKeyInfo, IndexInfo, NamespaceInfo, ObjectInfo, ObjectKind, TableDetails,
    TriggerInfo,
};
use crate::sql_log;
use prisme_dialects::sqlite_comments::{object_comment, table_comments};

/// Schema of the temporary tables of a connection.
const TEMP_SCHEMA: &str = "temp";

/// Name of the index of a primary key, as MySQL names it.
const PRIMARY: &str = "PRIMARY";

/// Schemas of the connection, in the order SQLite searches them.
pub(super) fn namespaces(connection: &Connection) -> rusqlite::Result<Vec<NamespaceInfo>> {
    let names: Vec<String> = query(
        connection,
        "SELECT name FROM pragma_database_list ORDER BY seq",
        &[],
        |row| row.get(0),
    )?;
    Ok(names
        .into_iter()
        .map(|name| NamespaceInfo {
            system: name.eq_ignore_ascii_case(TEMP_SCHEMA),
            namespace: Namespace::single(name),
            comment: String::new(),
        })
        .collect())
}

/// Tables and views of the schema `schema`, by name. The tables of SQLite
/// (`sqlite_schema`, `sqlite_sequence`...) and the shadow tables of the
/// virtual tables are tables of the system.
pub(super) fn objects(connection: &Connection, schema: &str) -> rusqlite::Result<Vec<ObjectInfo>> {
    let texts = create_texts(connection, schema)?;
    query(
        connection,
        "SELECT name, type FROM pragma_table_list WHERE schema = ?1 ORDER BY name",
        &[schema],
        |row| {
            let name: String = row.get(0)?;
            let kind: String = row.get(1)?;
            let kind = if kind == "shadow" || name.starts_with("sqlite_") {
                ObjectKind::SystemTable
            } else if kind == "view" {
                ObjectKind::View
            } else {
                ObjectKind::Table
            };
            let comment = match (kind, texts.get(&name)) {
                (ObjectKind::View, Some(sql)) => object_comment(sql),
                (ObjectKind::Table, Some(sql)) => table_comments(sql).table,
                _ => String::new(),
            };
            Ok(ObjectInfo {
                name,
                kind,
                rows: None,
                enabled: None,
                comment,
            })
        },
    )
}

/// Column of a table, as `pragma_table_xinfo` gives it.
struct ColumnRow {
    /// Table of the column.
    table: String,
    /// Name of the column.
    name: String,
    /// Declared type, empty when none.
    declared: String,
    /// Is it declared `NOT NULL`?
    not_null: bool,
    /// Place in the primary key from 1, 0 outside it.
    primary_key: u32,
    /// Is its table `WITHOUT ROWID`?
    without_rowid: bool,
}

/// Column of an index.
struct IndexRow {
    /// Table of the index.
    table: String,
    /// Name of the index.
    index: String,
    /// Is it unique?
    unique: bool,
    /// What created it: `c` (`CREATE INDEX`), `u` (`UNIQUE`), `pk`.
    origin: String,
    /// Column, `None` for an expression.
    column: Option<String>,
}

/// Column of a foreign key.
struct ForeignKeyRow {
    /// Table of the key.
    table: String,
    /// Number of the key in its table.
    id: i64,
    /// Column of the table.
    column: String,
    /// Table referenced.
    referenced_table: String,
    /// Column referenced, `None` for the primary key of the table referenced.
    referenced_column: Option<String>,
}

/// Texts of the `CREATE` statements of the tables, views and indexes of the
/// schema `schema`, by name: those SQLite keeps, which hold their comments.
/// An index of a constraint (`sqlite_autoindex_...`) has none.
fn create_texts(
    connection: &Connection,
    schema: &str,
) -> rusqlite::Result<BTreeMap<String, String>> {
    let sql = format!(
        "SELECT name, sql FROM {}.sqlite_schema \
         WHERE type IN ('table', 'view', 'index') AND sql IS NOT NULL",
        quoted(schema)
    );
    Ok(
        query(connection, &sql, &[], |row| Ok((row.get(0)?, row.get(1)?)))?
            .into_iter()
            .collect(),
    )
}

/// Details of the tables and views of the schema `schema`, by name.
pub(super) fn details(
    connection: &Connection,
    schema: &str,
) -> rusqlite::Result<Vec<TableDetails>> {
    let texts = create_texts(connection, schema)?;
    let columns = query(
        connection,
        "SELECT l.name, c.name, c.type, c.\"notnull\", c.pk, l.wr \
         FROM pragma_table_list AS l, pragma_table_xinfo(l.name, l.schema) AS c \
         WHERE l.schema = ?1 AND c.hidden <> 1 ORDER BY l.name, c.cid",
        &[schema],
        |row| {
            Ok(ColumnRow {
                table: row.get(0)?,
                name: row.get(1)?,
                declared: row.get(2)?,
                not_null: row.get(3)?,
                primary_key: row.get(4)?,
                without_rowid: row.get(5)?,
            })
        },
    )?;
    let indexes = query(
        connection,
        "SELECT l.name, i.name, i.\"unique\", i.origin, x.name \
         FROM pragma_table_list AS l, pragma_index_list(l.name, l.schema) AS i, \
              pragma_index_xinfo(i.name, l.schema) AS x \
         WHERE l.schema = ?1 AND x.key = 1 ORDER BY l.name, i.name, x.seqno",
        &[schema],
        |row| {
            Ok(IndexRow {
                table: row.get(0)?,
                index: row.get(1)?,
                unique: row.get(2)?,
                origin: row.get(3)?,
                column: row.get(4)?,
            })
        },
    )?;
    let foreign_keys = query(
        connection,
        "SELECT l.name, f.id, f.\"from\", f.\"table\", f.\"to\" \
         FROM pragma_table_list AS l, pragma_foreign_key_list(l.name, l.schema) AS f \
         WHERE l.schema = ?1 ORDER BY l.name, f.id, f.seq",
        &[schema],
        |row| {
            Ok(ForeignKeyRow {
                table: row.get(0)?,
                id: row.get(1)?,
                column: row.get(2)?,
                referenced_table: row.get(3)?,
                referenced_column: row.get(4)?,
            })
        },
    )?;
    let triggers_sql = format!(
        "SELECT tbl_name, name, sql FROM {}.sqlite_schema \
         WHERE type = 'trigger' ORDER BY tbl_name, name",
        quoted(schema)
    );
    let triggers = query(connection, &triggers_sql, &[], |row| {
        let sql: String = row.get(2)?;
        let (timing, event) = trigger_head(&sql);
        Ok((
            row.get::<_, String>(0)?,
            TriggerInfo {
                name: row.get(1)?,
                timing,
                event,
                comment: object_comment(&sql),
            },
        ))
    })?;
    Ok(assemble(
        schema,
        columns,
        indexes,
        foreign_keys,
        triggers,
        &texts,
    ))
}

/// Gathers the rows read table by table.
fn assemble(
    schema: &str,
    columns: Vec<ColumnRow>,
    indexes: Vec<IndexRow>,
    foreign_keys: Vec<ForeignKeyRow>,
    triggers: Vec<(String, TriggerInfo)>,
    texts: &BTreeMap<String, String>,
) -> Vec<TableDetails> {
    // Comments of the columns, by table.
    let column_comments: BTreeMap<&str, Vec<(String, String)>> = texts
        .iter()
        .map(|(name, sql)| (name.as_str(), table_comments(sql).columns))
        .filter(|(_, columns)| !columns.is_empty())
        .collect();
    let column_comment = |table: &str, column: &str| {
        column_comments
            .get(table)
            .and_then(|columns| {
                columns
                    .iter()
                    .find(|(name, _)| name.eq_ignore_ascii_case(column))
            })
            .map(|(_, comment)| comment.clone())
            .unwrap_or_default()
    };
    let mut tables: BTreeMap<String, TableDetails> = BTreeMap::new();
    // Columns of the primary key of each table, in their order.
    let mut primary_keys: BTreeMap<String, Vec<(u32, String)>> = BTreeMap::new();
    let mut rowid_alias: BTreeMap<String, String> = BTreeMap::new();
    for column in &columns {
        if column.primary_key > 0 {
            primary_keys
                .entry(column.table.clone())
                .or_default()
                .push((column.primary_key, column.name.clone()));
        }
    }
    for (table, keys) in &mut primary_keys {
        keys.sort();
        let alias = columns.iter().find(|column| {
            column.table == *table
                && column.primary_key > 0
                && !column.without_rowid
                && column.declared.eq_ignore_ascii_case("INTEGER")
        });
        if let (1, Some(alias)) = (keys.len(), alias) {
            rowid_alias.insert(table.clone(), alias.name.clone());
        }
    }
    for column in columns {
        let alias = rowid_alias.get(&column.table) == Some(&column.name);
        let in_key = column.primary_key > 0;
        let comment = column_comment(&column.table, &column.name);
        table(&mut tables, &column.table).columns.push(ColumnInfo {
            comment,
            nullable: !column.not_null && !(in_key && (alias || column.without_rowid)),
            key: if in_key { "PRI" } else { "" }.to_string(),
            auto_increment: alias,
            column_type: column.declared,
            name: column.name,
        });
    }
    for (name, keys) in &primary_keys {
        table(&mut tables, name).indexes.push(IndexInfo {
            name: PRIMARY.to_string(),
            columns: keys.iter().map(|(_, column)| column.clone()).collect(),
            unique: true,
            comment: String::new(),
        });
    }
    for row in indexes {
        // The index of a primary key is the index `PRIMARY` already.
        if row.origin == "pk" {
            continue;
        }
        let column = row.column.unwrap_or_else(|| "(expression)".to_string());
        let indexes = &mut table(&mut tables, &row.table).indexes;
        match indexes.last_mut() {
            Some(last) if last.name == row.index => last.columns.push(column),
            _ => indexes.push(IndexInfo {
                comment: texts
                    .get(&row.index)
                    .map_or_else(String::new, |sql| object_comment(sql)),
                name: row.index,
                columns: vec![column],
                unique: row.unique,
            }),
        }
    }
    for row in foreign_keys {
        let name = format!("fk_{}", row.id);
        let referenced = row.referenced_column.map(|column| vec![column]);
        let details = table(&mut tables, &row.table);
        match details.foreign_keys.last_mut() {
            Some(last) if last.name == name => {
                last.columns.push(row.column);
                last.referenced_columns
                    .extend(referenced.into_iter().flatten());
            }
            _ => details.foreign_keys.push(ForeignKeyInfo {
                name,
                columns: vec![row.column],
                referenced_schema: schema.to_string(),
                // Without columns, the key references the primary key of the
                // table referenced, completed below.
                referenced_columns: referenced.unwrap_or_default(),
                referenced_table: row.referenced_table,
            }),
        }
    }
    for (name, trigger) in triggers {
        table(&mut tables, &name).triggers.push(trigger);
    }
    let mut tables: Vec<TableDetails> = tables.into_values().collect();
    for details in &mut tables {
        for key in &mut details.foreign_keys {
            if key.referenced_columns.is_empty()
                && let Some(primary) = primary_keys
                    .iter()
                    .find(|(name, _)| name.eq_ignore_ascii_case(&key.referenced_table))
            {
                key.referenced_columns =
                    primary.1.iter().map(|(_, column)| column.clone()).collect();
            }
        }
        mark_index_keys(details);
    }
    tables
}

/// Marks `UNI` or `MUL` the columns that come first in an index and are not
/// in the primary key, as MySQL does: `UNI` for a unique index of one
/// column, `MUL` otherwise.
fn mark_index_keys(details: &mut TableDetails) {
    for index in &details.indexes {
        let Some(first) = index.columns.first() else {
            continue;
        };
        let key = if index.unique && index.columns.len() == 1 {
            "UNI"
        } else {
            "MUL"
        };
        if let Some(column) = details
            .columns
            .iter_mut()
            .find(|column| column.name == *first && column.key.is_empty())
        {
            column.key = key.to_string();
        }
    }
}

/// Details of the table `name`, created empty if they do not exist yet.
fn table<'a>(tables: &'a mut BTreeMap<String, TableDetails>, name: &str) -> &'a mut TableDetails {
    tables
        .entry(name.to_string())
        .or_insert_with(|| TableDetails {
            table: name.to_string(),
            ..TableDetails::default()
        })
}

/// Runs a query of the catalog with `parameters`, and traces it.
fn query<T>(
    connection: &Connection,
    sql: &str,
    parameters: &[&str],
    read: impl FnMut(&Row<'_>) -> rusqlite::Result<T>,
) -> rusqlite::Result<Vec<T>> {
    traced(&sql_log::bound(sql, parameters), || {
        let mut statement = connection.prepare(sql)?;
        let rows = statement.query_map(rusqlite::params_from_iter(parameters), read)?;
        rows.collect()
    })
}

/// Identifier quoted for SQLite.
fn quoted(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

/// Timing (`BEFORE`, `AFTER`, `INSTEAD OF`) and event (`INSERT`, `UPDATE`,
/// `DELETE`) of a trigger, read from the text of its `CREATE TRIGGER`. A
/// trigger without a timing runs `BEFORE`, as SQLite says.
fn trigger_head(sql: &str) -> (String, String) {
    let words = words(sql);
    let mut rest = words
        .iter()
        .skip_while(|word| !word.eq_ignore_ascii_case("TRIGGER"))
        .skip(1)
        .peekable();
    // `IF NOT EXISTS`, then the name, qualified or not.
    if rest
        .peek()
        .is_some_and(|word| word.eq_ignore_ascii_case("IF"))
    {
        rest.nth(2);
    }
    rest.next();
    if rest.peek().is_some_and(|word| *word == ".") {
        rest.nth(1);
    }
    let mut timing = "BEFORE".to_string();
    let mut event = String::new();
    for word in rest {
        let upper = word.to_ascii_uppercase();
        match upper.as_str() {
            "BEFORE" | "AFTER" => timing = upper,
            "INSTEAD" => timing = "INSTEAD OF".to_string(),
            "INSERT" | "UPDATE" | "DELETE" => {
                event = upper;
                break;
            }
            _ => {}
        }
    }
    (timing, event)
}

/// Words of a text of SQL: names and keywords, each quoted name as one word,
/// the dots apart; strings and comments left out.
fn words(sql: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut chars = sql.chars().peekable();
    while let Some(character) = chars.next() {
        match character {
            '"' | '`' | '[' => {
                let close = if character == '[' { ']' } else { character };
                let mut word = String::new();
                while let Some(next) = chars.next() {
                    if next == close {
                        if close != ']' && chars.peek() == Some(&close) {
                            chars.next();
                        } else {
                            break;
                        }
                    }
                    word.push(next);
                }
                words.push(word);
            }
            '\'' => {
                while let Some(next) = chars.next() {
                    if next == '\'' && chars.next_if_eq(&'\'').is_none() {
                        break;
                    }
                }
            }
            '-' if chars.peek() == Some(&'-') => {
                for next in chars.by_ref() {
                    if next == '\n' {
                        break;
                    }
                }
            }
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
                let mut previous = ' ';
                for next in chars.by_ref() {
                    if previous == '*' && next == '/' {
                        break;
                    }
                    previous = next;
                }
            }
            '.' => words.push(".".to_string()),
            _ if character.is_alphanumeric() || character == '_' => {
                let mut word = character.to_string();
                while let Some(next) = chars.next_if(|next| next.is_alphanumeric() || *next == '_')
                {
                    word.push(next);
                }
                words.push(word);
            }
            _ => {}
        }
    }
    words
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Connection holding a few tables, a view and a trigger.
    fn connection() -> Connection {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE parent (id INTEGER PRIMARY KEY, code TEXT NOT NULL UNIQUE);
                 CREATE TABLE child (
                     a INT, b TEXT, parent_id INTEGER REFERENCES parent,
                     code TEXT REFERENCES parent (code),
                     PRIMARY KEY (a, b)
                 );
                 CREATE INDEX child_code ON child (code, lower(b));
                 CREATE TABLE pairs (k TEXT PRIMARY KEY, v) WITHOUT ROWID;
                 CREATE VIEW v AS SELECT id FROM parent;
                 CREATE TRIGGER \"log it\" AFTER UPDATE OF code ON parent BEGIN SELECT 1; END;
                 CREATE TRIGGER main.t2 INSTEAD OF DELETE ON v BEGIN SELECT 1; END;
                 CREATE TEMP TABLE scratch (x);",
            )
            .unwrap();
        connection
    }

    #[test]
    fn lists_the_schemas_and_their_objects() {
        let connection = connection();
        let names: Vec<(String, bool)> = namespaces(&connection)
            .unwrap()
            .into_iter()
            .map(|info| (info.namespace.name().to_string(), info.system))
            .collect();
        assert_eq!(
            names,
            [("main".to_string(), false), ("temp".to_string(), true)]
        );
        let listed: Vec<(String, ObjectKind)> = objects(&connection, "main")
            .unwrap()
            .into_iter()
            .map(|object| (object.name, object.kind))
            .collect();
        assert_eq!(
            listed,
            [
                ("child".to_string(), ObjectKind::Table),
                ("pairs".to_string(), ObjectKind::Table),
                ("parent".to_string(), ObjectKind::Table),
                ("sqlite_schema".to_string(), ObjectKind::SystemTable),
                ("v".to_string(), ObjectKind::View),
            ]
        );
        assert_eq!(objects(&connection, "temp").unwrap().len(), 2);
    }

    #[test]
    fn reads_the_details_of_the_tables() {
        let connection = connection();
        let details = details(&connection, "main").unwrap();
        let of = |name: &str| details.iter().find(|table| table.table == name).unwrap();

        let parent = of("parent");
        let columns: Vec<(&str, &str, bool, &str, bool)> = parent
            .columns
            .iter()
            .map(|column| {
                (
                    column.name.as_str(),
                    column.column_type.as_str(),
                    column.nullable,
                    column.key.as_str(),
                    column.auto_increment,
                )
            })
            .collect();
        assert_eq!(
            columns,
            [
                ("id", "INTEGER", false, "PRI", true),
                ("code", "TEXT", false, "UNI", false),
            ]
        );
        assert_eq!(parent.indexes[0].name, "PRIMARY");
        assert_eq!(parent.indexes[0].columns, ["id"]);
        assert_eq!(
            parent
                .triggers
                .iter()
                .map(|trigger| (
                    trigger.name.as_str(),
                    trigger.timing.as_str(),
                    trigger.event.as_str()
                ))
                .collect::<Vec<_>>(),
            [("log it", "AFTER", "UPDATE")]
        );

        let child = of("child");
        let indexes: Vec<(&str, Vec<&str>, bool)> = child
            .indexes
            .iter()
            .map(|index| {
                (
                    index.name.as_str(),
                    index.columns.iter().map(String::as_str).collect(),
                    index.unique,
                )
            })
            .collect();
        assert_eq!(
            indexes,
            [
                ("PRIMARY", vec!["a", "b"], true),
                ("child_code", vec!["code", "(expression)"], false),
            ]
        );
        // The columns of a primary key of a rowid table that is not its row
        // id may hold NULL.
        assert!(child.columns.iter().all(|column| column.nullable));
        assert!(child.columns.iter().all(|column| !column.auto_increment));
        let keys: Vec<(&str, Vec<&str>, &str, Vec<&str>)> = child
            .foreign_keys
            .iter()
            .map(|key| {
                (
                    key.referenced_table.as_str(),
                    key.columns.iter().map(String::as_str).collect(),
                    key.referenced_schema.as_str(),
                    key.referenced_columns.iter().map(String::as_str).collect(),
                )
            })
            .collect();
        assert_eq!(
            keys,
            [
                ("parent", vec!["code"], "main", vec!["code"]),
                ("parent", vec!["parent_id"], "main", vec!["id"]),
            ]
        );

        let pairs = of("pairs");
        assert!(!pairs.columns[0].nullable);
        assert!(!pairs.columns[0].auto_increment);
        assert_eq!(of("v").columns[0].name, "id");
        assert_eq!(
            (
                of("v").triggers[0].timing.as_str(),
                of("v").triggers[0].event.as_str()
            ),
            ("INSTEAD OF", "DELETE")
        );
    }

    #[test]
    fn reads_the_head_of_a_trigger() {
        let head = |sql: &str| trigger_head(sql);
        assert_eq!(
            head("CREATE TRIGGER IF NOT EXISTS \"after\".x DELETE ON t BEGIN SELECT 'AFTER'; END"),
            ("BEFORE".to_string(), "DELETE".to_string())
        );
        assert_eq!(
            head("CREATE TEMP TRIGGER [before insert] -- AFTER\n BEFORE INSERT ON t BEGIN END"),
            ("BEFORE".to_string(), "INSERT".to_string())
        );
    }

    #[test]
    fn reads_the_comments_of_the_create_statements() {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "-- Lost: SQLite keeps the text from CREATE
                 CREATE TABLE customer -- Customers
                 -- of the shop
                 (
                     id INTEGER PRIMARY KEY, -- Identifier
                     /*
                      * Name, as written
                      * by the customer
                      */
                     name TEXT NOT NULL,
                     age INTEGER /* Years,
                                    if known */
                 ) -- Lost: after the closing parenthesis
                 ;
                 CREATE TABLE plain (x);
                 CREATE INDEX customer_name /* Search
                                               by name */ ON customer (name);
                 CREATE VIEW adult -- Customers
                 -- of age
                 AS SELECT * FROM customer WHERE age >= 18;
                 CREATE TRIGGER customer_audit /* Audit of
                                                  the inserts */
                 AFTER INSERT ON customer BEGIN SELECT 1; END;",
            )
            .unwrap();
        // SQLite keeps the text from CREATE to the end of the statement: the
        // comments before it and after its last token are gone.
        let kept = |name: &str| -> String {
            connection
                .query_row(
                    "SELECT sql FROM sqlite_schema WHERE name = ?1",
                    [name],
                    |row| row.get(0),
                )
                .unwrap()
        };
        let customer = kept("customer");
        assert!(
            customer.starts_with("CREATE TABLE customer -- Customers"),
            "{customer}"
        );
        assert!(customer.ends_with(')'), "{customer}");
        assert!(kept("customer_audit").ends_with("END"));

        let comments: Vec<(String, String)> = objects(&connection, "main")
            .unwrap()
            .into_iter()
            .map(|object| (object.name, object.comment))
            .collect();
        let text = |name: &str, comment: &str| (name.to_string(), comment.to_string());
        assert_eq!(
            comments,
            [
                text("adult", "Customers of age"),
                text("customer", "Customers of the shop"),
                text("plain", ""),
                text("sqlite_schema", ""),
            ]
        );

        let tables = details(&connection, "main").unwrap();
        let customer = tables
            .iter()
            .find(|table| table.table == "customer")
            .unwrap();
        let columns: Vec<(&str, &str)> = customer
            .columns
            .iter()
            .map(|column| (column.name.as_str(), column.comment.as_str()))
            .collect();
        assert_eq!(
            columns,
            [
                ("id", "Identifier"),
                ("name", "Name, as written by the customer"),
                ("age", "Years, if known"),
            ]
        );
        let indexes: Vec<(&str, &str)> = customer
            .indexes
            .iter()
            .map(|index| (index.name.as_str(), index.comment.as_str()))
            .collect();
        assert_eq!(
            indexes,
            [("PRIMARY", ""), ("customer_name", "Search by name")]
        );
        assert_eq!(customer.triggers[0].comment, "Audit of the inserts");
        let plain = tables.iter().find(|table| table.table == "plain").unwrap();
        assert_eq!(plain.columns[0].comment, "");
    }
}
