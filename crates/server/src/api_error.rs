//! Erreurs de l'API et leur traduction en réponses HTTP.
//!
//! Chaque erreur devient un statut HTTP et un corps JSON `{"error": "message"}`.
//! Les erreurs internes sont détaillées dans le journal, jamais dans la
//! réponse.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use ts_rs::TS;

use prisme_connectors::mysql::MysqlError;

use crate::connections::ConnectionsError;
use crate::settings::SettingsError;

/// Corps JSON d'une réponse en erreur.
#[derive(Serialize, TS)]
#[ts(export)]
pub struct ErrorBody {
    /// Message lisible par l'utilisateur.
    pub error: String,
}

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

impl From<SettingsError> for ApiError {
    /// Traduit une erreur de gestion des réglages en erreur HTTP.
    fn from(error: SettingsError) -> Self {
        match error {
            SettingsError::InvalidRowLimit(_) => Self::BadRequest(error.to_string()),
            SettingsError::Sqlite(_) => {
                tracing::error!("{error}");
                Self::Internal
            }
        }
    }
}

impl From<MysqlError> for ApiError {
    /// Traduit un échec du serveur cible : 504 s'il n'a pas répondu à temps, 502 sinon.
    fn from(error: MysqlError) -> Self {
        match error {
            MysqlError::ConnectTimeout(message) => Self::GatewayTimeout(message),
            _ => Self::BadGateway(error.to_string()),
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
        (status, Json(ErrorBody { error: message })).into_response()
    }
}
