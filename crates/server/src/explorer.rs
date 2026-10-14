//! Explorateur d'objets : catalogue d'une connexion enregistrée, envoyé au fil de l'eau.
//!
//! Route : `GET /api/connections/{id}/catalog`. La réponse est un flux de
//! lignes JSON (`application/x-ndjson`), un message `CatalogEvent` par ligne,
//! envoyé dès qu'il est prêt :
//!
//! 1. `schemas` : noms de toutes les bases, avec la variante du serveur
//!    (retenue aussi sur la connexion, pour le badge de l'explorateur) ;
//! 2. `objects` : tables, vues, procédures et fonctions, base par base ;
//! 3. `details` : colonnes, index, clefs étrangères et déclencheurs des
//!    tables, base par base ;
//! 4. `done`, ou `error` si une lecture échoue.
//!
//! L'interface affiche donc les bases presque aussitôt, puis les objets et
//! leur détail à mesure qu'ils arrivent. La base choisie dans la connexion
//! est lue en premier, les bases système (`information_schema`, `mysql`...)
//! en dernier.

use std::convert::Infallible;

use axum::Router;
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::header::CONTENT_TYPE;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use futures_util::stream;
use prisme_connectors::mysql::catalog::{self, SchemaObjects, TableDetails};
use prisme_connectors::mysql::{self, Connection, Flavor};
use serde::Serialize;
use tokio::sync::mpsc;
use ts_rs::TS;

use crate::api::AppState;
use crate::api_error::ApiError;
use crate::auth::CurrentUser;
use crate::connections;

/// Bases fournies par le serveur lui-même, lues en dernier.
const SYSTEM_SCHEMAS: [&str; 4] = ["information_schema", "mysql", "performance_schema", "sys"];

/// Nombre de messages en attente d'envoi au-delà duquel la lecture patiente.
const BUFFER: usize = 16;

/// Message du flux du catalogue.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CatalogEvent {
    /// Noms de toutes les bases, par ordre alphabétique, et variante du serveur.
    Schemas {
        schemas: Vec<String>,
        flavor: Flavor,
    },
    /// Objets d'une base : tables, vues, procédures et fonctions.
    Objects {
        schema: String,
        objects: SchemaObjects,
    },
    /// Détail des tables d'une base : colonnes, index, clefs étrangères, déclencheurs.
    Details {
        schema: String,
        tables: Vec<TableDetails>,
    },
    /// Échec d'une lecture ; le flux continue avec la base suivante.
    Error { message: String },
    /// Fin du catalogue.
    Done,
}

/// Routes de l'explorateur, à fusionner dans le routeur de l'API.
pub fn routes() -> Router<AppState> {
    Router::new().route("/api/connections/{id}/catalog", get(stream_catalog))
}

/// `GET /api/connections/{id}/catalog` : flux du catalogue du serveur.
///
/// La connexion au serveur cible s'ouvre avant le début du flux : un échec
/// de connexion reste une réponse HTTP en erreur (502, 504).
async fn stream_catalog(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<i64>,
) -> Result<Response, ApiError> {
    let user_id = user.id;
    let target = state
        .with_db(move |db, key| connections::load_target(db, key, user_id, id))
        .await?
        .ok_or(ApiError::NotFound)?;
    let mut connection = mysql::connect(&target).await?;
    let flavor = mysql::server_info(&mut connection).await?.flavor;
    state
        .with_db(move |db, _| connections::remember_flavor(db, user_id, id, flavor))
        .await?;

    let (sender, receiver) = mpsc::channel(BUFFER);
    tokio::spawn(async move {
        read_catalog(&mut connection, &target.database_name, flavor, &sender).await;
        let _ = connection.disconnect().await;
    });

    let lines = stream::unfold(receiver, |mut receiver| async move {
        let event = receiver.recv().await?;
        let line = format!(
            "{}
",
            serde_json::to_string(&event).expect("message sérialisable")
        );
        Some((Ok::<_, Infallible>(line), receiver))
    });
    Ok((
        [(CONTENT_TYPE, "application/x-ndjson")],
        Body::from_stream(lines),
    )
        .into_response())
}

/// Lit le catalogue base par base et envoie chaque partie dès qu'elle est lue.
///
/// S'arrête dès que le client a fermé la réponse.
async fn read_catalog(
    connection: &mut Connection,
    preferred: &str,
    flavor: Flavor,
    sender: &mpsc::Sender<CatalogEvent>,
) {
    let schemas = match catalog::schemas(connection).await {
        Ok(schemas) => schemas,
        Err(error) => {
            let _ = sender.send(error_event("bases", &error)).await;
            return;
        }
    };
    let order = reading_order(&schemas, preferred);
    if sender
        .send(CatalogEvent::Schemas { schemas, flavor })
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
            Err(error) => error_event(schema, &error),
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
            Err(error) => error_event(schema, &error),
        };
        if sender.send(event).await.is_err() {
            return;
        }
    }
    let _ = sender.send(CatalogEvent::Done).await;
}

/// Message d'erreur pour une lecture du catalogue en échec.
fn error_event(what: &str, error: &mysql::MysqlError) -> CatalogEvent {
    CatalogEvent::Error {
        message: format!("lecture du catalogue ({what}) impossible : {error}"),
    }
}

/// Ordre de lecture des bases : la base préférée, puis les bases de
/// l'utilisateur, puis les bases système.
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
    // Tri stable : l'ordre alphabétique est conservé à rang égal.
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
            init_sql: String::new(),
            session_timeout_minutes: 5,
            show_system_schemas: true,
        };
        connections::insert(&db, &key, user_id, &new).unwrap();
        let app = router(AppState::new(db, key, user_id));

        let request = Request::get(uri).body(Body::empty()).unwrap();
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
