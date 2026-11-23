//! Object navigator: catalog of a saved connection, sent progressively.
//!
//! Route: `GET /api/connections/{id}/catalog`. The response is a stream of
//! JSON lines (`application/x-ndjson`), one `CatalogEvent` message per line,
//! sent as soon as it is ready:
//!
//! 1. `namespaces`: all the namespaces of the source (databases of a server),
//!    with the identity of the source (also remembered on the connection,
//!    for the navigator's badge), whose capabilities tell the levels of the
//!    namespaces and the kinds of objects;
//! 2. `objects`: objects of each namespace (tables, views, routines,
//!    events...), namespace by namespace;
//! 3. `details`: columns, indexes, foreign keys and triggers of the tables,
//!    namespace by namespace;
//! 4. `done`, or `error` if a read fails.
//!
//! The interface therefore shows the namespaces almost at once, then the
//! objects and their details as they arrive. `?editor=` names the editor
//! whose navigator reads the catalog: its queries go to the console of that
//! editor, and its connection opens on the databases of the session of that
//! editor, its databases in memory and those it attached
//! (`query_session::Shared`). The namespace chosen in the connection is read first, those the
//! source provides itself (`information_schema`, `mysql`...) last.

use std::convert::Infallible;

use axum::Router;
use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::header::CONTENT_TYPE;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use futures_util::stream;
use prisme_connectors::api::{CatalogReader, ConnectorError, Namespace, OpenOptions, SourceInfo};
use prisme_connectors::catalog::{NamespaceInfo, ObjectInfo, TableDetails};
use prisme_connectors::drivers;
use prisme_connectors::flavor::Flavor;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::Instrument;
use ts_rs::TS;

use crate::api::AppState;
use crate::api_error::ApiError;
use crate::auth::CurrentUser;
use crate::connections::{self, ConnectionsError};
use crate::console::{self, Action};
use crate::error_detail::ErrorDetail;

/// Number of messages waiting to be sent beyond which reading waits.
const BUFFER: usize = 16;

/// Message of the catalog stream.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CatalogEvent {
    /// All the namespaces, in alphabetical order, and the identity of the
    /// source.
    Namespaces {
        source: SourceInfo,
        namespaces: Vec<NamespaceInfo>,
    },
    /// Objects of a namespace: tables, views, routines, events...
    Objects {
        namespace: Namespace,
        objects: Vec<ObjectInfo>,
    },
    /// Details of the tables of a namespace: columns, indexes, foreign keys,
    /// triggers.
    Details {
        namespace: Namespace,
        tables: Vec<TableDetails>,
    },
    /// Failure of a read; the stream goes on with the next namespace. The
    /// English message of the logs, and the detail for the interface.
    Error {
        message: String,
        detail: ErrorDetail,
    },
    /// End of the catalog.
    Done,
}

/// Editor whose navigator reads a catalog, read from the query string
/// (`editor=3`).
#[derive(Debug, Default, Deserialize)]
struct CatalogQuery {
    /// Id of the editor; absent, the queries go outside the editors.
    editor: Option<i64>,
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
    Query(query): Query<CatalogQuery>,
) -> Result<Response, ApiError> {
    let user_id = user.id;
    let locations = state.locations();
    let (target, name) = state
        .with_db(move |db, key| {
            let target = connections::load_target(db, key, user_id, id, &locations)?;
            let name = connections::name_of(db, user_id, id)?;
            Ok::<_, ConnectionsError>(target.map(|target| (target, name)))
        })
        .await?
        .ok_or(ApiError::NotFound)?;
    // The queries of the catalog go to the history of the queries.
    let span = console::sql_span(user_id, &name, query.editor, Action::Navigator);
    let (target, options) =
        state
            .sessions
            .for_editor(query.editor, id, target, OpenOptions::default());
    let opening = drivers::of(target.engine).open(&target, options);
    let mut connection = opening.instrument(span.clone()).await?;
    let server = connection.info().clone();
    let remembered = server.clone();
    state
        .with_db(move |db, _| connections::remember_server(db, user_id, id, &remembered))
        .await?;

    let (sender, receiver) = mpsc::channel(BUFFER);
    let preferred = target.namespace();
    tokio::spawn(
        async move {
            read_catalog(
                connection.catalog().as_mut(),
                preferred.as_ref(),
                server,
                &sender,
            )
            .await;
            connection.close().await;
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

/// Reads the catalog namespace by namespace and sends each part as soon as it
/// is read.
///
/// Stops as soon as the client has closed the response.
async fn read_catalog(
    catalog: &mut dyn CatalogReader,
    preferred: Option<&Namespace>,
    source: SourceInfo,
    sender: &mpsc::Sender<CatalogEvent>,
) {
    let flavor = source.flavor;
    let namespaces = match catalog.namespaces().await {
        Ok(namespaces) => namespaces,
        Err(error) => {
            let _ = sender.send(error_event(None, flavor, &error)).await;
            return;
        }
    };
    let order = reading_order(&namespaces, preferred);
    if sender
        .send(CatalogEvent::Namespaces { source, namespaces })
        .await
        .is_err()
    {
        return;
    }

    for namespace in &order {
        let event = match catalog.objects(namespace).await {
            Ok(objects) => CatalogEvent::Objects {
                namespace: namespace.clone(),
                objects,
            },
            Err(error) => error_event(Some(namespace), flavor, &error),
        };
        if sender.send(event).await.is_err() {
            return;
        }
    }
    for namespace in &order {
        let event = match catalog.details(namespace).await {
            Ok(tables) => CatalogEvent::Details {
                namespace: namespace.clone(),
                tables,
            },
            Err(error) => error_event(Some(namespace), flavor, &error),
        };
        if sender.send(event).await.is_err() {
            return;
        }
    }
    let _ = sender.send(CatalogEvent::Done).await;
}

/// Error message for a failed catalog read: of the list of the namespaces
/// (`namespace` absent), or of a namespace.
fn error_event(
    namespace: Option<&Namespace>,
    flavor: Flavor,
    error: &ConnectorError,
) -> CatalogEvent {
    let detail = ErrorDetail::CatalogUnreadable {
        schema: namespace.map(|namespace| namespace.name().to_string()),
        flavor,
        cause: error.cause(),
    };
    CatalogEvent::Error {
        message: detail.to_string(),
        detail,
    }
}

/// Reading order of the namespaces: the preferred one, then the user's, then
/// those the source provides itself.
fn reading_order(namespaces: &[NamespaceInfo], preferred: Option<&Namespace>) -> Vec<Namespace> {
    let rank = |info: &NamespaceInfo| {
        if Some(&info.namespace) == preferred {
            0
        } else if info.system {
            2
        } else {
            1
        }
    };
    let mut order: Vec<&NamespaceInfo> = namespaces.iter().collect();
    // Stable sort: the alphabetical order is kept at equal rank.
    order.sort_by_key(|info| rank(info));
    order
        .into_iter()
        .map(|info| info.namespace.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use prisme_connectors::api::Engine;
    use prisme_dialects::classify::WriteKind;
    use secrecy::SecretString;
    use serde_json::json;
    use tower::ServiceExt;

    use crate::api::{AppState, Folders, router};
    use crate::connections::{self, NewConnection, NewServerSettings, NewSettings};
    use crate::editors::{self, NewEditor};
    use crate::master_key::MasterKey;
    use crate::store;
    use crate::test_support::{
        allow_writes, attachable_database, files_connection, run_on_session, serve,
    };

    /// Sends `GET uri` to an application whose connection 1 targets a closed port.
    async fn status_of(uri: &str) -> StatusCode {
        let db = store::open_in_memory();
        let key = MasterKey::generate();
        let user_id = store::ensure_user(&db, "test@example.com").unwrap();
        let new = NewConnection {
            name: "closed".to_string(),
            engine: Engine::Mysql,
            settings: NewSettings::Server(NewServerSettings {
                host: "127.0.0.1".to_string(),
                // Port 1: no server listens there.
                port: 1,
                username: "root".to_string(),
                password: SecretString::from(""),
                database_name: String::new(),
            }),
            init_sql: String::new(),
            session_timeout_minutes: 5,
            show_system_schemas: true,
        };
        connections::insert(&db, &key, user_id, &new).unwrap();
        let app = router(AppState::new(db, key, user_id, Folders::unused()));

        let request = Request::get(uri)
            .header("host", "localhost:8484")
            .body(Body::empty())
            .unwrap();
        app.oneshot(request).await.unwrap().status()
    }

    #[test]
    fn reads_the_preferred_schema_first_and_system_schemas_last() {
        use prisme_connectors::api::Namespace;
        use prisme_connectors::catalog::NamespaceInfo;

        let namespaces: Vec<NamespaceInfo> = [
            ("information_schema", true),
            ("mysql", true),
            ("shop", false),
            ("sys", true),
            ("xlist", false),
        ]
        .map(|(name, system)| NamespaceInfo {
            namespace: Namespace::single(name),
            system,
        })
        .to_vec();
        let order = super::reading_order(&namespaces, Some(&Namespace::single("xlist")));
        let names: Vec<&str> = order.iter().map(Namespace::name).collect();
        assert_eq!(
            names,
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

    #[tokio::test(flavor = "multi_thread")]
    async fn reads_the_databases_of_the_session_of_its_editor() {
        let db = store::open_in_memory();
        let key = MasterKey::generate();
        let user_id = store::ensure_user(&db, "test@example.com").unwrap();
        let folder = tempfile::tempdir().unwrap();
        let connection = files_connection(folder.path());
        let id = connections::insert(&db, &key, user_id, &connection).unwrap();
        allow_writes(&db, user_id, &[WriteKind::Attach, WriteKind::CreateTable]);
        let editor = NewEditor {
            connection_id: Some(id),
        };
        let editor_id = editors::create(&db, user_id, &editor).unwrap().id;
        let app = router(AppState::new(db, key, user_id, Folders::unused()));
        let address = serve(app.clone()).await;
        let url = format!("ws://{address}/api/editors/{editor_id}/session");
        let (mut socket, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
        run_on_session(&mut socket, "CREATE TABLE scratch.t (x INTEGER)").await;
        let other = attachable_database(folder.path(), "other");
        run_on_session(&mut socket, &format!("ATTACH '{other}' AS other")).await;

        let request = Request::get(format!("/api/connections/{id}/catalog?editor={editor_id}"))
            .header("host", "localhost:8484")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let events: Vec<serde_json::Value> = String::from_utf8(bytes.to_vec())
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        let objects = |schema: &str| -> Vec<String> {
            events
                .iter()
                .filter(|event| event["type"] == "objects" && event["namespace"] == json!([schema]))
                .flat_map(|event| {
                    let objects = event["objects"].as_array().unwrap().clone();
                    objects.into_iter().map(|object| object["name"].to_string())
                })
                .collect()
        };
        // The table of the session in memory, beside `sqlite_schema`, and
        // that of the database it attached.
        assert!(
            objects("scratch").contains(&"\"t\"".to_string()),
            "{events:?}"
        );
        assert!(
            objects("other").contains(&"\"u\"".to_string()),
            "{events:?}"
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
