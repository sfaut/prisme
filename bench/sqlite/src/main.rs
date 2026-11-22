//! Measures of the SQLite embedded in Prisme, before its connector is written
//! (`docs/TODO-SQLITE.md`, step 0). Each measure runs the library as the
//! connector will, and prints what it does: read-only inherited by an
//! attachment, files created by an attachment, databases in memory shared
//! between connections (`memdb`), text of the real numbers, interruption,
//! transactions after an error, detaching while in use, paths on Windows,
//! values kept by `NUMERIC` and `REAL` columns. The results go to `docs/SQL-COMPARISON.md`, section "SQLite embedded in
//! Prisme".
//!
//! From the root of the repository:
//! `cargo run --release --manifest-path bench/sqlite/Cargo.toml --target-dir target/sqlite-measures`

use std::fmt::Debug;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

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

/// Values written into a `NUMERIC` and a `REAL` column: the storage class
/// SQLite keeps, and the text it gives for them.
fn numeric_affinity() {
    println!("\n9. NUMERIC and REAL columns");
    let connection = Connection::open_in_memory().expect("connection");
    connection
        .execute_batch("CREATE TABLE n (written TEXT, numeric_value NUMERIC, real_value REAL)")
        .expect("table");
    let values = [
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
                    "INSERT INTO n VALUES ({}, {value}, {value})",
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
            "SELECT written, typeof(numeric_value), CAST(numeric_value AS TEXT), \
             typeof(real_value), CAST(real_value AS TEXT) FROM n",
        )
        .expect("prepare");
    let rows = statement
        .query_map([], |row| {
            Ok([0, 1, 2, 3, 4].map(|index| row.get::<_, String>(index).unwrap_or_default()))
        })
        .expect("query");
    for row in rows {
        let [written, numeric_class, numeric_text, real_class, real_text] = row.expect("row");
        println!(
            "  {written}: NUMERIC {numeric_class} {numeric_text}, REAL {real_class} {real_text}"
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
