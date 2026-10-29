//! Tests of the export routes: the table and the folder read together,
//! removals, download, then exports on a real MariaDB server
//! (`testcontainers`).

use std::fs::File;
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use secrecy::SecretString;
use serde_json::{Value, json};
use tempfile::TempDir;
use testcontainers_modules::mariadb::Mariadb;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use tower::ServiceExt;

use super::table::{self, NewExport};
use super::{ExportState, interrupt_unfinished};
use crate::api::{AppState, router};
use crate::connections::{self, NewConnection};
use crate::master_key::MasterKey;
use crate::store;

/// Application whose exports go to a temporary folder.
struct TestApp {
    /// Router of the API.
    app: Router,
    /// State of the application, to read its internal database.
    state: AppState,
    /// Exports folder.
    folder: PathBuf,
    /// Temporary folder holding the exports folder, deleted with the test.
    _data: TempDir,
}

impl TestApp {
    /// Application with, for its user, a connection to `host:port` (root
    /// account without a password, `test` database) and the exports that
    /// `prepare` saves in its internal database.
    fn new(host: &str, port: u16, prepare: impl FnOnce(&rusqlite::Connection, i64)) -> Self {
        let db = store::open_in_memory();
        let key = MasterKey::generate();
        let user_id = store::ensure_user(&db, "test@example.com").unwrap();
        let connection = NewConnection {
            name: "test".to_string(),
            host: host.to_string(),
            port,
            username: "root".to_string(),
            password: SecretString::from(""),
            database_name: "test".to_string(),
            init_sql: String::new(),
            session_timeout_minutes: 5,
            show_system_schemas: true,
        };
        connections::insert(&db, &key, user_id, &connection).unwrap();
        prepare(&db, user_id);
        let data = tempfile::tempdir().unwrap();
        let folder = data.path().join(super::FOLDER);
        std::fs::create_dir(&folder).unwrap();
        let state = AppState::new(db, key, user_id, folder.clone());
        Self {
            app: router(state.clone()),
            state,
            folder,
            _data: data,
        }
    }

    /// Sends a request and returns the status and the JSON body (Null if empty).
    async fn send(&self, method: &str, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
        let response = self.request(method, uri, body).await;
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    /// Sends a request and returns the response.
    async fn request(
        &self,
        method: &str,
        uri: &str,
        body: Option<Value>,
    ) -> axum::response::Response {
        let request = Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json")
            .header("host", "localhost:8484")
            .body(body.map_or_else(Body::empty, |json| Body::from(json.to_string())))
            .unwrap();
        self.app.clone().oneshot(request).await.unwrap()
    }

    /// Export `id`, as listed.
    async fn export(&self, id: i64) -> Value {
        let (status, list) = self.send("GET", "/api/exports", None).await;
        assert_eq!(status, StatusCode::OK);
        let exports = list["exports"].as_array().unwrap();
        exports
            .iter()
            .find(|export| export["id"] == id)
            .unwrap()
            .clone()
    }

    /// Rows of export `id` saved in the internal database.
    async fn saved_rows(&self, id: i64) -> u64 {
        let user_id = self.state.local_user_id;
        self.state
            .with_db(move |db, _| {
                let exports = table::list(db, user_id)?;
                let saved = exports.iter().find(|(export, _)| export.id == id);
                Ok::<_, super::ExportsError>(saved.map_or(0, |(export, _)| export.rows))
            })
            .await
            .unwrap()
    }

    /// Waits until export `id` satisfies `until`, and returns it.
    async fn wait_for(&self, id: i64, until: impl Fn(&Value) -> bool) -> Value {
        for _ in 0..600 {
            let export = self.export(id).await;
            if until(&export) {
                return export;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        panic!("export {id}: expected state not reached");
    }
}

/// Saves an ended export of `user_id` whose file is `file`, and returns its id.
fn saved_export(db: &rusqlite::Connection, user_id: i64, millis: u128) -> (i64, String) {
    let new = NewExport {
        connection_name: "test",
        name: "ventes.csv",
        sql: "SELECT 1",
        expected_rows: None,
    };
    let (export, file) = table::insert(db, user_id, &new, millis).unwrap();
    table::finish(db, export.id, ExportState::Ready, 1, 3, None).unwrap();
    (export.id, file)
}

/// Writes `content` to file `name` of `folder`.
fn write(folder: &Path, name: &str, content: &str) {
    std::fs::write(folder.join(name), content).unwrap();
}

/// Application with two ended exports, the file of the first one in the
/// folder, that of the second one gone, a file of Prisme that no export
/// references and a file of another program. Returns the application, the
/// two exports and their files.
fn app_with_exports() -> (TestApp, [(i64, String); 2]) {
    let mut exports = Vec::new();
    let test = TestApp::new("127.0.0.1", 1, |db, user_id| {
        exports.push(saved_export(db, user_id, 1));
        exports.push(saved_export(db, user_id, 2));
    });
    write(&test.folder, &exports[0].1, "a;b\r\n");
    write(&test.folder, "99-3.csv", "orphan");
    write(&test.folder, "notes.txt", "kept");
    (test, exports.try_into().unwrap())
}

#[tokio::test]
async fn lists_the_exports_with_the_files_of_the_folder() {
    let (test, [(present, _), (missing, _)]) = app_with_exports();
    let (status, list) = test.send("GET", "/api/exports", None).await;
    assert_eq!(status, StatusCode::OK);
    // The most recent first.
    assert_eq!(list["exports"][0]["id"], missing);
    assert_eq!(list["exports"][0]["file_missing"], true);
    assert_eq!(list["exports"][1]["id"], present);
    assert_eq!(list["exports"][1]["file_missing"], false);
    assert_eq!(list["exports"][1]["state"], "ready");
    // The size of the file, the table's without one.
    assert_eq!(list["exports"][1]["bytes"], 5);
    assert_eq!(list["exports"][0]["bytes"], 3);
    // The file of the other program is not counted.
    assert_eq!(list["orphans"], json!({ "count": 1, "bytes": 6 }));
}

#[tokio::test]
async fn removes_an_export_and_its_file_even_when_gone() {
    let (test, [(present, file), (missing, _)]) = app_with_exports();
    let (status, _) = test
        .send("DELETE", &format!("/api/exports/{present}"), None)
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(!test.folder.join(file).exists());
    let (status, _) = test
        .send("DELETE", &format!("/api/exports/{missing}"), None)
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = test
        .send("DELETE", &format!("/api/exports/{missing}"), None)
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (_, list) = test.send("GET", "/api/exports", None).await;
    assert_eq!(list["exports"], json!([]));
    assert_eq!(list["orphans"]["count"], 1);
}

#[tokio::test]
async fn deletes_only_the_files_that_no_export_references() {
    let (test, [(_, file), _]) = app_with_exports();
    let (status, _) = test.send("DELETE", "/api/exports/orphans", None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(!test.folder.join("99-3.csv").exists());
    assert!(test.folder.join(file).exists());
    assert!(test.folder.join("notes.txt").exists());
}

#[tokio::test]
async fn downloads_the_file_of_an_export() {
    let (test, [(present, _), (missing, file)]) = app_with_exports();
    let response = test
        .request("GET", &format!("/api/exports/{present}/file"), None)
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let headers = response.headers();
    assert_eq!(headers["content-type"], "text/csv; charset=utf-8");
    assert_eq!(headers["content-length"], "5");
    assert_eq!(
        headers["content-disposition"],
        "attachment; filename=\"ventes.csv\"; filename*=UTF-8''ventes.csv"
    );
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(&body[..], b"a;b\r\n");

    let (status, body) = test
        .send("GET", &format!("/api/exports/{missing}/file"), None)
        .await;
    assert_eq!(status, StatusCode::GONE);
    assert_eq!(
        body["detail"],
        json!({ "code": "export_file_missing", "file": file })
    );
    let (status, _) = test.send("GET", "/api/exports/999/file", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn refuses_an_invalid_export_before_connecting() {
    let (test, [(ended, _), _]) = app_with_exports();
    let request = |name: &str, connection_id: i64| json!({ "connection_id": connection_id, "sql": "SELECT 1", "columns": [], "name": name });
    let (status, body) = test
        .send("POST", "/api/exports", Some(request("", 1)))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["detail"]["code"], "field_length");
    let (status, _) = test
        .send("POST", "/api/exports", Some(request("a.csv", 999)))
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Stopping an ended export changes nothing.
    let (status, _) = test
        .send("POST", &format!("/api/exports/{ended}/cancel"), None)
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(test.export(ended).await["state"], "ready");
    let (status, _) = test.send("POST", "/api/exports/999/cancel", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

/// Running export of `user_id`, its progress saved at `rows` rows and
/// `bytes` bytes; returns its id and its file.
fn running_export(db: &rusqlite::Connection, user_id: i64, rows: u64, bytes: u64) -> (i64, String) {
    let new = NewExport {
        connection_name: "test",
        name: "ventes.csv",
        sql: "SELECT 1",
        expected_rows: None,
    };
    let (export, file) = table::insert(db, user_id, &new, 1).unwrap();
    table::progress(db, export.id, rows, bytes).unwrap();
    (export.id, file)
}

#[test]
fn interrupts_the_running_exports_with_the_rows_of_their_files() {
    let db = store::open_in_memory();
    let user_id = store::ensure_user(&db, "test@example.com").unwrap();
    let folder = tempfile::tempdir().unwrap();
    let start = "\u{FEFF}n\r\n1\r\n";
    // Saved after its first row, then 2 rows written, the last one cut short.
    let (saved, saved_file) = running_export(&db, user_id, 1, start.len() as u64);
    write(folder.path(), &saved_file, &format!("{start}2\r\n3\r\n4"));
    let last_write = UNIX_EPOCH + Duration::from_millis(1_792_500_000_123);
    File::options()
        .write(true)
        .open(folder.path().join(&saved_file))
        .unwrap()
        .set_modified(last_write)
        .unwrap();
    // Never saved: its file starts with the line of the names.
    let (unsaved, unsaved_file) = running_export(&db, user_id, 0, 0);
    write(folder.path(), &unsaved_file, start);
    // Its file gone: its saved counters.
    let (gone, _) = running_export(&db, user_id, 7, 70);

    assert_eq!(interrupt_unfinished(&db, folder.path()).unwrap(), 3);
    let exports = table::list(&db, user_id).unwrap();
    let export_of = |id| {
        &exports
            .iter()
            .find(|(export, _)| export.id == id)
            .unwrap()
            .0
    };
    let saved = export_of(saved);
    assert_eq!(saved.state, ExportState::Interrupted);
    assert_eq!((saved.rows, saved.bytes), (3, start.len() as u64 + 7));
    assert_eq!(saved.ended_at.as_deref(), Some("2026-10-20T12:40:00.123Z"));
    assert_eq!(export_of(unsaved).rows, 1);
    let gone = export_of(gone);
    assert_eq!((gone.rows, gone.bytes), (7, 70));
    assert!(gone.ended_at.is_some());
    // Their files stay.
    assert!(folder.path().join(&saved_file).exists());
}

/// Request of an export of `sql` on the connection of the test application.
fn export_request(sql: &str, columns: &[u32]) -> Value {
    json!({ "connection_id": 1, "sql": sql, "columns": columns, "name": "export.csv" })
}

#[tokio::test]
#[ignore = "requires Docker Desktop running"]
async fn exports_a_full_result_in_the_displayed_order_then_removes_it() {
    let server = Mariadb::default().start().await.unwrap();
    let host = server.get_host().await.unwrap().to_string();
    let port = server.get_host_port_ipv4(3306).await.unwrap();
    let test = TestApp::new(&host, port, |_, _| {});

    let sql = "SELECT seq, seq / 2 AS half FROM seq_1_to_200000";
    let (status, started) = test
        .send("POST", "/api/exports", Some(export_request(sql, &[1, 0])))
        .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(started["state"], "running");
    let id = started["id"].as_i64().unwrap();
    let ended = test
        .wait_for(id, |export| export["state"] != "running")
        .await;
    assert_eq!(ended["state"], "ready", "{ended}");
    assert_eq!(ended["rows"], 200_000);

    let files: Vec<PathBuf> = std::fs::read_dir(&test.folder)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    assert_eq!(files.len(), 1);
    let bytes = std::fs::read(&files[0]).unwrap();
    assert_eq!(ended["bytes"], bytes.len());
    let text = String::from_utf8(bytes).unwrap();
    let lines: Vec<&str> = text.split("\r\n").collect();
    // Decimal comma for the DECIMAL column, columns in the requested order.
    assert_eq!(lines[..3], ["\u{FEFF}half;seq", "0,5000;1", "1,0000;2"]);
    assert_eq!(lines[200_000..], ["100000,0000;200000", ""]);

    let (status, _) = test
        .send("DELETE", &format!("/api/exports/{id}"), None)
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(!files[0].exists());
}

// Several threads, as in Prisme: the connection of the stopped export
// closes in the background, while the test goes on.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires Docker Desktop running"]
async fn stops_refuses_and_fails_exports() {
    let server = Mariadb::default().start().await.unwrap();
    let host = server.get_host().await.unwrap().to_string();
    let port = server.get_host_port_ipv4(3306).await.unwrap();
    let test = TestApp::new(&host, port, |_, _| {});

    // A billion rows: stopped while they are written, the query interrupted
    // on the server (otherwise the rest of the result would be read), the
    // partial file kept.
    let sql = "SELECT seq FROM seq_1_to_1000000000";
    let (_, started) = test
        .send("POST", "/api/exports", Some(export_request(sql, &[])))
        .await;
    let id = started["id"].as_i64().unwrap();
    // The progress is saved while the export runs.
    let mut saved = 0;
    for _ in 0..600 {
        saved = test.saved_rows(id).await;
        if saved > 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(saved > 0, "progress of export {id} never saved");
    let (status, _) = test
        .send("POST", &format!("/api/exports/{id}/cancel"), None)
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let stopped = test
        .wait_for(id, |export| export["state"] != "running")
        .await;
    assert_eq!(stopped["state"], "cancelled", "{stopped}");
    assert!(stopped["rows"].as_u64() < Some(1_000_000_000));
    assert_eq!(stopped["file_missing"], false);

    let refusals = [
        ("DELETE FROM t", "export_not_read"),
        ("SELECT 1; SELECT 2", "multiple_queries"),
        ("SELECT 1 +", "unparsed_query_refused"),
    ];
    for (sql, code) in refusals {
        let request = export_request(sql, &[]);
        let (status, body) = test.send("POST", "/api/exports", Some(request)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{sql}");
        assert_eq!(body["detail"]["code"], code, "{sql}");
    }

    // Columns of the same name: the export starts, then fails on them.
    let (status, started) = test
        .send(
            "POST",
            "/api/exports",
            Some(export_request("SELECT 1 AS a, 2 AS a", &[])),
        )
        .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let id = started["id"].as_i64().unwrap();
    let failed = test
        .wait_for(id, |export| export["state"] != "running")
        .await;
    assert_eq!(failed["state"], "failed");
    assert_eq!(
        failed["error"],
        json!({ "code": "duplicate_columns", "names": ["a"] })
    );
}
