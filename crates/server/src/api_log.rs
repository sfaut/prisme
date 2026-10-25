//! Log of the use of the API, written with `--verbose`.
//!
//! One line per HTTP request, files of the interface and WebSocket upgrades
//! included: method, path, status and duration. One line per message of the
//! query sessions, received from the client or sent to it: its type, the
//! numbers of its queries and its counts. The lines of a session carry its
//! span, for example `editor{id=3}`.
//!
//! The log tells how the API is used, without what it carries: no bodies
//! (they hold the passwords of the connections), no query strings, no SQL
//! texts, no error messages (they quote the SQL) and no values of rows. Its
//! lines are at the `debug` level, under the target of this module:
//! `--verbose` enables them (`directive`), as does `RUST_LOG=debug`.

use std::fmt;
use std::time::Instant;

use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;
use tracing::Span;
use tracing_subscriber::filter::Directive;

use crate::query_session::{ClientMessage, LimitReport, ServerMessage};

/// Filter directive that enables this log: its target, at the `debug` level.
pub fn directive() -> Directive {
    concat!(module_path!(), "=debug")
        .parse()
        .expect("a module path followed by a level is a valid directive")
}

/// Writes each request, with its status and duration, once it is answered.
pub async fn requests(request: Request, next: Next) -> Response {
    let method = request.method().clone();
    let uri = request.uri().clone();
    let start = Instant::now();
    let response = next.run(request).await;
    tracing::debug!(
        "{method} {} {} {} ms",
        uri.path(),
        response.status().as_u16(),
        start.elapsed().as_millis()
    );
    response
}

/// Span of the query session of editor `id`: the lines of the session carry it.
pub fn editor_span(id: i64) -> Span {
    tracing::debug_span!("editor", id)
}

/// Span of a query session opened on connection `id`, outside an editor.
pub fn connection_span(id: i64) -> Span {
    tracing::debug_span!("connection", id)
}

/// Writes a message received from the client of a query session, `None` for
/// a message that cannot be read.
pub fn received(message: Option<&ClientMessage>) {
    tracing::debug!("received {}", Received(message));
}

/// Writes a message sent to the client of a query session.
pub fn sent(message: &ServerMessage) {
    tracing::debug!("sent {}", Sent(message));
}

/// Summary of a message of the client, for the log.
struct Received<'a>(Option<&'a ClientMessage>);

impl fmt::Display for Received<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            None => f.write_str("an unreadable message"),
            Some(ClientMessage::Run(request)) => {
                f.write_str("run")?;
                // The cursor prevails over the selection, as in the session.
                match (&request.cursor, &request.selection) {
                    (Some(cursor), _) => write!(f, ", cursor {cursor}")?,
                    (None, Some(passages)) if !passages.is_empty() => write!(
                        f,
                        ", {}",
                        Count(passages.len(), "selected passage", "selected passages")
                    )?,
                    _ => f.write_str(", all the queries")?,
                }
                match request.row_limit {
                    Some(limit) => write!(f, ", row limit {limit}"),
                    None => Ok(()),
                }
            }
            Some(ClientMessage::Cancel) => f.write_str("cancel"),
            Some(ClientMessage::CancelQuery { query }) => write!(f, "cancel_query, query {query}"),
            Some(ClientMessage::Use { .. }) => f.write_str("use"),
        }
    }
}

/// Summary of a message sent to the client, for the log.
struct Sent<'a>(&'a ServerMessage);

impl fmt::Display for Sent<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            ServerMessage::Ready { server, .. } => write!(f, "ready, server {}", server.version),
            ServerMessage::Batch { queries } => {
                write!(f, "batch, {}", Count(queries.len(), "query", "queries"))
            }
            ServerMessage::Started { query } => write!(f, "started, query {query}"),
            ServerMessage::Columns {
                query,
                columns,
                limit,
                execution_us,
                ..
            } => write!(
                f,
                "columns, query {query}, {}, limit {}, executed in {execution_us} µs",
                Count(columns.len(), "column", "columns"),
                limit_name(limit)
            ),
            ServerMessage::Rows { query, rows } => {
                write!(
                    f,
                    "rows, query {query}, {}",
                    Count(rows.len(), "row", "rows")
                )
            }
            ServerMessage::Done {
                query,
                row_count,
                truncated,
                affected_rows,
                elapsed_ms,
                ..
            } => {
                write!(
                    f,
                    "done, query {query}, {}",
                    Count(*row_count, "row", "rows")
                )?;
                if *truncated {
                    f.write_str(", truncated")?;
                }
                if *affected_rows > 0 {
                    let affected = Count(*affected_rows, "affected row", "affected rows");
                    write!(f, ", {affected}")?;
                }
                write!(f, ", {elapsed_ms} ms")
            }
            ServerMessage::Error {
                query: Some(query), ..
            } => write!(f, "error, query {query}"),
            ServerMessage::Error { query: None, .. } => f.write_str("error, session"),
            ServerMessage::BatchDone => f.write_str("batch_done"),
            ServerMessage::Database { .. } => f.write_str("database"),
            ServerMessage::Transaction { open: true } => f.write_str("transaction, open"),
            ServerMessage::Transaction { open: false } => f.write_str("transaction, ended"),
            ServerMessage::Resume { running, queries } => {
                write!(f, "resume, {}", Count(queries.len(), "query", "queries"))?;
                if *running {
                    f.write_str(", running")?;
                }
                Ok(())
            }
        }
    }
}

/// Name of the handling of the limit, as in the protocol.
fn limit_name(limit: &LimitReport) -> &'static str {
    match limit {
        LimitReport::Added => "added",
        LimitReport::UserLimit => "user_limit",
        LimitReport::NotApplicable => "not_applicable",
        LimitReport::Unparsed => "unparsed",
    }
}

/// A number followed by its noun: the singular for 1, the plural otherwise.
struct Count<T>(T, &'static str, &'static str);

impl<T: fmt::Display + PartialEq + From<u8>> fmt::Display for Count<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let noun = if self.0 == T::from(1) { self.1 } else { self.2 };
        write!(f, "{} {noun}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use std::io;
    use std::sync::{Arc, Mutex};

    use axum::Router;

    use crate::error_detail::ErrorDetail;
    use axum::body::Body;
    use axum::http::StatusCode;
    use axum::middleware;
    use axum::routing::post;
    use prisme_connectors::mysql::{Flavor, ServerInfo};
    use tower::ServiceExt;

    use super::*;

    fn client(json: &str) -> String {
        let message = serde_json::from_str::<ClientMessage>(json).unwrap();
        Received(Some(&message)).to_string()
    }

    fn server(message: ServerMessage) -> String {
        Sent(&message).to_string()
    }

    #[test]
    fn enables_the_target_of_this_module() {
        assert_eq!(directive().to_string(), "prisme::api_log=debug");
    }

    #[test]
    fn sums_up_the_client_messages_without_their_sql() {
        assert_eq!(
            client(r#"{"type": "run", "sql": "SELECT secret", "cursor": 3}"#),
            "run, cursor 3"
        );
        assert_eq!(
            client(r#"{"type": "run", "sql": "SELECT 1", "selection": [{"start": 0, "end": 2}]}"#),
            "run, 1 selected passage"
        );
        assert_eq!(
            client(r#"{"type": "run", "sql": "SELECT 1", "selection": [], "row_limit": 500}"#),
            "run, all the queries, row limit 500"
        );
        assert_eq!(client(r#"{"type": "cancel"}"#), "cancel");
        assert_eq!(
            client(r#"{"type": "cancel_query", "query": 2}"#),
            "cancel_query, query 2"
        );
        assert_eq!(client(r#"{"type": "use", "database": "shop"}"#), "use");
        assert_eq!(Received(None).to_string(), "an unreadable message");
    }

    #[test]
    fn sums_up_the_server_messages_with_their_numbers_and_counts() {
        assert_eq!(
            server(ServerMessage::Ready {
                server: ServerInfo {
                    flavor: Flavor::MariaDb,
                    version: "10.6.28-MariaDB".to_string(),
                    ansi_quotes: false,
                    oldest_supported: None,
                },
                database: Some("shop".to_string()),
                transaction: false,
            }),
            "ready, server 10.6.28-MariaDB"
        );
        assert_eq!(
            server(ServerMessage::Transaction { open: true }),
            "transaction, open"
        );
        assert_eq!(
            server(ServerMessage::Columns {
                query: 0,
                columns: vec!["a".to_string(), "b".to_string()],
                right_aligned: vec![false, false],
                types: Vec::new(),
                sql: "SELECT secret LIMIT 1001".to_string(),
                limit: LimitReport::Added,
                expected_rows: Some(1000),
                execution_us: 830,
                executed_at: 1_792_500_000_000,
            }),
            "columns, query 0, 2 columns, limit added, executed in 830 µs"
        );
        assert_eq!(
            server(ServerMessage::Rows {
                query: 0,
                rows: vec![Vec::new(); 50],
            }),
            "rows, query 0, 50 rows"
        );
        assert_eq!(
            server(ServerMessage::Done {
                query: 0,
                row_count: 1000,
                truncated: true,
                affected_rows: 0,
                elapsed_ms: 160,
                database: None,
            }),
            "done, query 0, 1000 rows, truncated, 160 ms"
        );
        assert_eq!(
            server(ServerMessage::Done {
                query: 1,
                row_count: 0,
                truncated: false,
                affected_rows: 1,
                elapsed_ms: 3,
                database: None,
            }),
            "done, query 1, 0 rows, 1 affected row, 3 ms"
        );
        assert_eq!(
            server(ServerMessage::Error {
                query: Some(1),
                message: "near 'SELECT secret'".to_string(),
                detail: ErrorDetail::NoQuery,
            }),
            "error, query 1"
        );
        assert_eq!(
            server(ServerMessage::Error {
                query: None,
                message: "refused".to_string(),
                detail: ErrorDetail::NoQuery,
            }),
            "error, session"
        );
        assert_eq!(
            server(ServerMessage::Resume {
                running: true,
                queries: Vec::new(),
            }),
            "resume, 0 queries, running"
        );
    }

    /// Log written in memory by the tests.
    #[derive(Clone, Default)]
    struct Captured(Arc<Mutex<Vec<u8>>>);

    impl io::Write for Captured {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn writes_a_request_without_its_body_nor_its_query_string() {
        let log = Captured::default();
        let writer = log.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::new("off").add_directive(directive()))
            .with_writer(move || writer.clone())
            .with_ansi(false)
            .finish();
        let _default = tracing::subscriber::set_default(subscriber);

        let app = Router::new()
            .route("/api/connections", post(|| async { StatusCode::CREATED }))
            .layer(middleware::from_fn(requests));
        let request = axum::http::Request::post("/api/connections?token=secret")
            .body(Body::from(r#"{"password": "secret"}"#))
            .unwrap();
        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);

        let text = String::from_utf8(log.0.lock().unwrap().clone()).unwrap();
        assert!(text.contains("POST /api/connections 201 "), "{text}");
        assert!(!text.contains("secret"), "{text}");
    }
}
