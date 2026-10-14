//! Éditeurs SQL de l'utilisateur (onglets du haut de l'écran) et leurs
//! onglets de résultat, enregistrés dans la base interne : tout survit à un
//! rafraîchissement de la page ou à la fermeture du navigateur.
//!
//! Chaque éditeur a son texte, sa connexion et sa propre session de
//! requêtes (`GET /api/editors/{id}/session`). Les résultats sont enregistrés
//! par le serveur lui-même au fil de l'exécution (`recorder.rs`) : le
//! navigateur ne les renvoie jamais. Fermer un éditeur le marque comme fermé
//! (`closed_at`) ; il reste rouvrable 24 heures, puis il est purgé.
//!
//! Routes :
//!
//! | Méthode et chemin                       | Rôle                                                    |
//! |-----------------------------------------|---------------------------------------------------------|
//! | `GET /api/editors`                      | Éditeurs ouverts, dans l'ordre.                         |
//! | `POST /api/editors`                     | Crée un éditeur vide, à la suite des autres.            |
//! | `PUT /api/editors/{id}`                 | Change le nom, le texte ou la connexion (204).          |
//! | `DELETE /api/editors/{id}`              | Ferme l'éditeur ; rouvrable 24 heures (204).            |
//! | `PUT /api/editors/order`                | Range les éditeurs ouverts dans l'ordre donné (204).    |
//! | `GET /api/editors/closed`               | Éditeurs fermés depuis moins de 24 heures.              |
//! | `POST /api/editors/{id}/reopen`         | Rouvre un éditeur fermé (204).                          |
//! | `GET /api/editors/{id}/results`         | Onglets de résultat de l'éditeur, lignes comprises.     |
//! | `PUT /api/editors/{id}/results/order`   | Range les onglets de résultat dans l'ordre donné (204). |
//! | `PUT /api/results/{id}`                 | Renomme ou épingle un onglet de résultat (204).         |
//! | `DELETE /api/results/{id}`              | Supprime un onglet de résultat et ses lignes (204).     |

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use prisme_connectors::mysql::Cell;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::api::AppState;
use crate::api_error::ApiError;
use crate::auth::CurrentUser;
use crate::query_session::LimitReport;

/// Longueur maximale du nom d'un éditeur ou d'un onglet de résultat, en caractères.
const NAME_MAX_LENGTH: usize = 100;

/// Durée pendant laquelle un éditeur fermé reste rouvrable, au format des
/// modificateurs de date de SQLite.
const REOPEN_DELAY: &str = "-1 day";

/// Instant présent, au format des colonnes de date de la base interne.
const NOW: &str = "strftime('%Y-%m-%dT%H:%M:%fZ', 'now')";

/// Erreurs de gestion des éditeurs.
#[derive(Debug, thiserror::Error)]
pub enum EditorsError {
    /// Saisie invalide : le message dit quel champ et pourquoi.
    #[error("{0}")]
    Invalid(String),

    /// Erreur de la base interne.
    #[error("base interne : {0}")]
    Sqlite(#[from] rusqlite::Error),

    /// Résultat enregistré illisible.
    #[error("résultat enregistré illisible : {0}")]
    Json(#[from] serde_json::Error),
}

/// Éditeur ouvert, tel que l'interface le reçoit.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct Editor {
    /// Identifiant de l'éditeur.
    pub id: i64,
    /// Nom donné par l'utilisateur, chaîne vide pour le nom par défaut.
    pub name: String,
    /// Connexion de l'éditeur, absente tant qu'aucune n'est choisie.
    pub connection_id: Option<i64>,
    /// Texte SQL de l'éditeur.
    pub sql: String,
}

/// Onglet de résultat enregistré.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct ResultTab {
    /// Identifiant de l'onglet.
    pub id: i64,
    /// Numéro de la requête dans son lot.
    pub query: u32,
    /// Texte de la requête.
    pub sql: String,
    /// Nom donné par l'utilisateur, chaîne vide pour le libellé par défaut.
    pub name: String,
    /// Onglet épinglé : il reste quand les requêtes sont relancées.
    pub pinned: bool,
    /// État du résultat.
    pub result: SavedResult,
    /// Lignes reçues.
    pub rows: Vec<Vec<Cell>>,
}

/// Colonnes d'un résultat et ce qui les accompagne (message `columns`).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SavedColumns {
    /// Noms des colonnes.
    pub columns: Vec<String>,
    /// Pour chaque colonne, ses valeurs s'alignent-elles à droite ?
    pub right_aligned: Vec<bool>,
    /// Texte réellement exécuté.
    pub sql: String,
    /// Traitement de la limite de lignes.
    pub limit: LimitReport,
    /// Nombre de lignes au plus, `null` si inconnu.
    pub expected_rows: Option<u64>,
    /// Temps d'exécution mesuré par le serveur, en microsecondes.
    pub execution_us: u64,
}

/// État enregistré d'un résultat, rempli au fil des messages de la session.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(default)]
pub struct SavedResult {
    /// La requête est-elle partie ?
    pub started: bool,
    /// La requête est-elle terminée, avec succès ou en échec ?
    pub done: bool,
    /// Message d'échec, absent si la requête a réussi ou tourne encore.
    pub error: Option<String>,
    /// Colonnes, absentes tant qu'elles ne sont pas reçues (ou sans résultat).
    pub columns: Option<SavedColumns>,
    /// Nombre de lignes reçues.
    pub row_count: usize,
    /// Le résultat a-t-il été tronqué par la limite ?
    pub truncated: bool,
    /// Lignes modifiées par une écriture.
    pub affected_rows: u64,
    /// Temps de transfert des lignes, mesuré par le serveur, en microsecondes.
    pub transfer_us: u64,
    /// Volume des messages du résultat, en octets.
    pub bytes: u64,
}

/// Éditeur fermé depuis moins de 24 heures, rouvrable.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct ClosedEditor {
    /// Identifiant de l'éditeur.
    pub id: i64,
    /// Nom donné par l'utilisateur, chaîne vide pour le nom par défaut.
    pub name: String,
    /// Début du texte SQL, pour le reconnaître.
    pub sql: String,
    /// Date de fermeture, en UTC au format ISO 8601.
    pub closed_at: String,
}

/// Modification d'un éditeur : nom, texte et connexion, tous renvoyés.
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct EditorUpdate {
    /// Nom, chaîne vide pour le nom par défaut.
    pub name: String,
    /// Texte SQL.
    pub sql: String,
    /// Connexion, absente pour aucune.
    pub connection_id: Option<i64>,
}

/// Modification d'un onglet de résultat.
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct ResultTabUpdate {
    /// Nom, chaîne vide pour le libellé par défaut.
    pub name: String,
    /// Onglet épinglé ?
    pub pinned: bool,
}

/// Nouvel ordre d'une liste : identifiants dans l'ordre d'affichage.
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct Order {
    /// Identifiants, du premier au dernier.
    pub ids: Vec<i64>,
}

/// Création d'un éditeur.
#[derive(Debug, Default, Deserialize, TS)]
#[ts(export)]
#[serde(default)]
pub struct NewEditor {
    /// Connexion de départ, absente pour aucune.
    #[ts(optional)]
    pub connection_id: Option<i64>,
}

/// Vérifie la longueur d'un nom.
fn check_name(name: &str) -> Result<(), EditorsError> {
    let length = name.chars().count();
    if length > NAME_MAX_LENGTH {
        return Err(EditorsError::Invalid(format!(
            "nom : {NAME_MAX_LENGTH} caractères au plus, {length} reçus"
        )));
    }
    Ok(())
}

/// Supprime les éditeurs fermés depuis plus de 24 heures, tous utilisateurs
/// confondus, avec leurs onglets de résultat. Renvoie leur nombre.
pub fn purge(db: &Connection) -> Result<usize, EditorsError> {
    Ok(db.execute(
        &format!(
            "DELETE FROM editors WHERE closed_at != ''
             AND closed_at < strftime('%Y-%m-%dT%H:%M:%fZ', 'now', '{REOPEN_DELAY}')"
        ),
        [],
    )?)
}

/// Éditeurs ouverts de l'utilisateur, dans l'ordre (sans leurs onglets de
/// résultat, lus éditeur par éditeur : `results_of`).
pub fn list_open(db: &Connection, user_id: i64) -> Result<Vec<Editor>, EditorsError> {
    purge(db)?;
    let mut statement = db.prepare(
        "SELECT id, name, connection_id, sql FROM editors
         WHERE user_id = ?1 AND closed_at = '' ORDER BY position, id",
    )?;
    let editors = statement
        .query_map(params![user_id], |row| {
            Ok(Editor {
                id: row.get(0)?,
                name: row.get(1)?,
                connection_id: row.get(2)?,
                sql: row.get(3)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(editors)
}

/// Onglets de résultat de l'éditeur ouvert `editor_id`, s'il est à
/// l'utilisateur ; `None` sinon.
pub fn results_of(
    db: &Connection,
    user_id: i64,
    editor_id: i64,
) -> Result<Option<Vec<ResultTab>>, EditorsError> {
    if connection_of(db, user_id, editor_id)?.is_none() {
        return Ok(None);
    }
    Ok(Some(result_tabs(db, editor_id)?))
}

/// Onglets de résultat d'un éditeur, dans l'ordre d'affichage.
fn result_tabs(db: &Connection, editor_id: i64) -> Result<Vec<ResultTab>, EditorsError> {
    let mut statement = db.prepare(
        "SELECT id, query, sql, name, pinned, result, rows FROM result_tabs
         WHERE editor_id = ?1 ORDER BY position, id",
    )?;
    let rows = statement.query_map(params![editor_id], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, u32>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, bool>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, String>(6)?,
        ))
    })?;
    let mut tabs = Vec::new();
    for row in rows {
        let (id, query, sql, name, pinned, result, rows) = row?;
        tabs.push(ResultTab {
            id,
            query,
            sql,
            name,
            pinned,
            result: serde_json::from_str(&result)?,
            rows: serde_json::from_str(&rows)?,
        });
    }
    Ok(tabs)
}

/// Connexion de l'éditeur ouvert `id`, s'il est à l'utilisateur : `None` si
/// l'éditeur n'existe pas, `Some(None)` s'il n'a pas de connexion.
pub fn connection_of(
    db: &Connection,
    user_id: i64,
    id: i64,
) -> Result<Option<Option<i64>>, EditorsError> {
    Ok(db
        .query_row(
            "SELECT connection_id FROM editors WHERE id = ?1 AND user_id = ?2 AND closed_at = ''",
            params![id, user_id],
            |row| row.get(0),
        )
        .optional()?)
}

/// Crée un éditeur vide à la suite des éditeurs ouverts et le renvoie.
pub fn create(db: &Connection, user_id: i64, new: &NewEditor) -> Result<Editor, EditorsError> {
    db.execute(
        "INSERT INTO editors (user_id, connection_id, position)
         SELECT ?1, ?2, COALESCE(MAX(position) + 1, 0) FROM editors
         WHERE user_id = ?1 AND closed_at = ''",
        params![user_id, new.connection_id],
    )?;
    Ok(Editor {
        id: db.last_insert_rowid(),
        name: String::new(),
        connection_id: new.connection_id,
        sql: String::new(),
    })
}

/// Modifie l'éditeur ouvert `id`. Renvoie `false` s'il n'existe pas ou
/// appartient à un autre utilisateur.
pub fn update(
    db: &Connection,
    user_id: i64,
    id: i64,
    changed: &EditorUpdate,
) -> Result<bool, EditorsError> {
    check_name(&changed.name)?;
    let updated = db.execute(
        &format!(
            "UPDATE editors SET name = ?3, sql = ?4, connection_id = ?5, updated_at = {NOW}
             WHERE id = ?1 AND user_id = ?2 AND closed_at = ''"
        ),
        params![
            id,
            user_id,
            changed.name,
            changed.sql,
            changed.connection_id
        ],
    )?;
    Ok(updated == 1)
}

/// Range les éditeurs ouverts de l'utilisateur dans l'ordre donné.
pub fn reorder(db: &Connection, user_id: i64, order: &Order) -> Result<(), EditorsError> {
    for (position, id) in order.ids.iter().enumerate() {
        db.execute(
            "UPDATE editors SET position = ?3 WHERE id = ?1 AND user_id = ?2",
            params![id, user_id, position as i64],
        )?;
    }
    Ok(())
}

/// Ferme l'éditeur `id` : il reste rouvrable 24 heures. Renvoie `false`
/// s'il n'existe pas, est déjà fermé ou appartient à un autre utilisateur.
pub fn close(db: &Connection, user_id: i64, id: i64) -> Result<bool, EditorsError> {
    let closed = db.execute(
        &format!(
            "UPDATE editors SET closed_at = {NOW}
             WHERE id = ?1 AND user_id = ?2 AND closed_at = ''"
        ),
        params![id, user_id],
    )?;
    Ok(closed == 1)
}

/// Éditeurs fermés depuis moins de 24 heures, du plus récent au plus ancien.
pub fn closed_recently(db: &Connection, user_id: i64) -> Result<Vec<ClosedEditor>, EditorsError> {
    purge(db)?;
    let mut statement = db.prepare(
        "SELECT id, name, substr(sql, 1, 200), closed_at FROM editors
         WHERE user_id = ?1 AND closed_at != '' ORDER BY closed_at DESC",
    )?;
    let editors = statement
        .query_map(params![user_id], |row| {
            Ok(ClosedEditor {
                id: row.get(0)?,
                name: row.get(1)?,
                sql: row.get(2)?,
                closed_at: row.get(3)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    Ok(editors)
}

/// Rouvre l'éditeur fermé `id`, à la suite des éditeurs ouverts. Renvoie
/// `false` s'il n'existe pas, est déjà ouvert ou appartient à un autre utilisateur.
pub fn reopen(db: &Connection, user_id: i64, id: i64) -> Result<bool, EditorsError> {
    let reopened = db.execute(
        "UPDATE editors SET closed_at = '',
             position = (SELECT COALESCE(MAX(position) + 1, 0) FROM editors
                         WHERE user_id = ?2 AND closed_at = '')
         WHERE id = ?1 AND user_id = ?2 AND closed_at != ''",
        params![id, user_id],
    )?;
    Ok(reopened == 1)
}

/// Renomme ou épingle l'onglet de résultat `id`, s'il appartient à un
/// éditeur de l'utilisateur. Renvoie `false` sinon.
pub fn update_result_tab(
    db: &Connection,
    user_id: i64,
    id: i64,
    changed: &ResultTabUpdate,
) -> Result<bool, EditorsError> {
    check_name(&changed.name)?;
    let updated = db.execute(
        "UPDATE result_tabs SET name = ?3, pinned = ?4
         WHERE id = ?1 AND editor_id IN (SELECT id FROM editors WHERE user_id = ?2)",
        params![id, user_id, changed.name, changed.pinned],
    )?;
    Ok(updated == 1)
}

/// Supprime l'onglet de résultat `id` et ses lignes, s'il appartient à un
/// éditeur de l'utilisateur. Renvoie `false` sinon.
pub fn delete_result_tab(db: &Connection, user_id: i64, id: i64) -> Result<bool, EditorsError> {
    let deleted = db.execute(
        "DELETE FROM result_tabs
         WHERE id = ?1 AND editor_id IN (SELECT id FROM editors WHERE user_id = ?2)",
        params![id, user_id],
    )?;
    Ok(deleted == 1)
}

/// Range les onglets de résultat de l'éditeur `editor_id` dans l'ordre donné.
pub fn reorder_result_tabs(
    db: &Connection,
    user_id: i64,
    editor_id: i64,
    order: &Order,
) -> Result<(), EditorsError> {
    for (position, id) in order.ids.iter().enumerate() {
        db.execute(
            "UPDATE result_tabs SET position = ?3
             WHERE id = ?1 AND editor_id = ?4
               AND editor_id IN (SELECT id FROM editors WHERE user_id = ?2)",
            params![id, user_id, position as i64, editor_id],
        )?;
    }
    Ok(())
}

/// Début d'un lot dans l'éditeur `editor_id` : les onglets non épinglés
/// disparaissent, un onglet par requête est créé à la suite des onglets
/// épinglés. Renvoie les identifiants des nouveaux onglets, dans l'ordre des requêtes.
pub fn start_batch(
    db: &Connection,
    editor_id: i64,
    queries: &[(u32, String)],
) -> Result<Vec<i64>, EditorsError> {
    db.execute(
        "DELETE FROM result_tabs WHERE editor_id = ?1 AND pinned = 0",
        params![editor_id],
    )?;
    let first: i64 = db.query_row(
        "SELECT COALESCE(MAX(position) + 1, 0) FROM result_tabs WHERE editor_id = ?1",
        params![editor_id],
        |row| row.get(0),
    )?;
    let mut ids = Vec::with_capacity(queries.len());
    for (offset, (query, sql)) in queries.iter().enumerate() {
        db.execute(
            "INSERT INTO result_tabs (editor_id, position, query, sql) VALUES (?1, ?2, ?3, ?4)",
            params![editor_id, first + offset as i64, query, sql],
        )?;
        ids.push(db.last_insert_rowid());
    }
    Ok(ids)
}

/// Enregistre l'état et les lignes d'un onglet de résultat. Sans effet si
/// l'onglet a été supprimé entre-temps (fermé par l'utilisateur).
pub fn save_result(
    db: &Connection,
    tab_id: i64,
    result: &SavedResult,
    rows: &[Vec<Cell>],
) -> Result<(), EditorsError> {
    db.execute(
        "UPDATE result_tabs SET result = ?2, rows = ?3 WHERE id = ?1",
        params![
            tab_id,
            serde_json::to_string(result)?,
            serde_json::to_string(rows)?
        ],
    )?;
    Ok(())
}

/// Routes des éditeurs.
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/editors", get(list_editors).post(create_editor))
        .route("/api/editors/order", put(reorder_editors))
        .route("/api/editors/closed", get(list_closed_editors))
        .route("/api/editors/{id}", put(update_editor).delete(close_editor))
        .route("/api/editors/{id}/reopen", post(reopen_editor))
        .route("/api/editors/{id}/results", get(list_results))
        .route("/api/editors/{id}/results/order", put(reorder_results))
        .route(
            "/api/results/{id}",
            put(update_result).delete(delete_result),
        )
}

/// `GET /api/editors` : éditeurs ouverts, dans l'ordre.
async fn list_editors(
    State(state): State<AppState>,
    user: CurrentUser,
) -> Result<Json<Vec<Editor>>, ApiError> {
    let editors = state.with_db(move |db, _| list_open(db, user.id)).await?;
    Ok(Json(editors))
}

/// `POST /api/editors` : crée un éditeur vide.
async fn create_editor(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(new): Json<NewEditor>,
) -> Result<(StatusCode, Json<Editor>), ApiError> {
    let editor = state
        .with_db(move |db, _| create(db, user.id, &new))
        .await?;
    Ok((StatusCode::CREATED, Json(editor)))
}

/// Réponse vide (204) ou 404 selon que l'élément visé a été trouvé.
fn no_content_or_not_found(found: bool) -> Result<StatusCode, ApiError> {
    if found {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound)
    }
}

/// `PUT /api/editors/{id}` : change le nom, le texte ou la connexion.
async fn update_editor(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<i64>,
    Json(changed): Json<EditorUpdate>,
) -> Result<StatusCode, ApiError> {
    let found = state
        .with_db(move |db, _| update(db, user.id, id, &changed))
        .await?;
    no_content_or_not_found(found)
}

/// `DELETE /api/editors/{id}` : ferme l'éditeur.
async fn close_editor(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    let found = state.with_db(move |db, _| close(db, user.id, id)).await?;
    if found {
        // Un éditeur fermé n'a plus de session : sa connexion se ferme.
        state.sessions.stop_editor(id);
    }
    no_content_or_not_found(found)
}

/// `PUT /api/editors/order` : range les éditeurs ouverts.
async fn reorder_editors(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(order): Json<Order>,
) -> Result<StatusCode, ApiError> {
    state
        .with_db(move |db, _| reorder(db, user.id, &order))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /api/editors/closed` : éditeurs fermés depuis moins de 24 heures.
async fn list_closed_editors(
    State(state): State<AppState>,
    user: CurrentUser,
) -> Result<Json<Vec<ClosedEditor>>, ApiError> {
    let editors = state
        .with_db(move |db, _| closed_recently(db, user.id))
        .await?;
    Ok(Json(editors))
}

/// `POST /api/editors/{id}/reopen` : rouvre un éditeur fermé.
async fn reopen_editor(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    let found = state.with_db(move |db, _| reopen(db, user.id, id)).await?;
    no_content_or_not_found(found)
}

/// `GET /api/editors/{id}/results` : onglets de résultat de l'éditeur.
async fn list_results(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<i64>,
) -> Result<Json<Vec<ResultTab>>, ApiError> {
    let results = state
        .with_db(move |db, _| results_of(db, user.id, id))
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(results))
}

/// `PUT /api/editors/{id}/results/order` : range les onglets de résultat.
async fn reorder_results(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<i64>,
    Json(order): Json<Order>,
) -> Result<StatusCode, ApiError> {
    state
        .with_db(move |db, _| reorder_result_tabs(db, user.id, id, &order))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `PUT /api/results/{id}` : renomme ou épingle un onglet de résultat.
async fn update_result(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<i64>,
    Json(changed): Json<ResultTabUpdate>,
) -> Result<StatusCode, ApiError> {
    let found = state
        .with_db(move |db, _| update_result_tab(db, user.id, id, &changed))
        .await?;
    no_content_or_not_found(found)
}

/// `DELETE /api/results/{id}` : supprime un onglet de résultat.
async fn delete_result(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    let found = state
        .with_db(move |db, _| delete_result_tab(db, user.id, id))
        .await?;
    no_content_or_not_found(found)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store;

    /// Base de test avec un utilisateur.
    fn setup() -> (Connection, i64) {
        let db = store::open_in_memory();
        let user_id = store::ensure_user(&db, "test@example.com").unwrap();
        (db, user_id)
    }

    #[test]
    fn creates_updates_orders_and_lists_editors() {
        let (db, user_id) = setup();
        let first = create(&db, user_id, &NewEditor::default()).unwrap();
        let second = create(&db, user_id, &NewEditor::default()).unwrap();
        let changed = EditorUpdate {
            name: "Ventes".to_string(),
            sql: "SELECT 1".to_string(),
            connection_id: None,
        };
        assert!(update(&db, user_id, first.id, &changed).unwrap());
        reorder(
            &db,
            user_id,
            &Order {
                ids: vec![second.id, first.id],
            },
        )
        .unwrap();
        let editors = list_open(&db, user_id).unwrap();
        assert_eq!(
            editors.iter().map(|e| e.id).collect::<Vec<_>>(),
            [second.id, first.id]
        );
        assert_eq!(
            (editors[1].name.as_str(), editors[1].sql.as_str()),
            ("Ventes", "SELECT 1")
        );
        // Un autre utilisateur ne voit ni ne modifie rien.
        let other = store::ensure_user(&db, "autre@example.com").unwrap();
        assert!(list_open(&db, other).unwrap().is_empty());
        assert!(!update(&db, other, first.id, &changed).unwrap());
    }

    #[test]
    fn closes_reopens_and_purges_after_a_day() {
        let (db, user_id) = setup();
        let editor = create(&db, user_id, &NewEditor::default()).unwrap();
        assert!(close(&db, user_id, editor.id).unwrap());
        assert!(list_open(&db, user_id).unwrap().is_empty());
        assert_eq!(closed_recently(&db, user_id).unwrap().len(), 1);
        assert!(reopen(&db, user_id, editor.id).unwrap());
        assert_eq!(list_open(&db, user_id).unwrap().len(), 1);

        // Fermé depuis plus de 24 heures : purgé.
        assert!(close(&db, user_id, editor.id).unwrap());
        db.execute(
            "UPDATE editors SET closed_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now', '-25 hours')",
            [],
        )
        .unwrap();
        assert!(closed_recently(&db, user_id).unwrap().is_empty());
        assert!(!reopen(&db, user_id, editor.id).unwrap());
    }

    #[test]
    fn keeps_pinned_result_tabs_when_a_batch_starts() {
        let (db, user_id) = setup();
        let editor = create(&db, user_id, &NewEditor::default()).unwrap();
        let first = start_batch(
            &db,
            editor.id,
            &[(0, "SELECT 1".into()), (1, "SELECT 2".into())],
        )
        .unwrap();
        let result = SavedResult {
            started: true,
            done: true,
            row_count: 1,
            ..SavedResult::default()
        };
        save_result(&db, first[0], &result, &[vec![Some("1".to_string())]]).unwrap();
        let pin = ResultTabUpdate {
            name: "Gardé".to_string(),
            pinned: true,
        };
        assert!(update_result_tab(&db, user_id, first[0], &pin).unwrap());

        let second = start_batch(&db, editor.id, &[(0, "SELECT 3".into())]).unwrap();
        let tabs = results_of(&db, user_id, editor.id).unwrap().unwrap();
        assert_eq!(
            tabs.iter().map(|t| t.id).collect::<Vec<_>>(),
            [first[0], second[0]]
        );
        assert_eq!(tabs[0].rows, [vec![Some("1".to_string())]]);
        assert!(tabs[0].result.done && tabs[0].pinned);

        assert!(delete_result_tab(&db, user_id, second[0]).unwrap());
        assert_eq!(
            results_of(&db, user_id, editor.id).unwrap().unwrap().len(),
            1
        );
        assert_eq!(
            results_of(&db, user_id, editor.id).unwrap().unwrap().len(),
            1
        );
        let other = store::ensure_user(&db, "autre@example.com").unwrap();
        assert!(results_of(&db, other, editor.id).unwrap().is_none());
    }
}
