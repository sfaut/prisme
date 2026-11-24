//! Helpers of the tests that run queries through a query session: a
//! connection of SQLite in memory or on files, a database to attach, the
//! writes allowed to a user, an
//! application served on a free port and the queries sent over the
//! WebSocket of a session.

use std::net::SocketAddr;
use std::path::Path;

use axum::Router;
use futures_util::{SinkExt, StreamExt};
use prisme_connectors::api::Engine;
use prisme_dialects::classify::WriteKind;
use serde_json::{Value, json};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

use crate::connections::{NewConnection, NewFileDatabase, NewFilesSettings, NewSettings};
use crate::settings::{self, SettingsUpdate};

/// WebSocket of a query session, on the client side.
pub type ClientSocket = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// Connection of SQLite to a main database in memory.
pub fn memory_connection() -> NewConnection {
    NewConnection {
        name: "memory".to_string(),
        engine: Engine::Sqlite,
        settings: NewSettings::Files(NewFilesSettings {
            databases: vec![NewFileDatabase {
                schema: "main".to_string(),
                location: ":memory:".to_string(),
                create: false,
            }],
        }),
        init_sql: String::new(),
        session_timeout_minutes: 5,
        show_system_schemas: true,
    }
}

/// Connection of SQLite to a main database in the file `main.db` of
/// `folder`, created empty, and a database in memory attached as `scratch`.
pub fn files_connection(folder: &Path) -> NewConnection {
    let main = folder.join("main.db");
    rusqlite::Connection::open(&main).unwrap();
    let database = |schema: &str, location: String| NewFileDatabase {
        schema: schema.to_string(),
        location,
        create: false,
    };
    NewConnection {
        name: "files".to_string(),
        settings: NewSettings::Files(NewFilesSettings {
            databases: vec![
                database("main", main.to_string_lossy().into_owned()),
                database("scratch", ":memory:".to_string()),
            ],
        }),
        ..memory_connection()
    }
}

/// Creates a database of SQLite in `folder`, named `name`, holding a table
/// `u` of three rows, and returns the absolute path of its file.
pub fn attachable_database(folder: &Path, name: &str) -> String {
    let path = folder.join(format!("{name}.db"));
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute_batch("CREATE TABLE u (a); INSERT INTO u VALUES (1), (2), (3)")
        .unwrap();
    path.to_string_lossy().into_owned()
}

/// Makes the sessions of user `user_id` read-write, allowing `writes`.
pub fn allow_writes(db: &rusqlite::Connection, user_id: i64, writes: &[WriteKind]) {
    let current = settings::load(db, user_id).unwrap();
    let update = SettingsUpdate {
        row_limit: current.row_limit,
        read_only_session: false,
        allowed_writes: writes.to_vec(),
        history_retention_days: current.history_retention_days,
        debug_retention_days: current.debug_retention_days,
        indent_size: current.indent_size,
        concat_max_bytes: current.concat_max_bytes,
        pivot_refresh_ms: current.pivot_refresh_ms,
        remote_connect_timeout_seconds: current.remote_connect_timeout_seconds,
        remote_download_timeout_minutes: current.remote_download_timeout_minutes,
        remote_max_mebibytes: current.remote_max_mebibytes,
        remote_cache_days: current.remote_cache_days,
    };
    settings::update(db, user_id, &update).unwrap();
}

/// Serves `app` on a free port of the loopback, and returns its address.
pub async fn serve(app: Router) -> SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    address
}

/// Runs `sql` under the cursor on the session of `socket`, until the end of
/// its batch; an error fails the test.
pub async fn run_on_session(socket: &mut ClientSocket, sql: &str) {
    let request = json!({ "type": "run", "sql": sql, "cursor": 0 });
    socket
        .send(Message::Text(request.to_string().into()))
        .await
        .unwrap();
    while let Some(message) = socket.next().await {
        if let Message::Text(text) = message.unwrap() {
            let message: Value = serde_json::from_str(&text).unwrap();
            assert_ne!(message["type"], "error", "{sql}: {message}");
            if message["type"] == "batch_done" {
                return;
            }
        }
    }
}
