//! Measures of the SQLite embedded in Prisme, before its connector is written
//! (`docs/TODO-SQLITE.md`, step 0). Each measure runs the library as the
//! connector will, and prints what it does: read-only inherited by an
//! attachment, files created by an attachment, databases in memory shared
//! between connections (`memdb`), text of the real numbers, interruption,
//! transactions after an error, detaching while in use, paths on Windows,
//! values kept by `INT`, `NUMERIC` and `REAL` columns, lexical rules of the
//! texts (comments, quotes, end of a statement), `LIMIT` after `VALUES`,
//! pragmas (forms, unknown names, functions, read-only files), what
//! `changes()` and `last_insert_rowid()` report after each write, how the
//! connector opens its databases (flags of a database in memory, `ATTACH` with
//! parameters, `database_list`), the rows the update hook reports for each
//! write and its cost. The results go to
//! `docs/SQL-COMPARISON.md`, section "SQLite embedded in Prisme".
//!
//! From the root of the repository:
//! `cargo run --release --manifest-path bench/sqlite/Cargo.toml --target-dir target/sqlite-measures`

use std::ffi::CString;
use std::fmt::Debug;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use std::sync::{Arc, Mutex};

use rusqlite::hooks::Action;
use rusqlite::types::ValueRef;
use rusqlite::{Connection, OpenFlags};

/// Flags of a connection that reads and writes an existing file, URIs read.
const READ_WRITE: OpenFlags = OpenFlags::SQLITE_OPEN_READ_WRITE
    .union(OpenFlags::SQLITE_OPEN_URI)
    .union(OpenFlags::SQLITE_OPEN_NO_MUTEX);

/// Flags of a connection that only reads, URIs read.
const READ_ONLY: OpenFlags = OpenFlags::SQLITE_OPEN_READ_ONLY
    .union(OpenFlags::SQLITE_OPEN_URI)
    .union(OpenFlags::SQLITE_OPEN_NO_MUTEX);

/// Flags of a connection that may create its file, URIs read.
const CREATE: OpenFlags = READ_WRITE.union(OpenFlags::SQLITE_OPEN_CREATE);

fn main() {
    let folder =
        std::env::temp_dir().join(format!("prisme-sqlite-measures-{}", std::process::id()));
    std::fs::create_dir_all(&folder).expect("temporary folder");

    println!("SQLite {} (rusqlite 0.41.1, bundled)", rusqlite::version());
    println!("Folder: {}", folder.display());

    read_only_attachments(&folder);
    attachment_creates_files(&folder);
    memdb_sharing();
    real_text();
    interruption(&folder);
    autocommit_after_errors(&folder);
    detach_while_in_use(&folder);
    windows_paths(&folder);
    numeric_affinity();
    lexical_rules();
    limit_after_values();
    pragmas(&folder);
    write_outcomes();
    connector_opening(&folder);
    update_hook();

    std::fs::remove_dir_all(&folder).ok();
}

/// Prints one measure: its name and its outcome.
fn report<T: Debug>(name: &str, outcome: rusqlite::Result<T>) {
    match outcome {
        Ok(value) => println!("  {name}: ok {value:?}"),
        Err(error) => println!("  {name}: error \"{error}\""),
    }
}

/// Creates a database file holding a table `t` of one row.
fn create_database(path: &Path) {
    let connection = Connection::open_with_flags(path, CREATE).expect("database created");
    connection
        .execute_batch("CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT); INSERT INTO t (name) VALUES ('one');")
        .expect("table created");
}

/// Quotes a text as an SQL string literal.
fn literal(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}

/// Read-only of a connection, inherited or not by the databases it attaches.
fn read_only_attachments(folder: &Path) {
    println!("\n1. Read-only and attachments");
    let main = folder.join("ro-main.db");
    let other = folder.join("ro-other.db");
    create_database(&main);
    create_database(&other);

    let connection = Connection::open_with_flags(&main, READ_ONLY).expect("open read-only");
    report(
        "ATTACH of an existing file on a read-only connection",
        connection.execute(
            &format!("ATTACH {} AS other", literal(&other.to_string_lossy())),
            [],
        ),
    );
    report(
        "INSERT into the attached database",
        connection.execute("INSERT INTO other.t (name) VALUES ('two')", []),
    );
    report(
        "INSERT into main",
        connection.execute("INSERT INTO main.t (name) VALUES ('two')", []),
    );
    let missing = folder.join("ro-missing.db");
    report(
        "ATTACH of a missing file on a read-only connection",
        connection.execute(
            &format!("ATTACH {} AS missing", literal(&missing.to_string_lossy())),
            [],
        ),
    );
    println!("  missing file created: {}", missing.exists());

    let writer = Connection::open_with_flags(&main, READ_WRITE).expect("open read-write");
    let uri = format!(
        "file:{}?mode=ro",
        other.to_string_lossy().replace('\\', "/")
    );
    report(
        "ATTACH 'file:...?mode=ro' on a read-write connection",
        writer.execute(&format!("ATTACH {} AS other", literal(&uri)), []),
    );
    report(
        "INSERT into that attached database",
        writer.execute("INSERT INTO other.t (name) VALUES ('two')", []),
    );
}

/// Files that an attachment creates, or not, when missing.
fn attachment_creates_files(folder: &Path) {
    println!("\n2. Attaching a missing file");
    let main = folder.join("create-main.db");
    create_database(&main);
    let connection = Connection::open_with_flags(&main, READ_WRITE).expect("open read-write");

    let plain = folder.join("create-plain.db");
    report(
        "ATTACH of a missing path, connection opened without CREATE",
        connection.execute(
            &format!("ATTACH {} AS plain", literal(&plain.to_string_lossy())),
            [],
        ),
    );
    println!(
        "  file created: {}, size {:?}",
        plain.exists(),
        std::fs::metadata(&plain).map(|meta| meta.len()).ok()
    );

    let guarded = folder.join("create-guarded.db");
    let uri = format!(
        "file:{}?mode=rw",
        guarded.to_string_lossy().replace('\\', "/")
    );
    report(
        "ATTACH 'file:...?mode=rw' of a missing path",
        connection.execute(&format!("ATTACH {} AS guarded", literal(&uri)), []),
    );
    println!("  file created: {}", guarded.exists());

    let opened = folder.join("create-open.db");
    report(
        "open of a missing path without CREATE",
        Connection::open_with_flags(&opened, READ_WRITE).map(|_| ()),
    );
    println!("  file created: {}", opened.exists());

    let text = folder.join("create-text.db");
    std::fs::write(&text, "not a database").expect("text file");
    report(
        "ATTACH of a text file",
        connection.execute(
            &format!("ATTACH {} AS text", literal(&text.to_string_lossy())),
            [],
        ),
    );
    report(
        "SELECT from the text file",
        connection.query_row("SELECT count(*) FROM text.sqlite_schema", [], |row| {
            row.get::<_, i64>(0)
        }),
    );
}

/// A database in memory of the `memdb` VFS, shared by the connections of the
/// process, and how long it lives.
fn memdb_sharing() {
    println!("\n3. Databases in memory (memdb)");
    let uri = "file:/prisme-measures?vfs=memdb";
    let first = match Connection::open_with_flags(uri, CREATE) {
        Ok(connection) => connection,
        Err(error) => {
            println!("  open of a memdb database: error \"{error}\"");
            return;
        }
    };
    report(
        "CREATE TABLE and INSERT on the first connection",
        first.execute_batch("CREATE TABLE m (x INTEGER); INSERT INTO m VALUES (1), (2);"),
    );
    let second = Connection::open_with_flags(uri, CREATE).expect("second connection");
    report(
        "rows seen by a second connection",
        second.query_row("SELECT count(*) FROM m", [], |row| row.get::<_, i64>(0)),
    );
    drop(first);
    report(
        "rows seen after the first connection closed",
        second.query_row("SELECT count(*) FROM m", [], |row| row.get::<_, i64>(0)),
    );
    drop(second);
    let third = Connection::open_with_flags(uri, CREATE).expect("third connection");
    report(
        "table after every connection closed",
        third.query_row(
            "SELECT count(*) FROM sqlite_schema WHERE name = 'm'",
            [],
            |row| row.get::<_, i64>(0),
        ),
    );

    let attaching = Connection::open_in_memory().expect("plain connection");
    report(
        "ATTACH of a memdb database by URI",
        attaching.execute("ATTACH 'file:/prisme-attached?vfs=memdb' AS scratch", []),
    );
    report(
        "CREATE TABLE in it",
        attaching
            .execute_batch("CREATE TABLE scratch.s (x INTEGER); INSERT INTO scratch.s VALUES (7);"),
    );
    let other = Connection::open_in_memory().expect("other connection");
    report(
        "the same ATTACH from another connection, rows seen",
        other
            .execute("ATTACH 'file:/prisme-attached?vfs=memdb' AS scratch", [])
            .and_then(|_| {
                other.query_row("SELECT count(*) FROM scratch.s", [], |row| {
                    row.get::<_, i64>(0)
                })
            }),
    );
    let private = Connection::open_in_memory().expect("private connection");
    report(
        "ATTACH ':memory:' then a table, seen by another connection",
        private
            .execute_batch("ATTACH ':memory:' AS mine; CREATE TABLE mine.p (x INTEGER);")
            .and_then(|_| other.execute("ATTACH ':memory:' AS mine", []))
            .and_then(|_| {
                other.query_row("SELECT count(*) FROM mine.sqlite_schema", [], |row| {
                    row.get::<_, i64>(0)
                })
            }),
    );
}

/// Text of real numbers: SQLite's conversion against Rust's.
fn real_text() {
    println!("\n4. Text of a REAL");
    let connection = Connection::open_in_memory().expect("connection");
    for value in [
        0.1,
        1.0,
        100.0,
        1e15,
        1e16,
        1e20,
        1e-7,
        0.1 + 0.2,
        std::f64::consts::PI,
        123_456_789.123_456_79,
        f64::MAX,
    ] {
        let sqlite: String = connection
            .query_row("SELECT CAST(?1 AS TEXT)", [value], |row| row.get(0))
            .expect("cast");
        let printed: String = connection
            .query_row("SELECT printf('%!.17g', ?1)", [value], |row| row.get(0))
            .expect("printf");
        println!(
            "  {value:?}: SQLite CAST {sqlite}, printf %!.17g {printed}, Rust Display {value}"
        );
    }
}

/// Interruption of a running query from another thread.
fn interruption(folder: &Path) {
    println!("\n5. Interruption");
    let path = folder.join("interrupt.db");
    create_database(&path);
    let connection = Connection::open_with_flags(&path, READ_WRITE).expect("connection");
    let handle = connection.get_interrupt_handle();
    let worker = thread::spawn(move || {
        let start = Instant::now();
        let outcome = connection.query_row(
            "WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c) SELECT count(*) FROM c",
            [],
            |row| row.get::<_, i64>(0),
        );
        let elapsed = start.elapsed();
        let after = connection.query_row("SELECT 1", [], |row| row.get::<_, i64>(0));
        (outcome, elapsed, after, connection)
    });
    thread::sleep(Duration::from_millis(200));
    let sent = Instant::now();
    handle.interrupt();
    let (outcome, elapsed, after, connection) = worker.join().expect("worker");
    report("endless query interrupted after 200 ms", outcome);
    println!(
        "  ran {elapsed:?}, stopped {:?} after the interrupt",
        sent.elapsed()
    );
    report("SELECT 1 on the same connection afterwards", after);

    connection
        .execute_batch("BEGIN; INSERT INTO t (name) VALUES ('in transaction');")
        .expect("transaction");
    let handle = connection.get_interrupt_handle();
    let worker = thread::spawn(move || {
        let outcome = connection.execute(
            "INSERT INTO t (name) WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c) SELECT 'n' FROM c",
            [],
        );
        (outcome, connection)
    });
    thread::sleep(Duration::from_millis(200));
    handle.interrupt();
    let (outcome, connection) = worker.join().expect("worker");
    report("endless INSERT interrupted inside a transaction", outcome);
    println!(
        "  is_autocommit afterwards (true: the transaction was rolled back): {}",
        connection.is_autocommit()
    );
    report(
        "rows inserted before, still there",
        connection.query_row(
            "SELECT count(*) FROM t WHERE name = 'in transaction'",
            [],
            |row| row.get::<_, i64>(0),
        ),
    );
}

/// Transaction state after errors inside a transaction.
fn autocommit_after_errors(folder: &Path) {
    println!("\n6. Transaction after an error");
    let path = folder.join("autocommit.db");
    create_database(&path);
    let connection = Connection::open_with_flags(&path, READ_WRITE).expect("connection");
    connection
        .execute_batch("BEGIN; INSERT INTO t (id, name) VALUES (10, 'a');")
        .expect("transaction");
    report(
        "duplicate primary key inside the transaction",
        connection.execute("INSERT INTO t (id, name) VALUES (10, 'b')", []),
    );
    println!("  is_autocommit: {}", connection.is_autocommit());
    report(
        "syntax error inside the transaction",
        connection.execute("SELEC 1", []),
    );
    println!("  is_autocommit: {}", connection.is_autocommit());
    report(
        "INSERT OR ROLLBACK conflicting",
        connection.execute("INSERT OR ROLLBACK INTO t (id, name) VALUES (10, 'c')", []),
    );
    println!("  is_autocommit: {}", connection.is_autocommit());
}

/// Detaching a database while a transaction or a read uses it.
fn detach_while_in_use(folder: &Path) {
    println!("\n7. DETACH while in use");
    let main = folder.join("detach-main.db");
    let other = folder.join("detach-other.db");
    create_database(&main);
    create_database(&other);
    let connection = Connection::open_with_flags(&main, READ_WRITE).expect("connection");
    let attach = format!("ATTACH {} AS other", literal(&other.to_string_lossy()));
    connection.execute(&attach, []).expect("attach");

    connection
        .execute_batch("BEGIN; SELECT count(*) FROM other.t;")
        .expect("transaction");
    report(
        "DETACH inside a transaction that read it",
        connection.execute("DETACH other", []),
    );
    connection.execute_batch("ROLLBACK").ok();
    connection.execute("DETACH other", []).ok();
    connection.execute(&attach, []).expect("attach again");

    connection.execute_batch("BEGIN").expect("begin");
    report(
        "DETACH inside a transaction that did not touch it",
        connection.execute("DETACH other", []),
    );
    connection.execute_batch("ROLLBACK").ok();
    connection.execute(&attach, []).ok();

    let mut statement = connection
        .prepare("SELECT name FROM other.t")
        .expect("prepare");
    let mut rows = statement.query([]).expect("query");
    let first = rows
        .next()
        .map(|row| row.map(|row| row.get::<_, String>(0)));
    println!(
        "  read started: {:?}",
        first.map(|row| row.map(|name| name.ok()))
    );
    report(
        "DETACH while a read of it is open",
        connection.execute("DETACH other", []),
    );
    drop(rows);
    drop(statement);
    report(
        "DETACH once the read is finished",
        connection.execute("DETACH other", []),
    );
}

/// Values written into an `INT`, a `NUMERIC` and a `REAL` column: the
/// storage class SQLite keeps, and the text it gives for them.
fn numeric_affinity() {
    println!("\n9. INT, NUMERIC and REAL columns");
    let connection = Connection::open_in_memory().expect("connection");
    connection
        .execute_batch(
            "CREATE TABLE n (written TEXT, int_value INT, numeric_value NUMERIC, real_value REAL)",
        )
        .expect("table");
    let values = [
        "1e2",
        "1e10",
        "1.0e+10",
        "1e20",
        "1.5",
        "10.0",
        "0.1",
        "12345678901234567890",
        "'1e10'",
        "'1.50'",
        "'abc'",
        "9223372036854775807",
        "9223372036854775808",
    ];
    for value in values {
        connection
            .execute(
                &format!(
                    "INSERT INTO n VALUES ({}, {value}, {value}, {value})",
                    literal(value)
                ),
                [],
            )
            .expect("insert");
    }
    // The text of a value is the one SQLite gives when asked for text
    // (`CAST(x AS TEXT)` converts as `sqlite3_column_text` does).
    let mut statement = connection
        .prepare(
            "SELECT written, typeof(int_value), CAST(int_value AS TEXT), typeof(numeric_value), CAST(numeric_value AS TEXT), \
             typeof(real_value), CAST(real_value AS TEXT) FROM n",
        )
        .expect("prepare");
    let rows = statement
        .query_map([], |row| {
            Ok([0, 1, 2, 3, 4, 5, 6].map(|index| row.get::<_, String>(index).unwrap_or_default()))
        })
        .expect("query");
    for row in rows {
        let [
            written,
            int_class,
            int_text,
            numeric_class,
            numeric_text,
            real_class,
            real_text,
        ] = row.expect("row");
        println!(
            "  {written}: INT {int_class} {int_text}, NUMERIC {numeric_class} {numeric_text}, REAL {real_class} {real_text}"
        );
    }
}

/// Paths written with `/`, `\`, and as `file:` URIs, on this system.
fn windows_paths(folder: &Path) {
    println!("\n8. Paths");
    let path = folder.join("paths.db");
    create_database(&path);
    let text = path.to_string_lossy().to_string();
    let forward = text.replace('\\', "/");
    let backward = text.replace('/', "\\");
    let variants: [(&str, String); 4] = [
        ("separators '\\'", backward.clone()),
        ("separators '/'", forward.clone()),
        (
            "URI file:///",
            format!("file:///{}", forward.trim_start_matches('/')),
        ),
        ("URI file: with '\\'", format!("file:{backward}")),
    ];
    for (name, variant) in &variants {
        report(
            &format!("open, {name} ({variant})"),
            Connection::open_with_flags(PathBuf::from(variant), READ_WRITE).and_then(
                |connection| {
                    connection.query_row("SELECT count(*) FROM t", [], |row| row.get::<_, i64>(0))
                },
            ),
        );
        let connection = Connection::open_in_memory().expect("connection");
        report(
            &format!("ATTACH, {name}"),
            connection
                .execute(&format!("ATTACH {} AS p", literal(variant)), [])
                .and_then(|_| {
                    connection.query_row("SELECT count(*) FROM p.t", [], |row| row.get::<_, i64>(0))
                }),
        );
    }
}

/// Lexical rules of the texts: comments, quotes of the strings and of the
/// names, and where `sqlite3_complete` (the end of a statement of the
/// `sqlite3` shell) sees a statement end, triggers included.
fn lexical_rules() {
    println!("\n10. Lexical rules");
    let connection = Connection::open_in_memory().expect("connection");
    let text = |sql: &str| {
        connection.query_row(sql, [], |row| {
            row.get_ref(0).map(|value| {
                let shown = match value {
                    ValueRef::Text(bytes) => String::from_utf8_lossy(bytes).into_owned(),
                    ValueRef::Integer(number) => number.to_string(),
                    other => format!("{other:?}"),
                };
                format!("{:?} {shown}", value.data_type())
            })
        })
    };
    for sql in [
        "SELECT 1 # comment",
        "SELECT 1 --comment",
        "SELECT 1 -- comment",
        "SELECT 1 /* comment",
        "SELECT 1 /* outer /* inner */ */",
        "SELECT 'a\\'",
        "SELECT 'it''s'",
        "SELECT \"a b\" FROM (SELECT 1 AS [a b])",
        "SELECT `a b` FROM (SELECT 1 AS [a b])",
        "SELECT [a b] FROM (SELECT 1 AS \"a b\")",
        "SELECT \"a\"\"b\" FROM (SELECT 1 AS [a\"b])",
        "SELECT [a]]b] FROM (SELECT 1 AS \"a]b\")",
        "SELECT \"no_such_column\"",
        "SELECT x'41'",
    ] {
        report(sql, text(sql));
    }
    let complete = |sql: &str| {
        let text = CString::new(sql).expect("no NUL");
        // SAFETY: a NUL-terminated text, read only by the call.
        unsafe { rusqlite::ffi::sqlite3_complete(text.as_ptr()) != 0 }
    };
    for sql in [
        "SELECT 1",
        "SELECT 1;",
        "SELECT ';'",
        "SELECT 1 -- ;",
        "SELECT 1 /* ; */",
        "SELECT [;]",
        "SELECT 'a\\';",
        "SELECT 1 # ;",
        "CREATE TRIGGER tr AFTER INSERT ON t BEGIN SELECT 1;",
        "CREATE TRIGGER tr AFTER INSERT ON t BEGIN SELECT 1; END",
        "CREATE TRIGGER tr AFTER INSERT ON t BEGIN SELECT 1; END;",
        "CREATE TEMP TRIGGER tr AFTER INSERT ON t BEGIN SELECT 1; END;",
        "CREATE TEMPORARY TRIGGER tr AFTER INSERT ON t BEGIN SELECT 1;",
        "EXPLAIN CREATE TRIGGER tr AFTER INSERT ON t BEGIN SELECT 1;",
        "CREATE TRIGGER tr AFTER INSERT ON t BEGIN SELECT CASE WHEN 1 THEN 2 END;",
        "CREATE TRIGGER tr AFTER INSERT ON t BEGIN SELECT CASE WHEN 1 THEN 2 END; END;",
        "CREATE TABLE trigger_log (a); SELECT 1;",
    ] {
        println!("  sqlite3_complete({sql:?}): {}", complete(sql));
    }
    connection
        .execute_batch("CREATE TABLE t (a); CREATE TABLE u (b);")
        .expect("tables");
    report(
        "CREATE TRIGGER whose body holds CASE ... END;",
        connection
            .execute_batch(
                "CREATE TRIGGER tr AFTER INSERT ON t BEGIN \
                 INSERT INTO u VALUES (CASE WHEN new.a > 0 THEN 'pos' END); END; \
                 INSERT INTO t VALUES (1);",
            )
            .and_then(|()| text("SELECT b FROM u")),
    );
}

/// `LIMIT` after a `VALUES`, alone or in a set operation: where Prisme can
/// add its default limit.
fn limit_after_values() {
    println!(
        "
11. LIMIT after VALUES"
    );
    let connection = Connection::open_in_memory().expect("connection");
    for sql in [
        "VALUES (1), (2) LIMIT 1",
        "SELECT 1 UNION VALUES (2) LIMIT 1",
        "VALUES (2) UNION SELECT 1 LIMIT 1",
        "SELECT * FROM (VALUES (1), (2)) LIMIT 1",
    ] {
        report(
            sql,
            connection.prepare(sql).and_then(|mut statement| {
                statement.query_map([], |_| Ok(())).map(|rows| rows.count())
            }),
        );
    }
}

/// Text of a value: a text as such, the other values with their type.
fn shown(value: ValueRef<'_>) -> String {
    match value {
        ValueRef::Text(bytes) => String::from_utf8_lossy(bytes).into_owned(),
        ValueRef::Integer(number) => number.to_string(),
        other => format!("{other:?}"),
    }
}

/// Pragmas: the forms of their value, an unknown name, their functions in a
/// `SELECT`, and those that write on a read-only file; where Prisme's list
/// of the pragmas (`crates/dialects/src/sqlite_pragmas.rs`) gives each its
/// role.
fn pragmas(folder: &Path) {
    println!(
        "
12. Pragmas"
    );
    let path = folder.join("pragmas.db");
    create_database(&path);
    let connection = Connection::open_with_flags(&path, READ_WRITE).expect("connection");
    let rows = |connection: &Connection, sql: &str| {
        connection.prepare(sql).and_then(|mut statement| {
            let columns = statement.column_count();
            statement
                .query_map([], |row| {
                    (0..columns)
                        .map(|index| row.get_ref(index).map(shown))
                        .collect::<rusqlite::Result<Vec<_>>>()
                })?
                .collect::<rusqlite::Result<Vec<_>>>()
        })
    };
    let names: Vec<String> = rows(
        &connection,
        "SELECT name FROM pragma_pragma_list ORDER BY name",
    )
    .expect("pragma_list")
    .into_iter()
    .map(|row| row[0].clone())
    .collect();
    println!(
        "  pragma_list: {} pragmas: {}",
        names.len(),
        names.join(" ")
    );
    // Pragmas that a function of their name gives in a SELECT.
    let functions: Vec<&str> = names
        .iter()
        .filter(|name| {
            connection
                .prepare(&format!("SELECT * FROM pragma_{name}"))
                .is_ok()
        })
        .map(String::as_str)
        .collect();
    println!(
        "  functions pragma_*: {}: {}",
        functions.len(),
        functions.join(" ")
    );
    for sql in [
        "PRAGMA no_such_pragma",
        "PRAGMA no_such_pragma = 1",
        "PRAGMA table_info = t",
        "PRAGMA table_info(t)",
        "PRAGMA foreign_keys(1)",
        "PRAGMA foreign_keys",
        "PRAGMA foreign_keys = 0",
        "PRAGMA main.cache_size(-100)",
        "PRAGMA cache_size",
        "PRAGMA integrity_check(1)",
        "PRAGMA journal_mode",
        "SELECT * FROM pragma_table_info('t')",
        "SELECT * FROM pragma_journal_mode",
        "SELECT * FROM pragma_journal_mode('wal')",
        "SELECT * FROM pragma_foreign_keys(1)",
        "SELECT * FROM pragma_user_version",
        "SELECT * FROM pragma_optimize",
        "SELECT * FROM pragma_wal_checkpoint",
        "SELECT * FROM pragma_incremental_vacuum",
        "SELECT * FROM pragma_integrity_check",
    ] {
        report(sql, rows(&connection, sql));
    }
    drop(connection);
    let connection = Connection::open_with_flags(&path, READ_ONLY).expect("connection");
    println!("  On a read-only connection:");
    for sql in [
        "PRAGMA optimize",
        "PRAGMA wal_checkpoint",
        "PRAGMA incremental_vacuum",
        "PRAGMA user_version = 3",
        "PRAGMA foreign_keys = 1",
        "PRAGMA cache_size = 10",
    ] {
        report(sql, rows(&connection, sql));
    }
    let uri = format!(
        "file:{}?mode=rw",
        path.display().to_string().replace('\\', "/")
    );
    let attach = format!("ATTACH {} AS other", literal(&uri));
    report(&attach, connection.execute_batch(&attach));
    report(
        "INSERT INTO other.t (name) VALUES ('two')",
        connection.execute("INSERT INTO other.t (name) VALUES ('two')", []),
    );
}

/// What `changes()` and `last_insert_rowid()` report after each kind of
/// write: the outcome the connector shows (`crates/connectors/src/sqlite`).
fn write_outcomes() {
    println!(
        "
13. Outcome of a write"
    );
    let connection = Connection::open_in_memory().expect("connection");
    connection
        .execute_batch(
            "CREATE TABLE t (id INTEGER PRIMARY KEY, k TEXT UNIQUE, v INTEGER);
             CREATE TABLE log (x);
             CREATE TRIGGER tr AFTER DELETE ON t BEGIN INSERT INTO log VALUES (old.id); END;",
        )
        .expect("tables");
    for sql in [
        "INSERT INTO t (k, v) VALUES ('a', 1), ('b', 2), ('c', 3)",
        "INSERT INTO t (k, v) VALUES ('d', 4)",
        "INSERT OR IGNORE INTO t (k, v) VALUES ('a', 9), ('e', 5)",
        "INSERT OR REPLACE INTO t (k, v) VALUES ('a', 10), ('f', 6)",
        "INSERT INTO t (k, v) VALUES ('b', 20), ('g', 7) ON CONFLICT (k) DO UPDATE SET v = excluded.v",
        "INSERT INTO t (k, v) VALUES ('b', 20) ON CONFLICT (k) DO UPDATE SET v = excluded.v",
        "UPDATE t SET v = v",
        "UPDATE t SET v = 0 WHERE k = 'zzz'",
        "UPDATE OR REPLACE t SET k = 'c' WHERE k = 'd'",
        "DELETE FROM t WHERE k = 'g'",
        "INSERT INTO t (k, v) VALUES ('h', 8) RETURNING id",
        "CREATE TABLE u (a)",
        "CREATE TABLE v AS SELECT * FROM t",
        "SELECT 1",
        "DELETE FROM t",
    ] {
        let before = connection.last_insert_rowid();
        let total = connection.total_changes();
        let outcome = connection.prepare(sql).and_then(|mut statement| {
            let rows = statement.query_map([], |_| Ok(()))?.count();
            Ok(rows)
        });
        match outcome {
            Ok(rows) => println!(
                "  {sql}: rows {rows}, changes {}, total_changes +{}, last_insert_rowid {before} -> {}",
                connection.changes(),
                connection.total_changes() - total,
                connection.last_insert_rowid()
            ),
            Err(error) => println!("  {sql}: error \"{error}\""),
        }
    }
}

/// How the connector opens its databases: the flags a database in memory
/// takes, `ATTACH` whose file and schema are parameters, the schemas
/// `database_list` gives, an interruption sent while nothing runs.
fn connector_opening(folder: &Path) {
    println!(
        "
14. Opening of the connector"
    );
    for (label, flags) in [
        ("read-write without CREATE", READ_WRITE),
        ("read-only", READ_ONLY),
    ] {
        let uri = format!("file:/prisme-opening-{}?vfs=memdb", label.len());
        report(
            &format!("new memdb database opened {label}, then a table created"),
            Connection::open_with_flags(&uri, flags)
                .and_then(|connection| connection.execute_batch("CREATE TABLE m (x)")),
        );
        report(
            &format!(":memory: opened {label}, then a table created"),
            Connection::open_with_flags(":memory:", flags)
                .and_then(|connection| connection.execute_batch("CREATE TABLE m (x)")),
        );
    }
    let writer = Connection::open_with_flags("file:/prisme-opening-ro?vfs=memdb", CREATE)
        .expect("memdb writer");
    writer
        .execute_batch("CREATE TABLE m (x); INSERT INTO m VALUES (1)")
        .expect("table");
    report(
        "existing memdb database opened read-only: rows read",
        Connection::open_with_flags("file:/prisme-opening-ro?vfs=memdb", READ_ONLY).and_then(
            |reader| reader.query_row("SELECT count(*) FROM m", [], |row| row.get::<_, i64>(0)),
        ),
    );
    report(
        "the same, then INSERT",
        Connection::open_with_flags("file:/prisme-opening-ro?vfs=memdb", READ_ONLY)
            .and_then(|reader| reader.execute_batch("INSERT INTO m VALUES (2)")),
    );
    drop(writer);

    let path = folder.join("opening.db");
    create_database(&path);
    let connection = Connection::open_in_memory().expect("connection");
    report(
        "ATTACH ?1 AS ?2 (file and schema bound)",
        connection.execute(
            "ATTACH ?1 AS ?2",
            [path.to_str().expect("path"), "other db"],
        ),
    );
    report(
        "rows of the schema bound",
        connection.query_row("SELECT count(*) FROM \"other db\".t", [], |row| {
            row.get::<_, i64>(0)
        }),
    );
    let list = |connection: &Connection| {
        connection
            .prepare("SELECT seq, name, file FROM pragma_database_list")
            .and_then(|mut statement| {
                statement
                    .query_map([], |row| {
                        Ok(format!(
                            "{} {} {:?}",
                            row.get::<_, i64>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?
                        ))
                    })?
                    .collect::<rusqlite::Result<Vec<_>>>()
            })
    };
    report(
        "database_list before any temporary table",
        list(&connection),
    );
    connection
        .execute_batch("CREATE TEMP TABLE x (a)")
        .expect("temporary table");
    report("database_list after a temporary table", list(&connection));
    report(
        "table list of a schema (pragma_table_list)",
        connection
            .prepare("SELECT schema, name, type, wr FROM pragma_table_list ORDER BY schema, name")
            .and_then(|mut statement| {
                statement
                    .query_map([], |row| {
                        Ok(format!(
                            "{}.{} {} {}",
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, i64>(3)?
                        ))
                    })?
                    .collect::<rusqlite::Result<Vec<_>>>()
            }),
    );
    let in_memdb = Connection::open_with_flags("file:/prisme-opening-vfs?vfs=memdb", READ_WRITE)
        .expect("memdb main");
    let plain = path.to_str().expect("path");
    let vfs = if cfg!(windows) { "win32" } else { "unix" };
    let uri = format!(
        "file:///{}?vfs={vfs}",
        plain.replace('\\', "/").trim_start_matches('/')
    );
    for (label, file) in [("path", plain.to_string()), ("URI naming its vfs", uri)] {
        report(
            &format!("main in memdb, ATTACH of the {label}: rows of t"),
            in_memdb
                .execute("ATTACH ?1 AS f", [file.as_str()])
                .and_then(|_| {
                    in_memdb.query_row("SELECT count(*) FROM f.t", [], |row| row.get::<_, i64>(0))
                }),
        );
        in_memdb.execute_batch("DETACH f").ok();
    }
    if cfg!(windows) {
        let unc = format!(
            "//localhost/{}",
            plain.replacen(':', "$", 1).replace('\\', "/")
        );
        let uri = format!("file://{unc}?vfs=win32");
        report(
            &format!("ATTACH of the UNC path {unc} as a URI file:////..."),
            in_memdb
                .execute("ATTACH ?1 AS u", [uri.as_str()])
                .and_then(|_| {
                    in_memdb.query_row("SELECT count(*) FROM u.t", [], |row| row.get::<_, i64>(0))
                }),
        );
    }
    let file_uri = format!(
        "file:///{}?vfs={vfs}",
        plain.replace('\\', "/").trim_start_matches('/')
    );
    for (schema, file) in [
        ("file", file_uri.as_str()),
        ("private", ":memory:"),
        ("shared", "file:/prisme-opening-vfs-shared?vfs=memdb"),
        ("plain", plain),
    ] {
        in_memdb
            .execute("ATTACH ?1 AS ?2", [file, schema])
            .expect("attachment");
    }
    report(
        "main in memdb, database_list of a file, a private database in memory, a shared one (memdb) and a plain path",
        list(&in_memdb),
    );
    drop(in_memdb);
    connection.get_interrupt_handle().interrupt();
    report(
        "SELECT 1 after an interruption sent while nothing ran",
        connection.query_row("SELECT 1", [], |row| row.get::<_, i64>(0)),
    );
    report(
        "prepare of a statement followed by a comment",
        connection.prepare("SELECT 1; -- end").map(|_| ()),
    );
    report(
        "prepare of a comment alone",
        connection
            .prepare("-- nothing")
            .map(|statement| statement.column_count()),
    );
}

/// The rows the update hook (`sqlite3_update_hook`) reports for each kind of
/// write, beside `changes()`, and its cost on the insertion of a million
/// rows: a way, not taken, to tell the rows inserted from the rows updated.
fn update_hook() {
    println!(
        "
15. Update hook"
    );
    let mut connection = Connection::open_in_memory().expect("connection");
    connection
        .execute_batch(
            "CREATE TABLE t (id INTEGER PRIMARY KEY, k TEXT UNIQUE, v INTEGER);
             CREATE TABLE log (x);
             CREATE TRIGGER tr AFTER DELETE ON t BEGIN INSERT INTO log VALUES (old.id); END;
             CREATE TABLE w (k TEXT PRIMARY KEY, v INTEGER) WITHOUT ROWID;
             INSERT INTO t (k, v) VALUES ('a', 1), ('b', 2), ('c', 3);
             INSERT INTO w VALUES ('a', 1);",
        )
        .expect("tables");
    let seen = Arc::new(Mutex::new(Vec::new()));
    let hooked = seen.clone();
    connection
        .update_hook(Some(move |action: Action, _: &str, table: &str, _: i64| {
            let action = match action {
                Action::SQLITE_INSERT => "insert",
                Action::SQLITE_UPDATE => "update",
                Action::SQLITE_DELETE => "delete",
                _ => "other",
            };
            hooked
                .lock()
                .expect("rows")
                .push(format!("{action} {table}"));
        }))
        .expect("hook");
    for sql in [
        "INSERT INTO t (k, v) VALUES ('a', 1), ('d', 4) ON CONFLICT (k) DO UPDATE SET v = excluded.v",
        "INSERT OR REPLACE INTO t (k, v) VALUES ('c', 30), ('f', 6)",
        "INSERT OR IGNORE INTO t (k, v) VALUES ('a', 9), ('g', 7)",
        "UPDATE t SET v = v",
        "DELETE FROM t WHERE k = 'd'",
        "INSERT INTO w VALUES ('a', 1) ON CONFLICT (k) DO UPDATE SET v = 2",
        "INSERT OR REPLACE INTO w VALUES ('a', 3)",
        "DELETE FROM log",
    ] {
        match connection.execute(sql, []) {
            Ok(_) => println!(
                "  {sql}: changes {}, hook {:?}",
                connection.changes(),
                seen.lock().expect("rows")
            ),
            Err(error) => println!("  {sql}: error \"{error}\""),
        }
        seen.lock().expect("rows").clear();
    }

    for hooked in [false, true] {
        let mut connection = Connection::open_in_memory().expect("connection");
        connection
            .execute_batch("CREATE TABLE big (id INTEGER PRIMARY KEY, a INTEGER, b TEXT)")
            .expect("table");
        if hooked {
            let count = Arc::new(Mutex::new(0_u64));
            connection
                .update_hook(Some(move |_: Action, _: &str, _: &str, _: i64| {
                    *count.lock().expect("count") += 1;
                }))
                .expect("hook");
        }
        let mut best = Duration::MAX;
        for _ in 0..5 {
            connection.execute("DELETE FROM big", []).expect("delete");
            let started = Instant::now();
            connection
                .execute(
                    "WITH RECURSIVE s(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM s WHERE x < 1000000)
                     INSERT INTO big (a, b) SELECT x, 'row ' || x FROM s",
                    [],
                )
                .expect("insert");
            best = best.min(started.elapsed());
        }
        println!(
            "  1,000,000 rows inserted, hook {}: best of 5 {} ms",
            if hooked { "set" } else { "absent" },
            best.as_millis()
        );
    }
}
