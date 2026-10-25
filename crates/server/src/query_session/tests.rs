//! Tests of the query session, on a real MariaDB server (`testcontainers`).

use futures_util::{SinkExt, StreamExt};
use secrecy::SecretString;
use serde_json::{Value, json};
use testcontainers_modules::mariadb::Mariadb;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message as ClientMessage;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

use crate::api::{AppState, router};
use crate::connections::{self, NewConnection};
use crate::editors::{self, NewEditor};
use crate::error_detail::ErrorDetail;
use crate::master_key::MasterKey;
use crate::settings::{self, SettingsUpdate};
use crate::store;
use prisme_dialects::Dialect;
use prisme_dialects::classify::WriteKind;
use std::collections::BTreeSet;

type ClientSocket = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// Starts Prisme on a free port, with a saved connection to `host:port`
/// (root account without a password, `test` database) and an editor on
/// this connection, and returns the server's address, the id of the
/// connection and that of the editor. The sessions are read-only.
async fn start(host: &str, port: u16) -> (String, i64, i64) {
    start_writing(host, port, None).await
}

/// Starts Prisme as `start` does; with `writes`, the sessions are not
/// read-only and allow these writes.
async fn start_writing(host: &str, port: u16, writes: Option<&[WriteKind]>) -> (String, i64, i64) {
    let db = store::open_in_memory();
    let key = MasterKey::generate();
    let user_id = store::ensure_user(&db, "test@example.com").unwrap();
    if let Some(writes) = writes {
        let current = settings::load(&db, user_id).unwrap();
        let update = SettingsUpdate {
            row_limit: current.row_limit,
            read_only_session: false,
            allowed_writes: writes.to_vec(),
            history_retention_days: current.history_retention_days,
            debug_retention_days: current.debug_retention_days,
            indent_size: current.indent_size,
        };
        settings::update(&db, user_id, &update).unwrap();
    }
    let new = NewConnection {
        name: "test".to_string(),
        host: host.to_string(),
        port,
        username: "root".to_string(),
        password: SecretString::from(""),
        database_name: "test".to_string(),
        // On-connect queries: the variable exists on each open connection,
        // and double quotes delimit identifiers there.
        init_sql: "SET @prisme_init = 1; SET SESSION sql_mode = 'ANSI_QUOTES'".to_string(),
        session_timeout_minutes: 5,
        show_system_schemas: true,
    };
    let id = connections::insert(&db, &key, user_id, &new).unwrap();
    let new_editor = NewEditor {
        connection_id: Some(id),
    };
    let editor = editors::create(&db, user_id, &new_editor).unwrap();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap().to_string();
    let app = router(AppState::new(db, key, user_id));
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (address, id, editor.id)
}

/// Reads the next text message of the server, as JSON.
async fn next_json(socket: &mut ClientSocket) -> Value {
    loop {
        if let ClientMessage::Text(text) = socket.next().await.unwrap().unwrap() {
            return serde_json::from_str(&text).unwrap();
        }
    }
}

/// Result of a query of a batch, as received by the client.
#[derive(Debug, Default)]
struct Received {
    /// `columns` message, or Null.
    columns: Value,
    /// Number of rows received.
    rows: usize,
    /// Number of rows of each `rows` message, in order.
    batch_sizes: Vec<usize>,
    /// Last message of the query: `done` or `error`.
    end: Value,
    /// Has the `started` message arrived?
    started: bool,
}

/// Sends a request, then reads the messages until `batch_done`.
/// Returns the `batch` message (or a session error) and the results, by query number.
async fn run(socket: &mut ClientSocket, mut request: Value) -> (Value, Vec<Received>) {
    request["type"] = json!("run");
    socket
        .send(ClientMessage::Text(request.to_string().into()))
        .await
        .unwrap();
    let mut batch = Value::Null;
    let mut results: Vec<Received> = Vec::new();
    loop {
        let message = next_json(socket).await;
        let kind = message["type"].as_str().unwrap().to_string();
        if kind == "batch_done" {
            return (batch, results);
        }
        if kind == "error" && message["query"].is_null() {
            return (message, results);
        }
        if kind == "transaction" {
            // State of the session, outside the results.
            continue;
        }
        if kind == "batch" {
            let count = message["queries"].as_array().unwrap().len();
            results = (0..count).map(|_| Received::default()).collect();
            batch = message;
            continue;
        }
        let result = &mut results[message["query"].as_u64().unwrap() as usize];
        match kind.as_str() {
            "started" => result.started = true,
            "columns" => result.columns = message,
            "rows" => {
                let size = message["rows"].as_array().unwrap().len();
                result.rows += size;
                result.batch_sizes.push(size);
            }
            _ => result.end = message,
        }
    }
}

/// Runs `sql` under the cursor and returns the `transaction` messages
/// received before `batch_done`, with the error of the query, if any.
async fn run_transaction(socket: &mut ClientSocket, sql: &str) -> (Vec<Value>, Value) {
    let request = json!({ "type": "run", "sql": sql, "cursor": 0 });
    socket
        .send(ClientMessage::Text(request.to_string().into()))
        .await
        .unwrap();
    let mut transactions = Vec::new();
    let mut error = Value::Null;
    loop {
        let message = next_json(socket).await;
        match message["type"].as_str().unwrap() {
            "batch_done" => return (transactions, error),
            "transaction" => transactions.push(message["open"].clone()),
            "error" => error = message["detail"]["code"].clone(),
            _ => {}
        }
    }
}

/// Runs all the queries of `sql`, without a cursor, and returns the number
/// of rows of each one.
async fn run_all_rows(socket: &mut ClientSocket, sql: &str) -> Vec<usize> {
    let (_, results) = run(socket, json!({ "sql": sql })).await;
    results.iter().map(|result| result.rows).collect()
}

/// Runs the query under the cursor (here, the only text sent) and returns its result.
async fn run_one(socket: &mut ClientSocket, sql: &str, row_limit: Option<u32>) -> Received {
    let mut request = json!({ "sql": sql, "cursor": 0 });
    if let Some(row_limit) = row_limit {
        request["row_limit"] = json!(row_limit);
    }
    run(socket, request).await.1.remove(0)
}

#[test]
fn chooses_the_statement_under_the_cursor_those_of_the_selection_or_all() {
    use super::execution::chosen_statements;
    use super::protocol::SelectionRange;

    // Queries at 0..9, 10..19 and 20..29.
    let statements = prisme_splitter::split("SELECT 1;\nSELECT 2;\nSELECT 3;");
    let sql = |chosen: Vec<&prisme_splitter::Statement>| {
        chosen
            .into_iter()
            .map(|statement| statement.sql.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        sql(chosen_statements(&statements, Some(12), &[])),
        ["SELECT 2"]
    );
    assert_eq!(
        sql(chosen_statements(&statements, None, &[])),
        ["SELECT 1", "SELECT 2", "SELECT 3"]
    );
    assert!(chosen_statements(&[], Some(0), &[]).is_empty());

    // Selected passages: the queries they touch, once each, in the order of the text.
    let range = |start, end| SelectionRange { start, end };
    assert_eq!(
        sql(chosen_statements(&statements, None, &[range(5, 12)])),
        ["SELECT 1", "SELECT 2"]
    );
    assert_eq!(
        sql(chosen_statements(
            &statements,
            None,
            &[range(25, 27), range(2, 3), range(1, 4)]
        )),
        ["SELECT 1", "SELECT 3"]
    );
    // A passage between two queries touches none.
    assert!(chosen_statements(&statements, None, &[range(9, 10)]).is_empty());
}

#[test]
fn runs_only_the_selected_text_when_asked() {
    use super::execution::selected_pieces;
    use super::protocol::SelectionRange;

    let text = "SELECT ALL id, name, unit_price
FROM product;
SELECT ALL id, name, email
FROM user
WHERE email LIKE '%.fr';";
    let statements = prisme_splitter::split(text);
    let range = |start, end| SelectionRange { start, end };
    let pieces = |selection: &[SelectionRange]| {
        selected_pieces(&statements, selection)
            .into_iter()
            .map(|piece| piece.sql)
            .collect::<Vec<_>>()
    };
    let user = text.find("user").unwrap() + 4;
    let second = text.find("SELECT ALL id, name, email").unwrap();
    // From the second SELECT to "user": without its WHERE clause.
    assert_eq!(
        pieces(&[range(second, user)]),
        ["SELECT ALL id, name, email
FROM user"]
    );
    // From the first SELECT to "user": both queries, the second one cut.
    assert_eq!(
        pieces(&[range(0, user)]),
        [
            "SELECT ALL id, name, unit_price
FROM product",
            "SELECT ALL id, name, email
FROM user"
        ]
    );
    // Passages in any order, overlapping ones merged; two separate passages
    // of a query give two queries.
    assert_eq!(
        pieces(&[range(second + 11, second + 13), range(0, 6), range(3, 10)]),
        ["SELECT ALL", "id"]
    );
    // Without a selection, nothing runs.
    assert!(pieces(&[]).is_empty());
}

#[test]
fn sends_a_short_first_batch_of_rows_then_full_ones() {
    let mut batches = super::execution::RowBatches::new();
    let sizes: Vec<usize> = (0..1_200)
        .filter_map(|_| batches.push(Vec::new()))
        .map(|batch| batch.len())
        .collect();
    assert_eq!(sizes, [50, 500, 500]);
    assert_eq!(batches.rest().map(|batch| batch.len()), Some(150));

    // A result shorter than the first batch goes in a single message.
    let mut batches = super::execution::RowBatches::new();
    assert!(batches.push(Vec::new()).is_none());
    assert_eq!(batches.rest().map(|batch| batch.len()), Some(1));
    assert!(super::execution::RowBatches::new().rest().is_none());
}

#[tokio::test]
async fn cancels_one_query_or_the_whole_batch() {
    // No running query: stopping does not contact the target server.
    let target = super::MysqlTarget {
        host: "127.0.0.1".to_string(),
        port: 1,
        username: "root".to_string(),
        password: secrecy::SecretString::from("secret"),
        database_name: String::new(),
        init_statements: Vec::new(),
    };
    let cancellation = super::Cancellation::default();

    // A stop sent right after its batch, before the batch starts, reaches it.
    let first = cancellation.receive_batch();
    cancellation.cancel(&target).await;
    assert!(cancellation.is_cancelled(first, 0));

    // A batch received after the stop is not affected.
    let second = cancellation.receive_batch();
    assert!(!cancellation.is_cancelled(second, 0));

    // A stop of one query reaches that query of the last batch only.
    cancellation.cancel_query(&target, 1).await;
    assert!(cancellation.is_cancelled(second, 1));
    assert!(!cancellation.is_cancelled(second, 0));
    let third = cancellation.receive_batch();
    assert!(!cancellation.is_cancelled(third, 1));
}

#[test]
fn refuses_the_writes_not_allowed_and_unparsable_queries() {
    let refusal = |sql: &str, writes: Option<&[WriteKind]>| {
        let writes: Option<BTreeSet<WriteKind>> =
            writes.map(|kinds| kinds.iter().copied().collect());
        super::limits::write_refusal(sql, Dialect::MariaDb, writes.as_ref(), &|_| false)
    };
    assert_eq!(refusal("SELECT 1", None), None);
    assert_eq!(refusal("SHOW TABLES", None), None);
    // A setting of the session, even read-only.
    assert_eq!(refusal("SET @a = 1", None), None);
    // The control of transactions, even read-only; not what would lift it.
    assert_eq!(refusal("START TRANSACTION", None), None);
    assert_eq!(refusal("COMMIT", None), None);
    assert_eq!(refusal("SET autocommit = 0", None), None);
    assert_eq!(
        refusal("SET SESSION TRANSACTION READ WRITE", None),
        Some(ErrorDetail::ReadWriteTransactionReadOnly)
    );
    assert_eq!(
        refusal("START TRANSACTION READ WRITE", None),
        Some(ErrorDetail::ReadWriteTransactionReadOnly)
    );
    assert_eq!(refusal("START TRANSACTION READ WRITE", Some(&[])), None);
    assert_eq!(
        refusal("SET GLOBAL TRANSACTION READ WRITE", Some(&WriteKind::ALL)),
        Some(ErrorDetail::WriteForbidden)
    );
    assert_eq!(
        refusal("XA START 'x1'", Some(&WriteKind::ALL)),
        Some(ErrorDetail::XaTransactionUnsupported)
    );
    // Read-only sessions: no write, whatever is allowed.
    assert_eq!(
        refusal("DELETE FROM t", None),
        Some(ErrorDetail::WriteReadOnly)
    );
    assert_eq!(
        refusal("WITH x AS (SELECT 1) DELETE FROM t", Some(&[])),
        Some(ErrorDetail::WriteNotAllowed {
            kinds: vec![WriteKind::Delete]
        })
    );
    assert_eq!(refusal("DELETE FROM t", Some(&[WriteKind::Delete])), None);
    // REPLACE needs both of its switches.
    assert_eq!(
        refusal("REPLACE INTO t VALUES (1)", Some(&[WriteKind::Insert])),
        Some(ErrorDetail::WriteNotAllowed {
            kinds: vec![WriteKind::Delete]
        })
    );
    assert_eq!(
        refusal("CALL p()", Some(&WriteKind::ALL)),
        Some(ErrorDetail::WriteForbidden)
    );
    assert_eq!(
        refusal("SELECT * FROM t PROCEDURE ANALYSE()", Some(&WriteKind::ALL)),
        Some(ErrorDetail::UnparsedQueryRefused)
    );
}

#[tokio::test]
async fn unreachable_server_sends_an_error() {
    // Port 1: no server listens there.
    let (address, id, _) = start("127.0.0.1", 1).await;
    let (mut socket, _) = connect_async(format!("ws://{address}/api/connections/{id}/query"))
        .await
        .unwrap();

    assert_eq!(next_json(&mut socket).await["type"], "error");
}

#[tokio::test]
async fn unknown_connection_is_refused() {
    let (address, _, _) = start("127.0.0.1", 1).await;
    assert!(
        connect_async(format!("ws://{address}/api/connections/999/query"))
            .await
            .is_err()
    );
}

#[tokio::test]
#[ignore = "requires Docker Desktop running"]
async fn runs_the_writes_the_settings_allow() {
    let server = Mariadb::default().start().await.unwrap();
    let host = server.get_host().await.unwrap().to_string();
    let port = server.get_host_port_ipv4(3306).await.unwrap();
    let allowed = [
        WriteKind::Insert,
        WriteKind::CreateTable,
        WriteKind::AlterTable,
        WriteKind::CreateProcedure,
        WriteKind::CreateTemporaryTable,
        WriteKind::AlterTemporaryTable,
        WriteKind::DropTemporaryTable,
    ];
    let (address, id, _) = start_writing(&host, port, Some(&allowed)).await;
    let (mut socket, _) = connect_async(format!("ws://{address}/api/connections/{id}/query"))
        .await
        .unwrap();
    assert_eq!(next_json(&mut socket).await["type"], "ready");

    for sql in [
        "CREATE TABLE w (a INT)",
        "INSERT INTO w VALUES (1), (2)",
        "ALTER TABLE w ADD KEY k (a)",
        "ALTER TABLE w RENAME KEY k TO j",
        "ALTER TABLE w DROP KEY j",
    ] {
        let result = run_one(&mut socket, sql, None).await;
        assert_eq!(result.end["type"], "done", "{sql}: {:?}", result.end);
    }

    // A procedure created after DELIMITER, its body kept whole.
    let text =
        "DELIMITER //\nCREATE PROCEDURE p() BEGIN SELECT COUNT(*) FROM w; END//\nDELIMITER ;";
    let (batch, results) = run(&mut socket, json!({ "sql": text, "cursor": 20 })).await;
    assert_eq!(
        batch["queries"][0]["sql"],
        "CREATE PROCEDURE p() BEGIN SELECT COUNT(*) FROM w; END"
    );
    assert_eq!(results[0].end["type"], "done", "{:?}", results[0].end);

    // A batch that writes runs in the order of the text, on the session's
    // connection (a temporary table is seen by the next queries), and stops
    // at the first failure.
    let text = "CREATE TEMPORARY TABLE tmp (a INT); INSERT INTO tmp VALUES (1); \
                SELECT COUNT(*) FROM tmp; INSERT INTO missing VALUES (1); SELECT 1;";
    let (_, results) = run(&mut socket, json!({ "sql": text })).await;
    let ends: Vec<&str> = results
        .iter()
        .map(|result| result.end["type"].as_str().unwrap_or_default())
        .collect();
    assert_eq!(ends, ["done", "done", "done", "error", "error"]);
    assert_eq!(results[2].rows, 1);
    assert_eq!(results[4].end["detail"]["code"], "query_skipped");

    // The switches of the temporary tables apply to the tables the session
    // created, followed when renamed, and only to them.
    let refused = |result: &Received| result.end["detail"].clone();
    for sql in [
        "ALTER TABLE tmp RENAME TO tmp2",
        "ALTER TABLE tmp2 ADD b INT",
    ] {
        let result = run_one(&mut socket, sql, None).await;
        assert_eq!(result.end["type"], "done", "{sql}: {:?}", result.end);
    }
    let result = run_one(&mut socket, "RENAME TABLE tmp2 TO tmp3", None).await;
    assert_eq!(
        refused(&result),
        json!({ "code": "write_not_allowed", "kinds": ["rename_temporary_table"] })
    );
    // TRUNCATE empties the data: the switch of the data, for every table.
    let result = run_one(&mut socket, "TRUNCATE TABLE tmp2", None).await;
    assert_eq!(
        refused(&result),
        json!({ "code": "write_not_allowed", "kinds": ["truncate_table"] })
    );
    let result = run_one(&mut socket, "DROP TABLE w", None).await;
    assert_eq!(
        refused(&result),
        json!({ "code": "write_not_allowed", "kinds": ["drop_table"] })
    );
    let result = run_one(&mut socket, "DROP TABLE tmp2", None).await;
    assert_eq!(result.end["type"], "done", "{:?}", result.end);
    // Dropped: a table of that name is a permanent table again.
    let result = run_one(&mut socket, "RENAME TABLE tmp2 TO tmp3", None).await;
    assert_eq!(
        refused(&result),
        json!({ "code": "write_not_allowed", "kinds": ["rename_table"] })
    );

    // A switch off, a write no switch allows.
    let result = run_one(&mut socket, "DELETE FROM w", None).await;
    assert_eq!(
        result.end["detail"],
        json!({ "code": "write_not_allowed", "kinds": ["delete"] })
    );
    let result = run_one(&mut socket, "CALL p()", None).await;
    assert_eq!(result.end["detail"]["code"], "write_forbidden");
}

#[tokio::test]
#[ignore = "requires Docker Desktop running"]
async fn streams_results_with_the_default_limit() {
    let server = Mariadb::default().start().await.unwrap();
    let host = server.get_host().await.unwrap().to_string();
    let port = server.get_host_port_ipv4(3306).await.unwrap();
    let (address, id, _) = start(&host, port).await;
    let (mut socket, _) = connect_async(format!("ws://{address}/api/connections/{id}/query"))
        .await
        .unwrap();

    let ready = next_json(&mut socket).await;
    assert_eq!(ready["type"], "ready");
    assert_eq!(ready["server"]["flavor"], "mariadb");
    assert_eq!(ready["server"]["ansi_quotes"], true);
    assert_eq!(ready["database"], "test");

    // No limit written: 1,000 rows shown out of 2,500, truncated result.
    let result = run_one(&mut socket, "SELECT seq FROM seq_1_to_2500", Some(1000)).await;
    assert_eq!(result.columns["limit"], "added");
    assert_eq!(
        result.columns["sql"],
        "SELECT seq FROM seq_1_to_2500 LIMIT 1001"
    );
    assert_eq!(result.rows, 1000);
    // A short first batch shows quickly, full batches follow.
    assert_eq!(result.batch_sizes, [50, 500, 450]);
    assert_eq!(result.end["truncated"], true);

    // Written limit: it applies as is, beyond the default limit.
    let result = run_one(
        &mut socket,
        "SELECT seq FROM seq_1_to_2500 LIMIT 1500",
        None,
    )
    .await;
    assert_eq!(result.columns["limit"], "user_limit");
    assert_eq!(result.rows, 1500);
    assert_eq!(result.end["truncated"], false);

    // An error does not interrupt the session.
    let result = run_one(&mut socket, "SELECT * FROM missing", None).await;
    assert_eq!(result.end["type"], "error");
    let result = run_one(&mut socket, "SELECT 1", None).await;
    assert_eq!(
        (result.rows, result.end["type"].as_str()),
        (1, Some("done"))
    );

    // USE changes the current database, announced at the end of the query.
    let result = run_one(&mut socket, "USE mysql", None).await;
    assert_eq!(result.end["database"], "mysql");

    // use changes the current database without a result; an unknown database gives a session error.
    for database in ["test", "missing", "mysql"] {
        let request = json!({ "type": "use", "database": database });
        socket
            .send(ClientMessage::Text(request.to_string().into()))
            .await
            .unwrap();
        let answer = next_json(&mut socket).await;
        if database == "missing" {
            assert_eq!(
                (answer["type"].as_str(), answer["query"].is_null()),
                (Some("error"), true)
            );
        } else {
            assert_eq!(answer, json!({ "type": "database", "database": database }));
        }
    }

    // Cursor on the empty line between two queries: the previous one runs.
    let text = "SELECT 'a';\n\nSELECT 'b', 'c';";
    let (batch, results) = run(&mut socket, json!({ "sql": text, "cursor": 12 })).await;
    assert_eq!(batch["queries"][0]["sql"], "SELECT 'a'");
    assert_eq!(results.len(), 1);

    // On-connect query run on the session's connection and on those of the
    // parallel queries.
    let check = "SELECT 1 FROM DUAL WHERE @prisme_init = 1";
    let (_, results) = run(&mut socket, json!({ "sql": check, "cursor": 0 })).await;
    assert_eq!(results[0].rows, 1);
    let request = json!({ "sql": format!("{check}; {check};") });
    let (_, results) = run(&mut socket, request).await;
    assert!(results.iter().all(|result| result.rows == 1));

    // Without a cursor: all the queries, in parallel, on the current database (mysql).
    let request = json!({ "sql": "SELECT 1; SELECT DATABASE(), 2;" });
    let (batch, results) = run(&mut socket, request).await;
    assert_eq!(batch["queries"].as_array().unwrap().len(), 2);
    assert_eq!(results[0].rows, 1);
    assert_eq!(results[1].columns["columns"], json!(["DATABASE()", "2"]));
    assert_eq!(results[1].columns["right_aligned"], json!([false, true]));
    let families: Vec<&Value> = results[1].columns["types"]
        .as_array()
        .unwrap()
        .iter()
        .map(|column| &column["family"])
        .collect();
    assert_eq!(families, [&json!("text"), &json!("integer")]);
    assert_eq!(results[1].end["database"], "mysql");
    assert!(results.iter().all(|result| result.started));

    // Selected text only: the WHERE clause outside the selection does not run.
    let text = "SELECT 1; SELECT 2 FROM DUAL WHERE 1 = 0;";
    let selection = json!([{ "start": 0, "end": 28 }]);
    let request = json!({ "sql": text, "selection": selection, "exact": true });
    let (batch, results) = run(&mut socket, request).await;
    let sqls: Vec<&Value> = batch["queries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|query| &query["sql"])
        .collect();
    assert_eq!(sqls, [&json!("SELECT 1"), &json!("SELECT 2 FROM DUAL")]);
    assert_eq!(results[1].rows, 1);

    // Stop: the running queries are interrupted well before their end.
    let started = std::time::Instant::now();
    let request = json!({ "type": "run", "sql": "SELECT SLEEP(30); SELECT SLEEP(30);" });
    socket
        .send(ClientMessage::Text(request.to_string().into()))
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    let cancel = json!({ "type": "cancel" }).to_string();
    socket
        .send(ClientMessage::Text(cancel.into()))
        .await
        .unwrap();
    let mut errors = 0;
    loop {
        let message = next_json(&mut socket).await;
        match message["type"].as_str().unwrap() {
            "error" => {
                assert_eq!(message["message"], "query interrupted by the user");
                errors += 1;
            }
            "batch_done" => break,
            _ => {}
        }
    }
    assert_eq!(errors, 2);
    assert!(started.elapsed() < std::time::Duration::from_secs(10));

    // Closing a tab: only its query is interrupted, the other one goes to the end.
    let started = std::time::Instant::now();
    let request = json!({ "type": "run", "sql": "SELECT SLEEP(30); SELECT SLEEP(1);" });
    socket
        .send(ClientMessage::Text(request.to_string().into()))
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    let cancel = json!({ "type": "cancel_query", "query": 0 }).to_string();
    socket
        .send(ClientMessage::Text(cancel.into()))
        .await
        .unwrap();
    let (mut interrupted, mut finished) = (Vec::new(), Vec::new());
    loop {
        let message = next_json(&mut socket).await;
        match message["type"].as_str().unwrap() {
            "error" => interrupted.push(message["query"].clone()),
            "done" => finished.push(message["query"].clone()),
            "batch_done" => break,
            _ => {}
        }
    }
    assert_eq!((interrupted, finished), (vec![json!(0)], vec![json!(1)]));
    assert!(started.elapsed() < std::time::Duration::from_secs(10));

    // Nothing to run: session error.
    let (error, _) = run(&mut socket, json!({ "sql": " -- nothing" })).await;
    assert_eq!(error["message"], "no query to run");
}

#[tokio::test]
#[ignore = "requires Docker Desktop running"]
async fn closing_the_editor_session_ends_it_at_once() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let server = Mariadb::default().start().await.unwrap();
    let host = server.get_host().await.unwrap().to_string();
    let port = server.get_host_port_ipv4(3306).await.unwrap();
    let (address, _, editor_id) = start(&host, port).await;
    let url = format!("ws://{address}/api/editors/{editor_id}/session");
    let (mut socket, _) = connect_async(&url).await.unwrap();
    assert_eq!(next_json(&mut socket).await["database"], "test");
    let result = run_one(&mut socket, "USE mysql", None).await;
    assert_eq!(result.end["database"], "mysql");

    // DELETE on the session, in plain HTTP: no HTTP client among the dependencies.
    let mut http = TcpStream::connect(&address).await.unwrap();
    let request = format!(
        "DELETE /api/editors/{editor_id}/session HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n"
    );
    http.write_all(request.as_bytes()).await.unwrap();
    let mut answer = String::new();
    http.read_to_string(&mut answer).await.unwrap();
    assert!(answer.starts_with("HTTP/1.1 204"), "{answer}");

    // The WebSocket closes, without waiting for the keep-alive delay.
    let closed = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while let Some(Ok(message)) = socket.next().await {
            if matches!(message, ClientMessage::Close(_)) {
                break;
            }
        }
    })
    .await;
    assert!(closed.is_ok());

    // A new WebSocket opens another session: back to the default database.
    let (mut socket, _) = connect_async(&url).await.unwrap();
    let ready = next_json(&mut socket).await;
    assert_eq!(
        (ready["type"].as_str(), ready["database"].as_str()),
        (Some("ready"), Some("test"))
    );
}

#[tokio::test]
#[ignore = "requires Docker Desktop running"]
async fn editor_session_opens_on_the_requested_database() {
    let server = Mariadb::default().start().await.unwrap();
    let host = server.get_host().await.unwrap().to_string();
    let port = server.get_host_port_ipv4(3306).await.unwrap();
    let (address, _, editor_id) = start(&host, port).await;
    let url = format!("ws://{address}/api/editors/{editor_id}/session?database=mysql");
    let (mut socket, _) = connect_async(&url).await.unwrap();
    // Ready on the requested database at once, without going through the
    // connection's database (`test`).
    let ready = next_json(&mut socket).await;
    assert_eq!(
        (ready["type"].as_str(), ready["database"].as_str()),
        (Some("ready"), Some("mysql"))
    );
}

#[tokio::test]
#[ignore = "requires Docker Desktop running"]
async fn editor_session_survives_a_disconnect() {
    let server = Mariadb::default().start().await.unwrap();
    let host = server.get_host().await.unwrap().to_string();
    let port = server.get_host_port_ipv4(3306).await.unwrap();
    let (address, _, editor_id) = start(&host, port).await;
    let url = format!("ws://{address}/api/editors/{editor_id}/session");
    let (mut socket, _) = connect_async(&url).await.unwrap();
    assert_eq!(next_json(&mut socket).await["database"], "test");

    // The current database changes on the session's connection.
    let result = run_one(&mut socket, "USE mysql", None).await;
    assert_eq!(result.end["database"], "mysql");

    // A batch starts, then the page disappears before its end.
    let request = json!({ "type": "run", "sql": "SELECT SLEEP(2); SELECT 1;" });
    socket
        .send(ClientMessage::Text(request.to_string().into()))
        .await
        .unwrap();
    let batch = next_json(&mut socket).await;
    assert_eq!(batch["type"], "batch");
    drop(socket);
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    // The page comes back: same session (mysql database), running batch
    // resumed with the same tabs, then continued to the end.
    let (mut socket, _) = connect_async(&url).await.unwrap();
    let ready = next_json(&mut socket).await;
    assert_eq!(
        (ready["type"].as_str(), ready["database"].as_str()),
        (Some("ready"), Some("mysql"))
    );
    let resume = next_json(&mut socket).await;
    assert_eq!(resume["type"], "resume");
    assert_eq!(resume["running"], true);
    let tabs = |message: &Value, key: &str| -> Vec<Value> {
        message["queries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|query| query[key].clone())
            .collect()
    };
    assert_eq!(tabs(&resume, "tab"), tabs(&batch, "tab"));
    let mut done: Vec<u64> = resume["queries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|query| query["result"]["done"] == true)
        .map(|query| query["query"].as_u64().unwrap())
        .collect();
    loop {
        let message = next_json(&mut socket).await;
        match message["type"].as_str().unwrap() {
            "done" => done.push(message["query"].as_u64().unwrap()),
            "batch_done" => break,
            kind => assert_ne!(kind, "error", "{message}"),
        }
    }
    done.sort_unstable();
    assert_eq!(done, [0, 1]);
}

#[tokio::test]
#[ignore = "requires Docker Desktop running"]
async fn editor_session_tells_its_open_transaction() {
    let server = Mariadb::default().start().await.unwrap();
    let host = server.get_host().await.unwrap().to_string();
    let port = server.get_host_port_ipv4(3306).await.unwrap();
    // Read-only sessions, as by default.
    let (address, _, editor_id) = start(&host, port).await;
    let url = format!("ws://{address}/api/editors/{editor_id}/session");
    let (mut socket, _) = connect_async(&url).await.unwrap();
    assert_eq!(next_json(&mut socket).await["transaction"], false);

    // What would lift the read-only mode is refused, and opens nothing.
    assert_eq!(
        run_transaction(&mut socket, "START TRANSACTION READ WRITE").await,
        (Vec::new(), json!("read_write_transaction_read_only"))
    );
    assert_eq!(
        run_transaction(&mut socket, "START TRANSACTION").await,
        (vec![json!(true)], Value::Null)
    );
    // Told on a change only.
    assert_eq!(
        run_transaction(&mut socket, "SELECT 1").await,
        (Vec::new(), Value::Null)
    );
    // The page comes back during the transaction.
    drop(socket);
    let (mut socket, _) = connect_async(&url).await.unwrap();
    assert_eq!(next_json(&mut socket).await["transaction"], true);
    assert_eq!(
        run_transaction(&mut socket, "COMMIT").await,
        (vec![json!(false)], Value::Null)
    );
}

#[tokio::test]
#[ignore = "requires Docker Desktop running"]
async fn runs_all_in_sequence_while_a_transaction_is_open() {
    let server = Mariadb::default().start().await.unwrap();
    let host = server.get_host().await.unwrap().to_string();
    let port = server.get_host_port_ipv4(3306).await.unwrap();
    let (address, _, editor_id) = start(&host, port).await;
    let url = format!("ws://{address}/api/editors/{editor_id}/session");
    let (mut socket, _) = connect_async(&url).await.unwrap();
    next_json(&mut socket).await;
    // A variable of the session's connection, which the extra connections
    // of a parallel run do not have.
    run_one(&mut socket, "SET @x = 42", None).await;
    let reads = "SELECT 1 FROM DUAL WHERE @x = 42; SELECT 2 FROM DUAL WHERE @x = 42";
    assert_eq!(run_all_rows(&mut socket, reads).await, [0, 0]);

    run_transaction(&mut socket, "START TRANSACTION").await;
    assert_eq!(run_all_rows(&mut socket, reads).await, [1, 1]);
    run_transaction(&mut socket, "COMMIT").await;
    assert_eq!(run_all_rows(&mut socket, reads).await, [0, 0]);
}

#[tokio::test]
#[ignore = "requires Docker Desktop running"]
async fn editor_session_ends_when_the_server_closes_its_connection() {
    let server = Mariadb::default().start().await.unwrap();
    let host = server.get_host().await.unwrap().to_string();
    let port = server.get_host_port_ipv4(3306).await.unwrap();
    let (address, _, editor_id) = start(&host, port).await;
    let url = format!("ws://{address}/api/editors/{editor_id}/session");
    let (mut socket, _) = connect_async(&url).await.unwrap();
    next_json(&mut socket).await;
    run_transaction(&mut socket, "START TRANSACTION").await;

    // The server closes the connection after the COMMIT: the session ends.
    let request = json!({ "type": "run", "sql": "COMMIT RELEASE", "cursor": 0 });
    socket
        .send(ClientMessage::Text(request.to_string().into()))
        .await
        .unwrap();
    let error = loop {
        let message = next_json(&mut socket).await;
        if message["type"] == "error" && message["query"].is_null() {
            break message;
        }
    };
    assert_eq!(error["detail"]["code"], "connection_closed_by_server");
    // Then the WebSocket closes.
    while let Some(Ok(message)) = socket.next().await {
        assert!(!matches!(message, ClientMessage::Text(_)), "{message:?}");
    }
}
