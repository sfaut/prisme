//! Explorateur d'objets : bases, tables et colonnes d'une connexion enregistrée.
//!
//! Routes :
//!
//! | Méthode et chemin                                            | Rôle                          |
//! |--------------------------------------------------------------|-------------------------------|
//! | `GET /api/connections/{id}/schemas`                          | Bases du serveur.             |
//! | `GET /api/connections/{id}/schemas/{schema}/tables`          | Tables et vues d'une base.    |
//! | `GET /api/connections/{id}/schemas/{schema}/tables/{table}/columns` | Colonnes d'une table.  |
//!
//! Chaque appel ouvre une connexion courte au serveur cible, lit le catalogue,
//! puis la referme. L'interface charge l'arbre niveau par niveau, à la
//! demande.

use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};
use prisme_connectors::mysql::catalog::{self, ColumnInfo, TableInfo};
use prisme_connectors::mysql::{self, Connection, MysqlError};

use crate::api::AppState;
use crate::api_error::ApiError;
use crate::auth::CurrentUser;
use crate::connections;

/// Routes de l'explorateur, à fusionner dans le routeur de l'API.
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/connections/{id}/schemas", get(list_schemas))
        .route(
            "/api/connections/{id}/schemas/{schema}/tables",
            get(list_tables),
        )
        .route(
            "/api/connections/{id}/schemas/{schema}/tables/{table}/columns",
            get(list_columns),
        )
}

/// `GET /api/connections/{id}/schemas` : bases du serveur.
async fn list_schemas(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<i64>,
) -> Result<Json<Vec<String>>, ApiError> {
    let mut connection = open(&state, user.id, id).await?;
    let result = catalog::schemas(&mut connection).await;
    close(connection, result).await
}

/// `GET /api/connections/{id}/schemas/{schema}/tables` : tables et vues d'une base.
async fn list_tables(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((id, schema)): Path<(i64, String)>,
) -> Result<Json<Vec<TableInfo>>, ApiError> {
    let mut connection = open(&state, user.id, id).await?;
    let result = catalog::tables(&mut connection, &schema).await;
    close(connection, result).await
}

/// `GET /api/connections/{id}/schemas/{schema}/tables/{table}/columns` : colonnes d'une table.
async fn list_columns(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((id, schema, table)): Path<(i64, String, String)>,
) -> Result<Json<Vec<ColumnInfo>>, ApiError> {
    let mut connection = open(&state, user.id, id).await?;
    let result = catalog::columns(&mut connection, &schema, &table).await;
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
            status_of("/api/connections/42/schemas").await,
            StatusCode::NOT_FOUND
        );
    }

    #[tokio::test]
    async fn unreachable_server_gives_bad_gateway() {
        for uri in [
            "/api/connections/1/schemas",
            "/api/connections/1/schemas/test/tables",
            "/api/connections/1/schemas/test/tables/t%201/columns",
        ] {
            assert_eq!(status_of(uri).await, StatusCode::BAD_GATEWAY, "{uri}");
        }
    }
}
