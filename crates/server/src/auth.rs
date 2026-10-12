//! Utilisateur courant d'une requête.
//!
//! C'est le seul endroit qui décide pour le compte de qui une requête s'exécute
//! (voir `docs/V1.md`, section 2.8, règle 3). Les routes le reçoivent en
//! paramètre (`user: CurrentUser`), comme un extracteur `axum`.
//!
//! En mode poste de développement, c'est toujours l'utilisateur local. Le mode
//! serveur remplacera ce code par la vérification d'une session, sans toucher
//! aux routes.

use std::convert::Infallible;

use axum::extract::FromRequestParts;
use axum::http::request::Parts;

use crate::api::AppState;

/// Utilisateur pour le compte duquel la requête s'exécute.
pub struct CurrentUser {
    /// Identifiant dans la table `users`.
    pub id: i64,
}

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = Infallible;

    /// Renvoie l'utilisateur local, unique en mode poste de développement.
    async fn from_request_parts(
        _parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        Ok(Self {
            id: state.local_user_id,
        })
    }
}
