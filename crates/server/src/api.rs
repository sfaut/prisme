//! HTTP API of Prisme, in JSON, served by `axum`.
//!
//! Routes:
//!
//! | Method and path                     | Role                                                          |
//! |-------------------------------------|---------------------------------------------------------------|
//! | `GET /api/health`                   | Checks that the server answers.                               |
//! | `GET /api/connections`              | Lists the user's connections, without passwords.              |
//! | `POST /api/connections`             | Saves a connection, password encrypted.                       |
//! | `POST /api/connections/probe`       | Tests typed settings, before saving them.                     |
//! | `PUT /api/connections/{id}`         | Changes a connection; empty password: unchanged (204).        |
//! | `DELETE /api/connections/{id}`      | Deletes a connection (204).                                   |
//! | `POST /api/connections/{id}/probe`  | Connects and returns the server's flavor and version.         |
//! | `GET /api/connections/{id}/query`   | WebSocket: query session (see `query_session.rs`), closed with it. |
//! | `GET /api/connections/{id}/catalog` | Databases, tables and columns (see `explorer.rs`).            |
//! | `GET /api/settings`                 | Settings of the user, with the allowed choices.               |
//! | `PUT /api/settings`                 | Changes the settings, returns the updated settings.           |
//! | `GET /api/profile`                  | Profile of the user: names, email address, read-only sessions. |
//! | `PUT /api/profile`                  | Changes the names and the read-only sessions, returns the profile. |
//! | `POST /api/beautify`                | Beautifies SQL (keywords in capitals...).                     |
//! | `POST /api/analyze`                 | Analyses the editor's text (see `analyze.rs`).                |
//! | `GET /api/dialects/{flavor}/keywords` | Keywords of the dialect (`?version=`: those of the version). |
//! | `GET /` and any other path          | Files of the interface (see `web_assets.rs`).                 |

use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::extract::ws::WebSocketUpgrade;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::middleware;
use axum::response::Response;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use prisme_connectors::mysql::{self, Flavor, ServerInfo};
use prisme_dialects::Dialect;
use prisme_dialects::analysis::CatalogNames;
use prisme_dialects::beautify::{BeautifyOptions, beautify_with};
use prisme_dialects::keywords::keywords;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::analyze::{self, Analysis, AnalyzeRequest, dialect_of};
use crate::api_error::ApiError;
use crate::auth::CurrentUser;
use crate::connections::{self, ConnectionSummary, NewConnection, ProbeRequest};
use crate::editors;
use crate::explorer;
use crate::master_key::MasterKey;
use crate::profile::{self, Profile, ProfileUpdate};
use crate::query_session;
use crate::recorder::Recorder;
use crate::request_guard;
use crate::sessions::Sessions;
use crate::settings::{self, Settings, SettingsUpdate};
use crate::web_assets;

/// State shared by all the routes.
#[derive(Clone)]
pub struct AppState {
    /// Internal database. `rusqlite` is synchronous: one access at a time, in a blocking task.
    db: Arc<Mutex<Connection>>,
    /// Master key, to encrypt and decrypt the passwords.
    key: Arc<MasterKey>,
    /// Single user of the workstation mode (see `auth.rs`).
    pub local_user_id: i64,
    /// Query sessions of the editors, kept open beyond their WebSocket.
    pub sessions: Arc<Sessions>,
}

impl AppState {
    /// Builds the state from the open internal database, the key and the local user.
    pub fn new(db: Connection, key: MasterKey, local_user_id: i64) -> Self {
        Self {
            db: Arc::new(Mutex::new(db)),
            key: Arc::new(key),
            local_user_id,
            sessions: Arc::default(),
        }
    }

    /// Runs `task` on the internal database in a blocking task, outside `tokio`'s threads.
    pub async fn with_db<T, E, F>(&self, task: F) -> Result<T, ApiError>
    where
        F: FnOnce(&Connection, &MasterKey) -> Result<T, E> + Send + 'static,
        E: Into<ApiError>,
        T: Send + 'static,
    {
        let db = Arc::clone(&self.db);
        let key = Arc::clone(&self.key);
        tokio::task::spawn_blocking(move || {
            let db = db.lock().map_err(|_| {
                tracing::error!("internal database lock poisoned");
                ApiError::Internal
            })?;
            task(&db, &key).map_err(Into::into)
        })
        .await
        .map_err(|error| {
            tracing::error!("internal database task interrupted: {error}");
            ApiError::Internal
        })?
    }
}

/// Response to the creation of a resource.
#[derive(Serialize, TS)]
#[ts(export)]
struct Created {
    /// Id of the created resource.
    id: i64,
}

/// Builds the router of the API.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route(
            "/api/connections",
            get(list_connections).post(create_connection),
        )
        .route("/api/connections/probe", post(probe_settings))
        .route(
            "/api/connections/{id}",
            put(update_connection).delete(delete_connection),
        )
        .route("/api/connections/{id}/probe", post(probe_connection))
        .route("/api/connections/{id}/query", get(query_socket))
        .route("/api/editors/{id}/session", get(editor_session))
        .route("/api/settings", get(get_settings).put(put_settings))
        .route("/api/profile", get(get_profile).put(put_profile))
        .route("/api/beautify", post(beautify_sql))
        .route("/api/analyze", post(analyze_sql))
        .route("/api/dialects/{flavor}/keywords", get(dialect_keywords))
        .merge(explorer::routes())
        .merge(editors::routes())
        .fallback(web_assets::serve)
        .layer(middleware::from_fn(request_guard::local_only))
        .with_state(state)
}

/// `GET /api/health`: answers `ok`.
async fn health() -> &'static str {
    "ok"
}

/// `GET /api/connections`: connections of the current user, without passwords.
async fn list_connections(
    State(state): State<AppState>,
    user: CurrentUser,
) -> Result<Json<Vec<ConnectionSummary>>, ApiError> {
    let summaries = state
        .with_db(move |db, _key| connections::list(db, user.id))
        .await?;
    Ok(Json(summaries))
}

/// `POST /api/connections`: saves a connection and returns its id (201).
async fn create_connection(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(new): Json<NewConnection>,
) -> Result<(StatusCode, Json<Created>), ApiError> {
    let id = state
        .with_db(move |db, key| connections::insert(db, key, user.id, &new))
        .await?;
    Ok((StatusCode::CREATED, Json(Created { id })))
}

/// `PUT /api/connections/{id}`: changes a connection of the current user (204).
/// An empty password keeps the saved password.
async fn update_connection(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<i64>,
    Json(changed): Json<NewConnection>,
) -> Result<StatusCode, ApiError> {
    let updated = state
        .with_db(move |db, key| connections::update(db, key, user.id, id, &changed))
        .await?;
    if updated {
        // The sessions opened with the old settings close.
        state.sessions.stop_connection(id);
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound)
    }
}

/// `DELETE /api/connections/{id}`: deletes a connection of the current user (204).
async fn delete_connection(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    let deleted = state
        .with_db(move |db, _key| connections::delete(db, user.id, id))
        .await?;
    if deleted {
        state.sessions.stop_connection(id);
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound)
    }
}

/// `GET /api/connections/{id}/query`: opens a query session over a
/// WebSocket, which closes with it (without a keep-alive delay).
async fn query_socket(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<i64>,
    upgrade: WebSocketUpgrade,
) -> Result<Response, ApiError> {
    let user_id = user.id;
    let (target, read_only) = state
        .with_db(move |db, key| {
            let target = connections::load_target(db, key, user_id, id)?;
            let read_only = profile::load(db, user_id)?.read_only_session;
            Ok::<_, ApiError>((target, read_only))
        })
        .await?;
    let target = connections::with_read_only(target.ok_or(ApiError::NotFound)?, read_only);
    Ok(upgrade.on_upgrade(move |socket| async move {
        let on_end = Box::new(|| {});
        query_session::start(socket, state, user_id, target, None, Duration::ZERO, on_end);
    }))
}

/// `GET /api/editors/{id}/session`: WebSocket, query session of an editor, on
/// the editor's connection; its results are saved. If the editor's session is
/// still open (page reloaded), the WebSocket reattaches to it; otherwise it
/// opens, kept after a disconnection for the delay set on the connection (see
/// `sessions.rs`).
async fn editor_session(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<i64>,
    upgrade: WebSocketUpgrade,
) -> Result<Response, ApiError> {
    let user_id = user.id;
    let connection_id = state
        .with_db(move |db, _| editors::connection_of(db, user_id, id))
        .await?
        .ok_or(ApiError::NotFound)?
        .ok_or_else(|| ApiError::BadRequest("no connection chosen for this editor".to_string()))?;
    let (target, minutes, read_only) = state
        .with_db(move |db, key| {
            let target = connections::load_target(db, key, user_id, connection_id)?;
            let minutes = connections::session_timeout(db, user_id, connection_id)?;
            let read_only = profile::load(db, user_id)?.read_only_session;
            Ok::<_, ApiError>(
                target
                    .zip(minutes)
                    .map(|(target, minutes)| (target, minutes, read_only)),
            )
        })
        .await?
        .ok_or(ApiError::NotFound)?;
    let target = connections::with_read_only(target, read_only);
    let keep_alive = Duration::from_secs(u64::from(minutes) * 60);
    Ok(upgrade.on_upgrade(move |socket| async move {
        let sessions = Arc::clone(&state.sessions);
        sessions.attach(id, connection_id, socket, |socket, on_end| {
            let recorder = Recorder::new(state.clone(), id);
            query_session::start(
                socket,
                state,
                user_id,
                target,
                Some(recorder),
                keep_alive,
                on_end,
            )
        });
    }))
}

/// Beautified SQL.
#[derive(Deserialize, Serialize, TS)]
#[ts(export)]
struct SqlText {
    /// SQL text, one or more queries.
    sql: String,
}

/// Names of the connection's catalog, as defined (with their case), for the
/// beautifier. Empty if the catalog is not loaded.
#[derive(Debug, Default, Deserialize, Serialize, TS)]
#[ts(export)]
#[serde(default)]
struct BeautifyCatalog {
    /// Databases.
    schemas: Vec<String>,
    /// Tables and views, all databases together.
    tables: Vec<String>,
    /// Columns, all tables together.
    columns: Vec<String>,
    /// Stored functions and procedures.
    routines: Vec<String>,
}

/// SQL to beautify, with the server's context.
#[derive(Deserialize, Serialize, TS)]
#[ts(export)]
struct BeautifyRequest {
    /// SQL text, one or more queries.
    sql: String,
    /// Names of the catalog: the names of the query take their spelling.
    #[serde(default)]
    #[ts(optional)]
    catalog: Option<BeautifyCatalog>,
    /// Flavor of the server, for its keywords; when absent, MySQL.
    #[serde(default)]
    #[ts(optional)]
    flavor: Option<Flavor>,
    /// Version announced by the server; when absent, all the dialect's keywords.
    #[serde(default)]
    #[ts(optional)]
    version: Option<String>,
}

/// `POST /api/beautify`: beautifies MySQL / MariaDB SQL (see `prisme_dialects::beautify`).
/// The work is CPU-bound: it runs in a blocking task, off the threads of `tokio`.
async fn beautify_sql(Json(request): Json<BeautifyRequest>) -> Result<Json<SqlText>, ApiError> {
    let sql = tokio::task::spawn_blocking(move || {
        let catalog = request.catalog.unwrap_or_default();
        let names = CatalogNames {
            schemas: catalog.schemas,
            tables: catalog.tables,
            columns: catalog.columns,
            routines: catalog.routines,
        };
        let options = BeautifyOptions {
            dialect: request.flavor.map_or(Dialect::MySql, dialect_of),
            version: request.version.as_deref(),
            catalog: &names,
        };
        beautify_with(&request.sql, options)
    })
    .await
    .map_err(|error| {
        tracing::error!("beautify task interrupted: {error}");
        ApiError::Internal
    })?;
    Ok(Json(SqlText { sql }))
}

/// Server version whose keywords are wanted.
#[derive(Debug, Deserialize)]
struct KeywordsQuery {
    /// Version announced by the server (`10.11.11-MariaDB`); when absent, all the words.
    version: Option<String>,
}

/// Keywords of a dialect, in capitals, by category
/// (`crates/dialects/keywords/*.yaml` files): the editor's highlighting sorts
/// them into its word classes.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
struct DialectKeywords {
    /// Reserved words.
    reserved: Vec<String>,
    /// Non-reserved keywords.
    keywords: Vec<String>,
    /// Column types.
    types: Vec<String>,
    /// Words that designate a value (`NULL`, `CURRENT_DATE`...).
    literals: Vec<String>,
}

/// `GET /api/dialects/{flavor}/keywords`: keywords of the dialect, limited to
/// those of the requested version.
async fn dialect_keywords(
    Path(flavor): Path<Flavor>,
    Query(query): Query<KeywordsQuery>,
) -> Json<DialectKeywords> {
    let words = keywords(dialect_of(flavor), query.version.as_deref());
    Json(DialectKeywords {
        reserved: words.reserved,
        keywords: words.keywords,
        types: words.types,
        literals: words.literals,
    })
}

/// `POST /api/analyze`: analyses the editor's text (see `analyze.rs`), in a
/// blocking task: the text is parsed on each pause of the typing.
async fn analyze_sql(Json(request): Json<AnalyzeRequest>) -> Result<Json<Analysis>, ApiError> {
    let analysis = tokio::task::spawn_blocking(move || analyze::analyze(&request))
        .await
        .map_err(|error| {
            tracing::error!("analysis task interrupted: {error}");
            ApiError::Internal
        })?;
    Ok(Json(analysis))
}

/// `GET /api/settings`: settings of the current user.
async fn get_settings(
    State(state): State<AppState>,
    user: CurrentUser,
) -> Result<Json<Settings>, ApiError> {
    let settings = state
        .with_db(move |db, _key| settings::load(db, user.id))
        .await?;
    Ok(Json(settings))
}

/// `PUT /api/settings`: changes the settings of the current user and returns them.
async fn put_settings(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(update): Json<SettingsUpdate>,
) -> Result<Json<Settings>, ApiError> {
    let settings = state
        .with_db(move |db, _key| settings::update(db, user.id, &update))
        .await?;
    Ok(Json(settings))
}

/// `GET /api/profile`: profile of the current user.
async fn get_profile(
    State(state): State<AppState>,
    user: CurrentUser,
) -> Result<Json<Profile>, ApiError> {
    let profile = state
        .with_db(move |db, _key| profile::load(db, user.id))
        .await?;
    Ok(Json(profile))
}

/// `PUT /api/profile`: changes the names of the current user and returns the profile.
async fn put_profile(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(update): Json<ProfileUpdate>,
) -> Result<Json<Profile>, ApiError> {
    let profile = state
        .with_db(move |db, _key| profile::update(db, user.id, &update))
        .await?;
    Ok(Json(profile))
}

/// `POST /api/connections/probe`: connects with typed settings, not saved yet,
/// and returns the server's identity.
async fn probe_settings(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(request): Json<ProbeRequest>,
) -> Result<Json<ServerInfo>, ApiError> {
    let target = state
        .with_db(move |db, key| connections::probe_target(db, key, user.id, request))
        .await?
        .ok_or(ApiError::NotFound)?;

    Ok(Json(mysql::probe(&target).await?))
}

/// `POST /api/connections/{id}/probe`: connects to the server and returns its identity.
async fn probe_connection(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<i64>,
) -> Result<Json<ServerInfo>, ApiError> {
    let user_id = user.id;
    let target = state
        .with_db(move |db, key| connections::load_target(db, key, user_id, id))
        .await?
        .ok_or(ApiError::NotFound)?;

    let info = mysql::probe(&target).await?;
    let flavor = info.flavor;
    state
        .with_db(move |db, _| connections::remember_flavor(db, user_id, id, flavor))
        .await?;
    Ok(Json(info))
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::Request;
    use serde_json::{Value, json};
    use tower::ServiceExt;

    use super::*;
    use crate::store;

    fn test_app() -> Router {
        let db = store::open_in_memory();
        let user_id = store::ensure_user(&db, "test@example.com").unwrap();
        router(AppState::new(db, MasterKey::generate(), user_id))
    }

    /// Sends a request to the application and returns the status and the JSON body (Null if empty or text).
    async fn send(
        app: &Router,
        method: &str,
        uri: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let request = Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json")
            .header("host", "localhost:8484")
            .body(body.map_or_else(Body::empty, |json| Body::from(json.to_string())))
            .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    fn connection_json(name: &str, port: u16) -> Value {
        json!({
            "name": name,
            "host": "127.0.0.1",
            "port": port,
            "username": "root",
            "password": "secret",
        })
    }

    #[tokio::test]
    async fn health_answers() {
        let (status, _) = send(&test_app(), "GET", "/api/health", None).await;
        assert_eq!(status, StatusCode::OK);
    }

    #[tokio::test]
    async fn unknown_api_path_is_not_found() {
        // The fallback to the interface's files does not hide a missing route.
        let (status, _) = send(&test_app(), "GET", "/api/absent", None).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn creates_renames_closes_and_reopens_editors() {
        let app = test_app();
        let (status, created) = send(&app, "POST", "/api/editors", Some(json!({}))).await;
        assert_eq!(status, StatusCode::CREATED);
        let id = created["id"].as_i64().unwrap();
        let uri = format!("/api/editors/{id}");
        let change = json!({ "name": "Sales", "sql": "SELECT 1", "connection_id": null });
        let (status, _) = send(&app, "PUT", &uri, Some(change)).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        let (_, editors) = send(&app, "GET", "/api/editors", None).await;
        assert_eq!(editors[0]["name"], "Sales");

        let (status, _) = send(&app, "DELETE", &uri, None).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        let (_, closed) = send(&app, "GET", "/api/editors/closed", None).await;
        assert_eq!(closed[0]["sql"], "SELECT 1");
        let (status, _) = send(&app, "POST", &format!("{uri}/reopen"), None).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        let (_, editors) = send(&app, "GET", "/api/editors", None).await;
        assert_eq!(editors.as_array().unwrap().len(), 1);

        // Without a chosen connection, the editor has no session.
        let (status, _) = send(&app, "GET", &format!("{uri}/session"), None).await;
        assert_ne!(status, StatusCode::OK);
    }

    #[tokio::test]
    async fn gives_the_keywords_of_a_dialect_and_version() {
        let app = test_app();
        let (status, all) = send(&app, "GET", "/api/dialects/mysql/keywords", None).await;
        assert_eq!(status, StatusCode::OK);
        let has = |body: &Value, category: &str, word: &str| {
            body[category].as_array().unwrap().contains(&json!(word))
        };
        assert!(has(&all, "reserved", "SELECT") && has(&all, "reserved", "WINDOW"));
        assert!(has(&all, "types", "VARCHAR") && has(&all, "literals", "NULL"));

        // MySQL 5.7: the reserved words that appeared with MySQL 8.0 are not kept.
        let uri = "/api/dialects/mysql/keywords?version=5.7.44";
        let (_, old) = send(&app, "GET", uri, None).await;
        assert!(has(&old, "reserved", "SELECT") && !has(&old, "reserved", "WINDOW"));

        let (status, _) = send(&app, "GET", "/api/dialects/mariadb/keywords", None).await;
        assert_eq!(status, StatusCode::OK);
    }

    #[tokio::test]
    async fn create_then_list_without_password() {
        let app = test_app();

        let (status, created) = send(
            &app,
            "POST",
            "/api/connections",
            Some(connection_json("prod", 3306)),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);
        assert!(created["id"].is_i64());

        let (status, list) = send(&app, "GET", "/api/connections", None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(list[0]["name"], "prod");
        assert!(!list.to_string().contains("secret"));
    }

    #[tokio::test]
    async fn delete_then_not_found() {
        let app = test_app();
        let (_, created) = send(
            &app,
            "POST",
            "/api/connections",
            Some(connection_json("prod", 3306)),
        )
        .await;
        let uri = format!("/api/connections/{}", created["id"]);

        let (status, _) = send(&app, "DELETE", &uri, None).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        let (status, _) = send(&app, "DELETE", &uri, None).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn update_then_list() {
        let app = test_app();
        let (_, created) = send(
            &app,
            "POST",
            "/api/connections",
            Some(connection_json("prod", 3306)),
        )
        .await;
        let uri = format!("/api/connections/{}", created["id"]);

        let (status, _) = send(&app, "PUT", &uri, Some(connection_json("staging", 3307))).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        let (_, list) = send(&app, "GET", "/api/connections", None).await;
        assert_eq!(list[0]["name"], "staging");
        assert_eq!(list[0]["port"], 3307);

        let (status, _) = send(
            &app,
            "PUT",
            "/api/connections/999",
            Some(connection_json("other", 3306)),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn probe_of_an_unknown_saved_connection_gives_not_found() {
        let mut connection = connection_json("prod", 3306);
        connection["password"] = json!("");
        let (status, _) = send(
            &test_app(),
            "POST",
            "/api/connections/probe",
            Some(json!({ "connection": connection, "saved_id": 999 })),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn duplicate_name_gives_conflict() {
        let app = test_app();
        send(
            &app,
            "POST",
            "/api/connections",
            Some(connection_json("prod", 3306)),
        )
        .await;

        let (status, body) = send(
            &app,
            "POST",
            "/api/connections",
            Some(connection_json("prod", 3306)),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert!(body["error"].as_str().unwrap().contains("prod"));
    }

    #[tokio::test]
    async fn invalid_input_gives_bad_request() {
        let (status, body) = send(
            &test_app(),
            "POST",
            "/api/connections",
            Some(connection_json("", 3306)),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(body["error"].as_str().unwrap().starts_with("name"));
    }

    #[tokio::test]
    async fn beautifies_sql() {
        let (status, body) = send(
            &test_app(),
            "POST",
            "/api/beautify",
            Some(json!({ "sql": "select a from t" })),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["sql"], "SELECT ALL a\nFROM t");
    }

    #[tokio::test]
    async fn beautifies_names_in_their_catalog_case() {
        let request = json!({
            "sql": "select now(), myfunction(ID) from CLIENTS",
            "catalog": { "tables": ["Clients"], "columns": ["id"], "routines": ["MyFunction"] },
            "flavor": "mariadb",
            "version": "10.11.11-MariaDB",
        });
        let (status, body) = send(&test_app(), "POST", "/api/beautify", Some(request)).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            body["sql"],
            "SELECT ALL\n    NOW(),\n    MyFunction(id)\nFROM Clients"
        );
    }

    #[tokio::test]
    async fn settings_round_trip() {
        let app = test_app();

        let (status, settings) = send(&app, "GET", "/api/settings", None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(settings["row_limit"], 1000);
        assert_eq!(
            settings["row_limit_choices"],
            json!([1000, 2000, 5000, 10000, 50000])
        );

        let (status, settings) = send(
            &app,
            "PUT",
            "/api/settings",
            Some(json!({ "row_limit": 10000 })),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(settings["row_limit"], 10000);

        let (status, _) = send(
            &app,
            "PUT",
            "/api/settings",
            Some(json!({ "row_limit": 3 })),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn probe_unknown_connection_gives_not_found() {
        let (status, _) = send(&test_app(), "POST", "/api/connections/42/probe", None).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn probe_unreachable_server_gives_bad_gateway() {
        let app = test_app();
        // Port 1: no server listens there, the connection is refused.
        let (_, created) = send(
            &app,
            "POST",
            "/api/connections",
            Some(connection_json("closed", 1)),
        )
        .await;
        let uri = format!("/api/connections/{}/probe", created["id"]);

        let (status, _) = send(&app, "POST", &uri, None).await;
        assert_eq!(status, StatusCode::BAD_GATEWAY);
    }
}
