//! Errors of the API and their translation into HTTP responses.
//!
//! Each error becomes an HTTP status and a JSON body `{"error": "message"}`.
//! Internal errors are detailed in the log, never in the response.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use ts_rs::TS;

use prisme_connectors::mysql::MysqlError;

use crate::connections::ConnectionsError;
use crate::editors::EditorsError;
use crate::settings::SettingsError;

/// JSON body of an error response.
#[derive(Serialize, TS)]
#[ts(export)]
pub struct ErrorBody {
    /// Message readable by the user.
    pub error: String,
}

/// Error returned by an API route.
#[derive(Debug)]
pub enum ApiError {
    /// Invalid input (400).
    BadRequest(String),
    /// Missing resource, or one belonging to another user (404).
    NotFound,
    /// Conflict with what exists, for example a name already taken (409).
    Conflict(String),
    /// The target server refused or failed (502).
    BadGateway(String),
    /// The target server did not answer in time (504).
    GatewayTimeout(String),
    /// Internal error, already written to the log (500).
    Internal,
}

impl From<ConnectionsError> for ApiError {
    /// Translates a connection management error into an HTTP error.
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

impl From<EditorsError> for ApiError {
    /// Translates an editor management error into an HTTP error.
    fn from(error: EditorsError) -> Self {
        match error {
            EditorsError::Invalid(message) => Self::BadRequest(message),
            EditorsError::Sqlite(_) | EditorsError::Json(_) => {
                tracing::error!("{error}");
                Self::Internal
            }
        }
    }
}

impl From<SettingsError> for ApiError {
    /// Translates a settings management error into an HTTP error.
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
    /// Translates a failure of the target server: 504 if it did not answer in time, 502 otherwise.
    fn from(error: MysqlError) -> Self {
        match error {
            MysqlError::ConnectTimeout(message) => Self::GatewayTimeout(message),
            _ => Self::BadGateway(error.to_string()),
        }
    }
}

impl IntoResponse for ApiError {
    /// Builds the HTTP response: status and JSON body `{"error": "message"}`.
    fn into_response(self) -> Response {
        let (status, message) = match self {
            Self::BadRequest(message) => (StatusCode::BAD_REQUEST, message),
            Self::NotFound => (StatusCode::NOT_FOUND, "resource not found".to_string()),
            Self::Conflict(message) => (StatusCode::CONFLICT, message),
            Self::BadGateway(message) => (StatusCode::BAD_GATEWAY, message),
            Self::GatewayTimeout(message) => (StatusCode::GATEWAY_TIMEOUT, message),
            Self::Internal => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal error, see the server log".to_string(),
            ),
        };
        (status, Json(ErrorBody { error: message })).into_response()
    }
}
