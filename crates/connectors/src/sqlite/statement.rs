//! Statements of a SQLite session, run on the thread of its connection: the
//! head of a result (its columns and its first row) comes back to the
//! reader at once, then the other rows through a bounded channel, which
//! holds the thread while the reader is behind. A reader that goes away
//! ends the reading; the statement is then reset, as SQLite allows at any
//! row.
//!
//! The first row is read before the head is sent: a statement fails at its
//! first step as at its preparation (a constraint, a read-only database),
//! and the type of a column without a declared type is that of its first
//! value. A value becomes the text Prisme shows: an integer in decimal, a
//! real with the fewest digits that read back the same value, in plain
//! decimal (`docs/TODO-SQLITE.md`, decision 14), a text as it is, a blob in
//! hexadecimal (`0x...`), as MySQL shows its binary data.
//!
//! The outcome of a write comes from `changes()`, which keeps the count of
//! the last write after a statement that writes nothing: it counts only
//! when `total_changes()` grew (`docs/SQL-COMPARISON.md`, section 6).

use std::time::Instant;

use prisme_dialects::verb::StatementVerb;
use rusqlite::types::{Type, ValueRef};
use rusqlite::{Connection, Row, Statement};
use tokio::sync::{mpsc, oneshot};

use super::failure::{cause, is_interruption};
use crate::api::{ColumnMeta, ConnectorError};
use crate::columns::{Cell, ColumnOrigin, ColumnTypeInfo, TypeFamily};
use crate::outcome::{OutcomeCounts, StatementOutcome};
use crate::sql_log::{self, Sent};

/// Rows sent ahead of the reader at most.
pub(super) const ROWS_AHEAD: usize = 256;

/// Message of the thread of a connection to the reader of a result.
pub(super) enum Message {
    /// Next row.
    Row(Vec<Cell>),
    /// The statement ran to its end.
    End(Ending),
    /// The statement failed after its first row.
    Failed(ConnectorError),
}

/// Start of a result: its columns, its first row, and its end when it has
/// at most one row.
pub(super) struct Head {
    /// Columns of the result, empty for a statement without rows.
    pub(super) columns: Vec<ColumnMeta>,
    /// First row, `None` for a result without rows.
    pub(super) first: Option<Vec<Cell>>,
    /// End of the statement, when it ended with its first step.
    pub(super) ending: Option<Ending>,
}

/// What a statement wrote, read once it ran.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct Ending {
    /// Rows the statement wrote itself (`changes()`), 0 for a statement that
    /// wrote none.
    pub(super) changes: u64,
    /// Did the statement write rows (`total_changes()` grew)?
    pub(super) wrote: bool,
    /// Row id of the row it inserted, when it inserted a single one.
    pub(super) inserted_rowid: Option<i64>,
}

/// Counters of a connection, read before a statement to tell what it wrote.
#[derive(Debug, Clone, Copy)]
struct Counters {
    /// `total_changes()`.
    total: u64,
    /// `last_insert_rowid()`.
    rowid: i64,
}

impl Counters {
    /// Counters of `connection` now.
    fn of(connection: &Connection) -> Self {
        Self {
            total: connection.total_changes(),
            rowid: connection.last_insert_rowid(),
        }
    }

    /// What the statement run since these counters wrote.
    fn ending(self, connection: &Connection) -> Ending {
        let wrote = connection.total_changes() > self.total;
        let changes = if wrote { connection.changes() } else { 0 };
        let rowid = connection.last_insert_rowid();
        Ending {
            changes,
            wrote,
            inserted_rowid: (wrote && changes == 1 && rowid != self.rowid).then_some(rowid),
        }
    }
}

/// Runs `sql` on `connection`: sends its head through `head`, then its
/// other rows and its end through `rows`. The server traces it, as it reads
/// it (`Session::execute`).
pub(super) fn run(
    connection: &Connection,
    sql: &str,
    head: oneshot::Sender<Result<Head, ConnectorError>>,
    rows: mpsc::Sender<Message>,
) {
    let counters = Counters::of(connection);
    let mut statement = match connection.prepare(sql) {
        Ok(statement) => statement,
        Err(error) => {
            let _ = head.send(Err(error.into()));
            return;
        }
    };
    let described = describe(connection, &statement);
    let mut result = statement.raw_query();
    let (first, classes) = match result.next() {
        Ok(row) => row.map(|row| (cells(row), classes(row))).unzip(),
        Err(error) => {
            let _ = head.send(Err(error.into()));
            return;
        }
    };
    let columns = columns(described, classes.as_deref());
    if first.is_none() {
        // A result without rows ended with its first step.
        drop(result);
        let _ = head.send(Ok(Head {
            columns,
            first,
            ending: Some(counters.ending(connection)),
        }));
        return;
    }
    let started = Head {
        columns,
        first,
        ending: None,
    };
    if head.send(Ok(started)).is_err() {
        return;
    }
    let message = loop {
        match result.next() {
            Ok(Some(row)) => {
                if rows.blocking_send(Message::Row(cells(row))).is_err() {
                    // The reader went away: the statement is reset.
                    return;
                }
            }
            Ok(None) => break None,
            Err(error) => break Some(Message::Failed(error.into())),
        }
    };
    drop(result);
    let message = message.unwrap_or_else(|| Message::End(counters.ending(connection)));
    let _ = rows.blocking_send(message);
}

/// Runs a statement that the connector sends itself (an attachment, the
/// state of a database...) with `run`, and traces it (`sql_log::record`).
pub(super) fn traced<T>(
    sql: &str,
    run: impl FnOnce() -> rusqlite::Result<T>,
) -> rusqlite::Result<T> {
    let started = Instant::now();
    let result = run();
    let error = result.as_ref().err().map(|error| cause(error).to_string());
    sql_log::record(&Sent {
        sql,
        duration: started.elapsed(),
        error: error.as_deref(),
        interrupted: result.as_ref().err().is_some_and(is_interruption),
        ..Sent::default()
    });
    result
}

/// Outcome of a statement of verb `verb` from what it wrote. `changes()`
/// counts the rows found by an `UPDATE`, changed or not, and, for an
/// upsert or a replacement, the rows inserted and updated or replaced
/// together: only their sum is told then.
pub(super) fn outcome(verb: StatementVerb, ending: Ending) -> StatementOutcome {
    let rows = ending.changes;
    let counts = match verb {
        StatementVerb::Insert => OutcomeCounts {
            inserted: Some(rows),
            ..OutcomeCounts::default()
        },
        StatementVerb::Update => OutcomeCounts {
            matched: Some(rows),
            ..OutcomeCounts::default()
        },
        StatementVerb::Delete => OutcomeCounts {
            deleted: Some(rows),
            ..OutcomeCounts::default()
        },
        _ => OutcomeCounts::default(),
    };
    let data = matches!(
        verb,
        StatementVerb::Insert
            | StatementVerb::Replace
            | StatementVerb::Upsert
            | StatementVerb::Update
            | StatementVerb::Delete
            | StatementVerb::Truncate
    );
    StatementOutcome {
        affected: (data || ending.wrote).then_some(rows),
        counts,
        inserted_id: ending.inserted_rowid.map(|rowid| rowid.to_string()),
        warnings: None,
        notices: Vec::new(),
    }
}

/// Column of a statement, before its first row: its name, its declared type
/// and its origin.
struct Described {
    /// Name of the column.
    name: String,
    /// Declared type of the column it reads, `None` for an expression.
    declared: Option<String>,
    /// Column of a table it reads, if any.
    origin: Option<ColumnOrigin>,
}

/// Columns of `statement`, with their origin: the schema, the table and the
/// column they read, and whether that column is in the primary key.
fn describe(connection: &Connection, statement: &Statement<'_>) -> Vec<Described> {
    statement
        .columns()
        .zip(statement.columns_with_metadata())
        .map(|(column, metadata)| {
            let origin = match (
                metadata.database_name(),
                metadata.table_name(),
                metadata.origin_name(),
            ) {
                (Some(schema), Some(table), Some(name)) => Some(ColumnOrigin {
                    schema: schema.to_string(),
                    table: table.to_string(),
                    column: name.to_string(),
                    primary_key: connection
                        .column_metadata(Some(schema), table, name)
                        .is_ok_and(|(_, _, _, primary_key, _)| primary_key),
                }),
                _ => None,
            };
            Described {
                name: column.name().to_string(),
                declared: column
                    .decl_type()
                    .filter(|declared| !declared.is_empty())
                    .map(str::to_string),
                origin,
            }
        })
        .collect()
}

/// Columns of a result, typed by their declared type, else by the storage
/// class of their value in the first row `classes`.
fn columns(described: Vec<Described>, classes: Option<&[Type]>) -> Vec<ColumnMeta> {
    described
        .into_iter()
        .enumerate()
        .map(|(index, column)| {
            let class = classes.and_then(|classes| classes.get(index)).copied();
            let (family, name) = match &column.declared {
                Some(declared) => (declared_family(declared), declared.clone()),
                None => class_family(class),
            };
            ColumnMeta {
                name: column.name,
                right_aligned: is_right_aligned(family),
                type_info: ColumnTypeInfo {
                    family,
                    name,
                    origin: column.origin,
                },
            }
        })
        .collect()
}

/// Family of a declared type, by the rules of the affinity of SQLite
/// (section 3.1 of its documentation of the types): `INT` gives an integer;
/// `CHAR`, `CLOB` or `TEXT` a text; `BLOB` binary data; `REAL`, `FLOA` or
/// `DOUB` a floating-point number; anything else is numeric, a date or a
/// time when its name says so (`DATE`, `DATETIME`, `TIMESTAMP`, `TIME`),
/// else a decimal.
fn declared_family(declared: &str) -> TypeFamily {
    let upper = declared.to_ascii_uppercase();
    let has = |part: &str| upper.contains(part);
    let word = upper.split(['(', ' ']).next().unwrap_or("");
    if has("INT") {
        TypeFamily::Integer
    } else if has("CHAR") || has("CLOB") || has("TEXT") {
        TypeFamily::Text
    } else if has("BLOB") {
        TypeFamily::Binary
    } else if has("REAL") || has("FLOA") || has("DOUB") {
        TypeFamily::Float
    } else {
        match word {
            "DATE" => TypeFamily::Date,
            "DATETIME" | "TIMESTAMP" => TypeFamily::Datetime,
            "TIME" => TypeFamily::Time,
            "JSON" => TypeFamily::Json,
            _ => TypeFamily::Decimal,
        }
    }
}

/// Family and name of a column without a declared type, from the storage
/// class of its first value; `Other` without a value.
fn class_family(class: Option<Type>) -> (TypeFamily, String) {
    let (family, name) = match class {
        Some(Type::Integer) => (TypeFamily::Integer, "INTEGER"),
        Some(Type::Real) => (TypeFamily::Float, "REAL"),
        Some(Type::Text) => (TypeFamily::Text, "TEXT"),
        Some(Type::Blob) => (TypeFamily::Binary, "BLOB"),
        Some(Type::Null) | None => (TypeFamily::Other, ""),
    };
    (family, name.to_string())
}

/// Are the values of a family right-aligned: numbers, dates and times?
fn is_right_aligned(family: TypeFamily) -> bool {
    matches!(
        family,
        TypeFamily::Integer
            | TypeFamily::Decimal
            | TypeFamily::Float
            | TypeFamily::Date
            | TypeFamily::Time
            | TypeFamily::Datetime
            | TypeFamily::Year
    )
}

/// Storage classes of the values of a row.
fn classes(row: &Row<'_>) -> Vec<Type> {
    (0..row.as_ref().column_count())
        .map(|index| {
            row.get_ref(index)
                .map_or(Type::Null, |value| value.data_type())
        })
        .collect()
}

/// Cells of a row, as Prisme shows them.
fn cells(row: &Row<'_>) -> Vec<Cell> {
    (0..row.as_ref().column_count())
        .map(|index| row.get_ref(index).ok().and_then(cell))
        .collect()
}

/// Text of a value, `None` for NULL.
fn cell(value: ValueRef<'_>) -> Cell {
    match value {
        ValueRef::Null => None,
        ValueRef::Integer(number) => Some(number.to_string()),
        // Rust writes a `f64` in plain decimal, with the fewest digits that
        // read back the same value.
        ValueRef::Real(number) => Some(number.to_string()),
        ValueRef::Text(bytes) => Some(match std::str::from_utf8(bytes) {
            Ok(text) => text.to_string(),
            Err(_) => hex(bytes),
        }),
        ValueRef::Blob(bytes) => Some(hex(bytes)),
    }
}

/// Bytes in uppercase hexadecimal, after `0x`.
fn hex(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(2 + 2 * bytes.len());
    text.push_str("0x");
    for byte in bytes {
        text.push_str(&format!("{byte:02X}"));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Head and rows of `sql` on `connection`, run as a session runs it.
    fn read(
        connection: &Connection,
        sql: &str,
    ) -> Result<(Head, Vec<Vec<Cell>>, Ending), ConnectorError> {
        let (head_sender, head) = oneshot::channel();
        let (rows_sender, mut rows) = mpsc::channel(ROWS_AHEAD);
        // The rows are read after the statement ran: enough room for them.
        run(connection, sql, head_sender, rows_sender);
        let head = head.blocking_recv().unwrap()?;
        let mut read: Vec<Vec<Cell>> = head.first.iter().cloned().collect();
        let mut ending = head.ending;
        while let Some(message) = rows.blocking_recv() {
            match message {
                Message::Row(row) => read.push(row),
                Message::End(end) => ending = Some(end),
                Message::Failed(error) => return Err(error),
            }
        }
        Ok((head, read, ending.unwrap()))
    }

    fn text(value: &str) -> Cell {
        Some(value.to_string())
    }

    #[test]
    fn writes_the_values_as_prisme_shows_them() {
        let connection = Connection::open_in_memory().unwrap();
        let (head, rows, _) = read(
            &connection,
            "SELECT 1, 1e20, 0.1 + 0.2, 1.0, 1e-7, 'clé €', x'00FF', NULL, CAST(x'FF' AS TEXT)",
        )
        .unwrap();
        assert_eq!(
            rows,
            [[
                text("1"),
                text("100000000000000000000"),
                text("0.30000000000000004"),
                text("1"),
                text("0.0000001"),
                text("clé €"),
                text("0x00FF"),
                None,
                text("0xFF"),
            ]]
        );
        let families: Vec<TypeFamily> = head
            .columns
            .iter()
            .map(|column| column.type_info.family)
            .collect();
        assert_eq!(
            families[..8],
            [
                TypeFamily::Integer,
                TypeFamily::Float,
                TypeFamily::Float,
                TypeFamily::Float,
                TypeFamily::Float,
                TypeFamily::Text,
                TypeFamily::Binary,
                TypeFamily::Other,
            ]
        );
    }

    #[test]
    fn types_the_columns_by_their_declared_type_and_their_origin() {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE t (id INTEGER PRIMARY KEY, label VARCHAR(20), price NUMERIC(10, 2),
                    at DATETIME, raw BLOB, ratio DOUBLE PRECISION, untyped);
                 INSERT INTO t VALUES (1, 'a', 2.5, '2026-11-23 10:00:00', x'01', 0.5, 'x')",
            )
            .unwrap();
        let (head, _, _) = read(&connection, "SELECT *, id + 1 AS next FROM t").unwrap();
        let types: Vec<(&str, TypeFamily, &str, bool)> = head
            .columns
            .iter()
            .map(|column| {
                (
                    column.name.as_str(),
                    column.type_info.family,
                    column.type_info.name.as_str(),
                    column.right_aligned,
                )
            })
            .collect();
        assert_eq!(
            types,
            [
                ("id", TypeFamily::Integer, "INTEGER", true),
                ("label", TypeFamily::Text, "VARCHAR(20)", false),
                ("price", TypeFamily::Decimal, "NUMERIC(10, 2)", true),
                ("at", TypeFamily::Datetime, "DATETIME", true),
                ("raw", TypeFamily::Binary, "BLOB", false),
                ("ratio", TypeFamily::Float, "DOUBLE PRECISION", true),
                // A column declared without a type has no declared type.
                ("untyped", TypeFamily::Text, "TEXT", false),
                ("next", TypeFamily::Integer, "INTEGER", true),
            ]
        );
        assert_eq!(
            head.columns[0].type_info.origin,
            Some(ColumnOrigin {
                schema: "main".to_string(),
                table: "t".to_string(),
                column: "id".to_string(),
                primary_key: true,
            })
        );
        assert!(
            head.columns[1]
                .type_info
                .origin
                .as_ref()
                .is_some_and(|origin| !origin.primary_key)
        );
        assert_eq!(head.columns[7].type_info.origin, None);
    }

    #[test]
    fn tells_what_a_write_did() {
        let connection = Connection::open_in_memory().unwrap();
        let ending = |sql: &str| read(&connection, sql).unwrap().2;
        ending("CREATE TABLE t (id INTEGER PRIMARY KEY, k TEXT UNIQUE, v INTEGER)");
        let inserted = ending("INSERT INTO t (k, v) VALUES ('a', 1), ('b', 2), ('c', 3)");
        assert_eq!(
            outcome(StatementVerb::Insert, inserted),
            StatementOutcome {
                affected: Some(3),
                counts: OutcomeCounts {
                    inserted: Some(3),
                    ..OutcomeCounts::default()
                },
                ..StatementOutcome::default()
            }
        );
        let one = outcome(
            StatementVerb::Insert,
            ending("INSERT INTO t (k) VALUES ('d')"),
        );
        assert_eq!(one.inserted_id.as_deref(), Some("4"));
        let updated = outcome(StatementVerb::Update, ending("UPDATE t SET v = v"));
        assert_eq!(updated.counts.matched, Some(4));
        // `changes()` still gives 4 after a statement that writes nothing.
        let created = outcome(StatementVerb::Schema, ending("CREATE TABLE u (a)"));
        assert_eq!(created.affected, None);
        let none = outcome(StatementVerb::Delete, ending("DELETE FROM t WHERE k = 'z'"));
        assert_eq!(none.affected, Some(0));
        let read_only = outcome(StatementVerb::Other, ending("SELECT 1"));
        assert_eq!(read_only.affected, None);
        let upsert = outcome(
            StatementVerb::Upsert,
            ending(
                "INSERT INTO t (k, v) VALUES ('a', 9), ('e', 5) ON CONFLICT (k) DO UPDATE SET v = excluded.v",
            ),
        );
        assert_eq!(upsert.affected, Some(2));
        assert_eq!(upsert.counts, OutcomeCounts::default());
    }

    #[test]
    fn fails_at_the_preparation_or_at_the_first_step() {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch("CREATE TABLE t (k TEXT UNIQUE); INSERT INTO t VALUES ('a')")
            .unwrap();
        for sql in ["SELECT * FROM missing", "INSERT INTO t VALUES ('a')"] {
            assert!(
                matches!(read(&connection, sql), Err(ConnectorError::Query(_))),
                "{sql}"
            );
        }
    }

    #[test]
    fn stops_when_the_reader_goes_away() {
        let connection = Connection::open_in_memory().unwrap();
        let (head_sender, head) = oneshot::channel();
        let (rows_sender, rows) = mpsc::channel(1);
        drop(rows);
        run(
            &connection,
            "WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c) SELECT x FROM c",
            head_sender,
            rows_sender,
        );
        let head = head.blocking_recv().unwrap().unwrap();
        assert_eq!(head.first, Some(vec![text("1")]));
    }
}
