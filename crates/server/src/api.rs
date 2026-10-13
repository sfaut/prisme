//! API HTTP de Prisme, en JSON, servie par `axum`.
//!
//! Routes :
//!
//! | Méthode et chemin                   | Rôle                                                         |
//! |-------------------------------------|--------------------------------------------------------------|
//! | `GET /api/health`                   | Vérifie que le serveur répond.                               |
//! | `GET /api/connections`              | Liste les connexions de l'utilisateur, sans mot de passe.    |
//! | `POST /api/connections`             | Enregistre une connexion, mot de passe chiffré.              |
//! | `DELETE /api/connections/{id}`      | Supprime une connexion (204).                                |
//! | `POST /api/connections/{id}/probe`  | Se connecte et renvoie la variante et la version du serveur. |
//! | `GET /api/connections/{id}/query`   | WebSocket : session de requêtes (voir `query_session.rs`).   |
//! | `GET /api/connections/{id}/catalog` | Bases, tables et colonnes (voir `explorer.rs`).             |
//! | `GET /api/settings`                 | Réglages de l'utilisateur, avec les choix permis.            |
//! | `PUT /api/settings`                 | Modifie les réglages, renvoie les réglages à jour.           |
//! | `POST /api/beautify`                | Enjolive du SQL (mots-clefs en capitales...).                |

use std::sync::{Arc, Mutex};

use axum::extract::ws::WebSocketUpgrade;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::Response;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use prisme_connectors::mysql::{self, ServerInfo};
use prisme_dialects::Dialect;
use prisme_dialects::beautify::beautify_with_routines;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::api_error::ApiError;
use crate::auth::CurrentUser;
use crate::connections::{self, ConnectionSummary, NewConnection};
use crate::explorer;
use crate::master_key::MasterKey;
use crate::query_session;
use crate::settings::{self, Settings, SettingsUpdate};

/// État partagé par toutes les routes.
#[derive(Clone)]
pub struct AppState {
    /// Base interne. `rusqlite` est synchrone : un seul accès à la fois, dans une tâche bloquante.
    db: Arc<Mutex<Connection>>,
    /// Clef maîtresse, pour chiffrer et déchiffrer les mots de passe.
    key: Arc<MasterKey>,
    /// Utilisateur unique du mode poste de développement (voir `auth.rs`).
    pub local_user_id: i64,
}

impl AppState {
    /// Construit l'état à partir de la base interne ouverte, de la clef et de l'utilisateur local.
    pub fn new(db: Connection, key: MasterKey, local_user_id: i64) -> Self {
        Self {
            db: Arc::new(Mutex::new(db)),
            key: Arc::new(key),
            local_user_id,
        }
    }

    /// Exécute `task` sur la base interne dans une tâche bloquante, hors des threads de `tokio`.
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
                tracing::error!("verrou de la base interne empoisonné");
                ApiError::Internal
            })?;
            task(&db, &key).map_err(Into::into)
        })
        .await
        .map_err(|error| {
            tracing::error!("tâche de la base interne interrompue : {error}");
            ApiError::Internal
        })?
    }
}

/// Réponse à la création d'une ressource.
#[derive(Serialize, TS)]
#[ts(export)]
struct Created {
    /// Identifiant de la ressource créée.
    id: i64,
}

/// Construit le routeur de l'API.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route(
            "/api/connections",
            get(list_connections).post(create_connection),
        )
        .route("/api/connections/{id}", delete(delete_connection))
        .route("/api/connections/{id}/probe", post(probe_connection))
        .route("/api/connections/{id}/query", get(query_socket))
        .route("/api/settings", get(get_settings).put(put_settings))
        .route("/api/beautify", post(beautify_sql))
        .merge(explorer::routes())
        .with_state(state)
}

/// `GET /api/health` : répond `ok`.
async fn health() -> &'static str {
    "ok"
}

/// `GET /api/connections` : connexions de l'utilisateur courant, sans mot de passe.
async fn list_connections(
    State(state): State<AppState>,
    user: CurrentUser,
) -> Result<Json<Vec<ConnectionSummary>>, ApiError> {
    let summaries = state
        .with_db(move |db, _key| connections::list(db, user.id))
        .await?;
    Ok(Json(summaries))
}

/// `POST /api/connections` : enregistre une connexion et renvoie son identifiant (201).
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

/// `DELETE /api/connections/{id}` : supprime une connexion de l'utilisateur courant (204).
async fn delete_connection(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    let deleted = state
        .with_db(move |db, _key| connections::delete(db, user.id, id))
        .await?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound)
    }
}

/// `GET /api/connections/{id}/query` : ouvre une session de requêtes par WebSocket.
async fn query_socket(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<i64>,
    upgrade: WebSocketUpgrade,
) -> Result<Response, ApiError> {
    let user_id = user.id;
    let target = state
        .with_db(move |db, key| connections::load_target(db, key, user_id, id))
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(upgrade.on_upgrade(move |socket| query_session::run(socket, state, user_id, target)))
}

/// SQL enjolivé.
#[derive(Deserialize, Serialize, TS)]
#[ts(export)]
struct SqlText {
    /// Texte SQL, une ou plusieurs requêtes.
    sql: String,
}

/// SQL à enjoliver.
#[derive(Deserialize, Serialize, TS)]
#[ts(export)]
struct BeautifyRequest {
    /// Texte SQL, une ou plusieurs requêtes.
    sql: String,
    /// Noms des fonctions et procédures de l'utilisateur, tels que définis
    /// (catalogue de la connexion) : ils gardent leur casse. Vide si le
    /// catalogue n'est pas chargé.
    #[serde(default)]
    routines: Vec<String>,
}

/// `POST /api/beautify` : enjolive du SQL MySQL / MariaDB (voir `prisme_dialects::beautify`).
async fn beautify_sql(Json(request): Json<BeautifyRequest>) -> Json<SqlText> {
    Json(SqlText {
        sql: beautify_with_routines(&request.sql, Dialect::MySql, &request.routines),
    })
}

/// `GET /api/settings` : réglages de l'utilisateur courant.
async fn get_settings(
    State(state): State<AppState>,
    user: CurrentUser,
) -> Result<Json<Settings>, ApiError> {
    let settings = state
        .with_db(move |db, _key| settings::load(db, user.id))
        .await?;
    Ok(Json(settings))
}

/// `PUT /api/settings` : modifie les réglages de l'utilisateur courant et les renvoie.
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

/// `POST /api/connections/{id}/probe` : se connecte au serveur et renvoie son identité.
async fn probe_connection(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<i64>,
) -> Result<Json<ServerInfo>, ApiError> {
    let target = state
        .with_db(move |db, key| connections::load_target(db, key, user.id, id))
        .await?
        .ok_or(ApiError::NotFound)?;

    Ok(Json(mysql::probe(&target).await?))
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

    /// Envoie une requête à l'application et renvoie le statut et le corps JSON (Null si vide ou texte).
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
    async fn beautifies_user_functions_in_their_defined_case() {
        let (status, body) = send(
            &test_app(),
            "POST",
            "/api/beautify",
            Some(json!({ "sql": "select now(), mafonction(1)", "routines": ["MaFonction"] })),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["sql"], "SELECT ALL\n    NOW(),\n    MaFonction(1)");
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
        // Port 1 : aucun serveur n'y écoute, la connexion est refusée.
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
