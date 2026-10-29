//! Errors of the API and their translation into HTTP responses.
//!
//! Each error becomes an HTTP status and a JSON body `{"error": "message",
//! "detail": {...}}`: the English message for the logs, the detail
//! (`ErrorDetail`) for the interface, which writes its own message from it.
//! Internal errors are detailed in the log, never in the response.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use ts_rs::TS;

use prisme_connectors::mysql::{ConnectFailure, MysqlError};

use crate::connections::ConnectionsError;
use crate::editors::EditorsError;
use crate::error_detail::ErrorDetail;
use crate::exports::ExportsError;
use crate::profile::ProfileError;
use crate::settings::SettingsError;

/// JSON body of an error response.
#[derive(Serialize, TS)]
#[ts(export)]
pub struct ErrorBody {
    /// Message in English, as the logs keep it.
    pub error: String,
    /// What went wrong, for the interface to tell the user.
    pub detail: ErrorDetail,
}

/// Error returned by an API route.
#[derive(Debug)]
pub enum ApiError {
    /// Invalid input (400).
    BadRequest(ErrorDetail),
    /// Missing resource, or one belonging to another user (404).
    NotFound,
    /// Conflict with what exists, for example a name already taken (409).
    Conflict(ErrorDetail),
    /// Resource gone, for example the file of an export deleted outside Prisme (410).
    Gone(ErrorDetail),
    /// The target server refused or failed (502).
    BadGateway(ErrorDetail),
    /// The target server did not answer in time (504).
    GatewayTimeout(ErrorDetail),
    /// Internal error, already written to the log (500).
    Internal,
}

impl From<ConnectionsError> for ApiError {
    /// Translates a connection management error into an HTTP error.
    fn from(error: ConnectionsError) -> Self {
        match error {
            ConnectionsError::Invalid(detail) => Self::BadRequest(detail),
            ConnectionsError::DuplicateName(name) => {
                Self::Conflict(ErrorDetail::DuplicateConnectionName { name })
            }
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
            EditorsError::Invalid(detail) => Self::BadRequest(detail),
            EditorsError::Sqlite(_) | EditorsError::Json(_) => {
                tracing::error!("{error}");
                Self::Internal
            }
        }
    }
}

impl From<ExportsError> for ApiError {
    /// Translates an export management error into an HTTP error: a failure
    /// of the exports folder is told to the user, with the system's message.
    fn from(error: ExportsError) -> Self {
        match error {
            ExportsError::Folder(cause) => Self::Conflict(ErrorDetail::ExportFile {
                cause: cause.to_string(),
            }),
            ExportsError::Sqlite(_) => {
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
            SettingsError::InvalidRowLimit(_)
            | SettingsError::RetentionOutOfRange { .. }
            | SettingsError::IndentSizeOutOfRange => Self::BadRequest(error.detail()),
            SettingsError::Sqlite(_) => {
                tracing::error!("{error}");
                Self::Internal
            }
        }
    }
}

impl From<ProfileError> for ApiError {
    /// Translates a profile management error into an HTTP error.
    fn from(error: ProfileError) -> Self {
        match error {
            ProfileError::NameTooLong { field, length } => {
                Self::BadRequest(ErrorDetail::FieldLength {
                    field,
                    min: 0,
                    max: crate::profile::NAME_MAX_CHARS as u32,
                    length,
                })
            }
            ProfileError::Sqlite(_) => {
                tracing::error!("{error}");
                Self::Internal
            }
        }
    }
}

impl From<MysqlError> for ApiError {
    /// Translates a failure of the target server: 504 if it did not answer in
    /// time, 502 otherwise. The flavor of the server is not known yet.
    fn from(error: MysqlError) -> Self {
        let detail = ErrorDetail::of_mysql(&error, None);
        match error {
            MysqlError::Connect(ConnectFailure::Timeout { .. }) => Self::GatewayTimeout(detail),
            _ => Self::BadGateway(detail),
        }
    }
}

impl IntoResponse for ApiError {
    /// Builds the HTTP response: status and JSON body `{"error": "message", "detail": {...}}`.
    fn into_response(self) -> Response {
        let (status, detail) = match self {
            Self::BadRequest(detail) => (StatusCode::BAD_REQUEST, detail),
            Self::NotFound => (StatusCode::NOT_FOUND, ErrorDetail::NotFound),
            Self::Conflict(detail) => (StatusCode::CONFLICT, detail),
            Self::Gone(detail) => (StatusCode::GONE, detail),
            Self::BadGateway(detail) => (StatusCode::BAD_GATEWAY, detail),
            Self::GatewayTimeout(detail) => (StatusCode::GATEWAY_TIMEOUT, detail),
            Self::Internal => (StatusCode::INTERNAL_SERVER_ERROR, ErrorDetail::Internal),
        };
        let body = ErrorBody {
            error: detail.to_string(),
            detail,
        };
        (status, Json(body)).into_response()
    }
}
