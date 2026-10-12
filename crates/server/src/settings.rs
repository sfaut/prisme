//! Réglages de l'utilisateur.
//!
//! Pour l'instant : la limite de lignes par défaut, ajoutée aux requêtes qui
//! n'en écrivent pas (voir `docs/V1.md`, section 2.3). Fonctions synchrones
//! (`rusqlite`), appelées depuis des tâches bloquantes.

use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Limites de lignes proposées à l'utilisateur. Seul endroit à modifier pour changer la liste.
pub const ROW_LIMIT_CHOICES: [u32; 5] = [1_000, 2_000, 5_000, 10_000, 50_000];

/// Réglages d'un utilisateur, tels que renvoyés par l'API.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct Settings {
    /// Limite de lignes par défaut.
    pub row_limit: u32,
    /// Limites permises, pour construire la liste de choix de l'interface.
    #[ts(type = "Array<number>")]
    pub row_limit_choices: &'static [u32],
}

/// Modification des réglages demandée par l'utilisateur.
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct SettingsUpdate {
    /// Nouvelle limite de lignes par défaut, parmi `ROW_LIMIT_CHOICES`.
    pub row_limit: u32,
}

/// Erreurs de gestion des réglages.
#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    /// Limite hors de la liste des choix.
    #[error("row_limit : {0} n'est pas permis, choix possibles : {choices:?}", choices = ROW_LIMIT_CHOICES)]
    InvalidRowLimit(u32),

    /// Erreur de la base interne.
    #[error("base interne : {0}")]
    Sqlite(#[from] rusqlite::Error),
}

/// Vérifie qu'une limite de lignes fait partie des choix permis.
pub fn check_row_limit(row_limit: u32) -> Result<u32, SettingsError> {
    if ROW_LIMIT_CHOICES.contains(&row_limit) {
        Ok(row_limit)
    } else {
        Err(SettingsError::InvalidRowLimit(row_limit))
    }
}

/// Lit les réglages de l'utilisateur `user_id`.
pub fn load(db: &Connection, user_id: i64) -> Result<Settings, SettingsError> {
    let row_limit = db.query_row(
        "SELECT row_limit FROM users WHERE id = ?1",
        params![user_id],
        |row| row.get(0),
    )?;
    Ok(Settings {
        row_limit,
        row_limit_choices: &ROW_LIMIT_CHOICES,
    })
}

/// Modifie les réglages de l'utilisateur `user_id` et renvoie les réglages à jour.
pub fn update(
    db: &Connection,
    user_id: i64,
    update: &SettingsUpdate,
) -> Result<Settings, SettingsError> {
    let row_limit = check_row_limit(update.row_limit)?;
    db.execute(
        "UPDATE users SET row_limit = ?1 WHERE id = ?2",
        params![row_limit, user_id],
    )?;
    load(db, user_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store;

    #[test]
    fn default_row_limit_is_1000() {
        let db = store::open_in_memory();
        let user_id = store::ensure_user(&db, "a@example.com").unwrap();
        assert_eq!(load(&db, user_id).unwrap().row_limit, 1_000);
    }

    #[test]
    fn update_accepts_only_the_choices() {
        let db = store::open_in_memory();
        let user_id = store::ensure_user(&db, "a@example.com").unwrap();

        let settings = update(&db, user_id, &SettingsUpdate { row_limit: 5_000 }).unwrap();
        assert_eq!(settings.row_limit, 5_000);

        assert!(matches!(
            update(&db, user_id, &SettingsUpdate { row_limit: 1_234 }),
            Err(SettingsError::InvalidRowLimit(1_234))
        ));
        assert_eq!(load(&db, user_id).unwrap().row_limit, 5_000);
    }
}
