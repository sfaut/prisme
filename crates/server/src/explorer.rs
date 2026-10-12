//! Explorateur d'objets : catalogue complet d'une connexion enregistrée.
//!
//! Route : `GET /api/connections/{id}/catalog`, qui renvoie toutes les bases,
//! avec leurs tables et leurs colonnes. L'interface charge le catalogue une
//! fois, à l'ouverture de la connexion, puis le parcourt sans autre appel.

use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};
use prisme_connectors::mysql::catalog::{self, SchemaInfo};
use prisme_connectors::mysql::{self, Connection, MysqlError};

use crate::api::AppState;
use crate::api_error::ApiError;
use crate::auth::CurrentUser;
use crate::connections;

/// Routes de l'explorateur, à fusionner dans le routeur de l'API.
pub fn routes() -> Router<AppState> {
    Router::new().route("/api/connections/{id}/catalog", get(load_catalog))
}

/// `GET /api/connections/{id}/catalog` : bases, tables et colonnes du serveur.
async fn load_catalog(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<i64>,
) -> Result<Json<Vec<SchemaInfo>>, ApiError> {
    let mut connection = open(&state, user.id, id).await?;
    let result = catalog::load(&mut connection).await;
    close(connection, result).await
}

/// Ouvre une connexion au serveur de la connexion enregistrée `id`.
async fn open(state: &AppState, user_id: i64, id: i64) -> Result<Connection, ApiError> {
    let target = state
        .with_db(move |db, key| connections::load_target(db, key, user_id, id))
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(mysql::connect(&target).await?)
}

/// Referme la connexion et met en forme le résultat de la lecture du catalogue.
async fn close<T>(
    connection: Connection,
    result: Result<T, MysqlError>,
) -> Result<Json<T>, ApiError> {
    // Une erreur de déconnexion n'enlève rien au résultat déjà lu.
    let _ = connection.disconnect().await;
    Ok(Json(result?))
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

    /// Envoie `GET uri` à une application dont la connexion 1 vise un port fermé.
    async fn status_of(uri: &str) -> StatusCode {
        let db = store::open_in_memory();
        let key = MasterKey::generate();
        let user_id = store::ensure_user(&db, "test@example.com").unwrap();
        let new = NewConnection {
            name: "fermée".to_string(),
            host: "127.0.0.1".to_string(),
            // Port 1 : aucun serveur n'y écoute.
            port: 1,
            username: "root".to_string(),
            password: SecretString::from(""),
            database_name: String::new(),
        };
        connections::insert(&db, &key, user_id, &new).unwrap();
        let app = router(AppState::new(db, key, user_id));

        let request = Request::get(uri).body(Body::empty()).unwrap();
        app.oneshot(request).await.unwrap().status()
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
