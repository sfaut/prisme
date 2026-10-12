//! Erreurs de l'API et leur traduction en réponses HTTP.
//!
//! Chaque erreur devient un statut HTTP et un corps JSON `{"error": "message"}`.
//! Les erreurs internes sont détaillées dans le journal, jamais dans la
//! réponse.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;

use crate::connections::ConnectionsError;

/// Erreur renvoyée par une route de l'API.
#[derive(Debug)]
pub enum ApiError {
    /// Saisie invalide (400).
    BadRequest(String),
    /// Ressource absente, ou appartenant à un autre utilisateur (404).
    NotFound,
    /// Conflit avec l'existant, par exemple un nom déjà pris (409).
    Conflict(String),
    /// Le serveur cible a refusé ou échoué (502).
    BadGateway(String),
    /// Le serveur cible n'a pas répondu à temps (504).
    GatewayTimeout(String),
    /// Erreur interne, déjà inscrite au journal (500).
    Internal,
}

impl From<ConnectionsError> for ApiError {
    /// Traduit une erreur de gestion des connexions en erreur HTTP.
    fn from(error: ConnectionsError) -> Self {
        match error {
            ConnectionsError::Invalid(message) => Self::BadRequest(message),
            ConnectionsError::DuplicateName(_) => Self::Conflict(error.to_string()),
            ConnectionsError::Secret(_) | ConnectionsError::Sqlite(_) => {
                tracing::error!("{error}");
                Self::Internal
            }
        }
    }
}

impl IntoResponse for ApiError {
    /// Construit la réponse HTTP : statut et corps JSON `{"error": "message"}`.
    fn into_response(self) -> Response {
        let (status, message) = match self {
            Self::BadRequest(message) => (StatusCode::BAD_REQUEST, message),
            Self::NotFound => (StatusCode::NOT_FOUND, "ressource introuvable".to_string()),
            Self::Conflict(message) => (StatusCode::CONFLICT, message),
            Self::BadGateway(message) => (StatusCode::BAD_GATEWAY, message),
            Self::GatewayTimeout(message) => (StatusCode::GATEWAY_TIMEOUT, message),
            Self::Internal => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "erreur interne, voir le journal du serveur".to_string(),
            ),
        };
        (status, Json(json!({ "error": message }))).into_response()
    }
}
