//! Current user of a request.
//!
//! It is the only place that decides on whose behalf a request runs (see
//! `docs/V1.md`, section 2.8, rule 3). Routes receive it as a parameter
//! (`user: CurrentUser`), like an `axum` extractor.
//!
//! In workstation mode, it is always the local user. The server mode will
//! replace this code by checking a session, without touching the routes.

use std::convert::Infallible;

use axum::extract::FromRequestParts;
use axum::http::request::Parts;

use crate::api::AppState;

/// User on whose behalf the request runs.
pub struct CurrentUser {
    /// Id in the `users` table.
    pub id: i64,
}

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = Infallible;

    /// Returns the local user, the only one in workstation mode.
    async fn from_request_parts(
        _parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        Ok(Self {
            id: state.local_user_id,
        })
    }
}
