//! Object navigator: catalog of a saved connection, sent progressively.
//!
//! Route: `GET /api/connections/{id}/catalog`. The response is a stream of
//! JSON lines (`application/x-ndjson`), one `CatalogEvent` message per line,
//! sent as soon as it is ready:
//!
//! 1. `schemas`: names of all the databases, with the server's flavor (also
//!    remembered on the connection, for the navigator's badge);
//! 2. `objects`: tables, views, procedures, functions and events, database by
//!    database;
//! 3. `details`: columns, indexes, foreign keys and triggers of the tables,
//!    database by database;
//! 4. `done`, or `error` if a read fails.
//!
//! The interface therefore shows the databases almost at once, then the
//! objects and their details as they arrive. The database chosen in the
//! connection is read first, the system databases (`information_schema`,
//! `mysql`...) last.

use std::convert::Infallible;

use axum::Router;
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::header::CONTENT_TYPE;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use futures_util::stream;
use prisme_connectors::mysql::catalog::{self, SchemaObjects, TableDetails};
use prisme_connectors::mysql::{self, Connection, Flavor, ServerInfo};
use serde::Serialize;
use tokio::sync::mpsc;
use tracing::Instrument;
use ts_rs::TS;

use crate::api::AppState;
use crate::api_error::ApiError;
use crate::auth::CurrentUser;
use crate::connections::{self, ConnectionsError};
use crate::console::{self, Action};
use crate::error_detail::ErrorDetail;

/// Databases provided by the server itself, read last.
const SYSTEM_SCHEMAS: [&str; 4] = ["information_schema", "mysql", "performance_schema", "sys"];

/// Number of messages waiting to be sent beyond which reading waits.
const BUFFER: usize = 16;

/// Message of the catalog stream.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CatalogEvent {
    /// Names of all the databases, in alphabetical order, and flavor and
    /// version (raw result of `SELECT VERSION()`) of the server.
    Schemas {
        schemas: Vec<String>,
        flavor: Flavor,
        version: String,
    },
    /// Objects of a database: tables, views, procedures, functions and events.
    Objects {
        schema: String,
        objects: SchemaObjects,
    },
    /// Details of the tables of a database: columns, indexes, foreign keys, triggers.
    Details {
        schema: String,
        tables: Vec<TableDetails>,
    },
    /// Failure of a read; the stream goes on with the next database. The
    /// English message of the logs, and the detail for the interface.
    Error {
        message: String,
        detail: ErrorDetail,
    },
    /// End of the catalog.
    Done,
}

/// Routes of the navigator, to merge into the API router.
pub fn routes() -> Router<AppState> {
    Router::new().route("/api/connections/{id}/catalog", get(stream_catalog))
}

/// `GET /api/connections/{id}/catalog`: stream of the server's catalog.
///
/// The connection to the target server opens before the stream starts: a
/// connection failure stays an HTTP error response (502, 504).
async fn stream_catalog(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<i64>,
) -> Result<Response, ApiError> {
    let user_id = user.id;
    let (target, name) = state
        .with_db(move |db, key| {
            let target = connections::load_target(db, key, user_id, id)?;
            let name = connections::name_of(db, user_id, id)?;
            Ok::<_, ConnectionsError>(target.map(|target| (target, name)))
        })
        .await?
        .ok_or(ApiError::NotFound)?;
    // The queries of the catalog go to the history of the queries.
    let span = console::sql_span(user_id, &name, Action::Navigator);
    let mut connection = mysql::connect(&target).instrument(span.clone()).await?;
    let server = mysql::server_info(&mut connection)
        .instrument(span.clone())
        .await?;
    let remembered = server.clone();
    state
        .with_db(move |db, _| connections::remember_server(db, user_id, id, &remembered))
        .await?;

    let (sender, receiver) = mpsc::channel(BUFFER);
    tokio::spawn(
        async move {
            read_catalog(&mut connection, &target.database_name, server, &sender).await;
            let _ = connection.disconnect().await;
        }
        .instrument(span),
    );

    let lines = stream::unfold(receiver, |mut receiver| async move {
        let event = receiver.recv().await?;
        let json = serde_json::to_string(&event)
            .expect("a navigator event always serialises: its fields are plain data");
        let line = format!("{json}\n");
        Some((Ok::<_, Infallible>(line), receiver))
    });
    Ok((
        [(CONTENT_TYPE, "application/x-ndjson")],
        Body::from_stream(lines),
    )
        .into_response())
}

/// Reads the catalog database by database and sends each part as soon as it is read.
///
/// Stops as soon as the client has closed the response.
async fn read_catalog(
    connection: &mut Connection,
    preferred: &str,
    server: ServerInfo,
    sender: &mpsc::Sender<CatalogEvent>,
) {
    let flavor = server.flavor;
    let schemas = match catalog::schemas(connection).await {
        Ok(schemas) => schemas,
        Err(error) => {
            let _ = sender.send(error_event(None, flavor, &error)).await;
            return;
        }
    };
    let order = reading_order(&schemas, preferred);
    if sender
        .send(CatalogEvent::Schemas {
            schemas,
            flavor,
            version: server.version,
        })
        .await
        .is_err()
    {
        return;
    }

    for schema in &order {
        let event = match catalog::objects(connection, schema).await {
            Ok(objects) => CatalogEvent::Objects {
                schema: schema.clone(),
                objects,
            },
            Err(error) => error_event(Some(schema), flavor, &error),
        };
        if sender.send(event).await.is_err() {
            return;
        }
    }
    for schema in &order {
        let event = match catalog::details(connection, schema).await {
            Ok(tables) => CatalogEvent::Details {
                schema: schema.clone(),
                tables,
            },
            Err(error) => error_event(Some(schema), flavor, &error),
        };
        if sender.send(event).await.is_err() {
            return;
        }
    }
    let _ = sender.send(CatalogEvent::Done).await;
}

/// Error message for a failed catalog read: of the list of the databases
/// (`schema` absent), or of a database.
fn error_event(schema: Option<&str>, flavor: Flavor, error: &mysql::MysqlError) -> CatalogEvent {
    let detail = ErrorDetail::CatalogUnreadable {
        schema: schema.map(str::to_owned),
        flavor,
        cause: error.cause(),
    };
    CatalogEvent::Error {
        message: detail.to_string(),
        detail,
    }
}

/// Reading order of the databases: the preferred database, then the user's
/// databases, then the system databases.
fn reading_order(schemas: &[String], preferred: &str) -> Vec<String> {
    let rank = |schema: &String| {
        if schema == preferred {
            0
        } else if SYSTEM_SCHEMAS.contains(&schema.as_str()) {
            2
        } else {
            1
        }
    };
    let mut order = schemas.to_vec();
    // Stable sort: the alphabetical order is kept at equal rank.
    order.sort_by_key(rank);
    order
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use secrecy::SecretString;
    use tower::ServiceExt;

    use crate::api::{AppState, router};
    use crate::connections::{self, NewConnection};
    use crate::master_key::MasterKey;
    use crate::store;

    /// Sends `GET uri` to an application whose connection 1 targets a closed port.
    async fn status_of(uri: &str) -> StatusCode {
        let db = store::open_in_memory();
        let key = MasterKey::generate();
        let user_id = store::ensure_user(&db, "test@example.com").unwrap();
        let new = NewConnection {
            name: "closed".to_string(),
            host: "127.0.0.1".to_string(),
            // Port 1: no server listens there.
            port: 1,
            username: "root".to_string(),
            password: SecretString::from(""),
            database_name: String::new(),
            init_sql: String::new(),
            session_timeout_minutes: 5,
            show_system_schemas: true,
        };
        connections::insert(&db, &key, user_id, &new).unwrap();
        let app = router(AppState::new(db, key, user_id));

        let request = Request::get(uri)
            .header("host", "localhost:8484")
            .body(Body::empty())
            .unwrap();
        app.oneshot(request).await.unwrap().status()
    }

    #[test]
    fn reads_the_preferred_schema_first_and_system_schemas_last() {
        let schemas: Vec<String> = ["information_schema", "mysql", "shop", "sys", "xlist"]
            .map(String::from)
            .to_vec();
        assert_eq!(
            super::reading_order(&schemas, "xlist"),
            ["xlist", "shop", "information_schema", "mysql", "sys"]
        );
    }

    #[tokio::test]
    async fn unknown_connection_gives_not_found() {
        assert_eq!(
            status_of("/api/connections/42/catalog").await,
            StatusCode::NOT_FOUND
        );
    }

    #[tokio::test]
    async fn unreachable_server_gives_bad_gateway() {
        assert_eq!(
            status_of("/api/connections/1/catalog").await,
            StatusCode::BAD_GATEWAY
        );
    }
}
