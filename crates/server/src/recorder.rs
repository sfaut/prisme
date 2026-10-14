//! Enregistreur des résultats d'un éditeur, branché sur le relais des
//! messages de sa session de requêtes (`session_relay.rs`).
//!
//! Chaque message envoyé au client passe d'abord par l'enregistreur :
//!
//! - `batch` : les onglets non épinglés de l'éditeur disparaissent, un onglet
//!   par requête est créé ; son identifiant part au client avec la requête ;
//! - `started`, `columns`, `rows` : l'état et les lignes de chaque requête
//!   s'accumulent en mémoire (les lignes sont bornées par la limite de lignes) ;
//! - `done`, `error` : le résultat complet est écrit dans la base interne.
//!
//! Le navigateur ne renvoie donc jamais les résultats : après un
//! rafraîchissement, il les relit tels que le serveur les a reçus. À la fin
//! de la session, une requête inachevée est enregistrée en échec.
//!
//! Le dernier lot reste en mémoire jusqu'au suivant, requêtes terminées
//! comprises : un WebSocket qui se raccroche à la session le reçoit tel
//! quel (`snapshot`), même si une requête s'est terminée entre la lecture
//! des onglets enregistrés par la page et son raccrochage.

use std::collections::HashMap;
use std::time::Instant;

use prisme_connectors::mysql::Cell;
use serde::Serialize;
use tokio::sync::Mutex;
use ts_rs::TS;

use crate::api::AppState;
use crate::editors::{self, SavedColumns, SavedResult};
use crate::query_session::ServerMessage;

/// Message d'une requête que la fin de la session a interrompue.
const SESSION_CLOSED: &str = "session fermée avant la fin de la requête";

/// Requête du dernier lot, telle qu'envoyée à un WebSocket qui se raccroche.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub(crate) struct ResumedQuery {
    /// Numéro de la requête dans le lot, repris par ses messages suivants.
    query: u32,
    /// Onglet de résultat enregistré de la requête.
    tab: i64,
    /// Texte de la requête, sans séparateur.
    sql: String,
    /// État du résultat.
    result: SavedResult,
    /// Lignes reçues jusqu'ici.
    rows: Vec<Vec<Cell>>,
    /// Temps écoulé depuis le départ de la requête, en microsecondes ;
    /// absent tant qu'elle attend son tour.
    started_us_ago: Option<u64>,
    /// Temps écoulé depuis la réception de ses colonnes, en microsecondes ;
    /// absent avant.
    columns_us_ago: Option<u64>,
}

/// Enregistreur des résultats d'un éditeur.
pub struct Recorder {
    /// État de l'application, pour la base interne.
    state: AppState,
    /// Éditeur dont les résultats sont enregistrés.
    editor_id: i64,
    /// Requêtes du dernier lot, par numéro.
    records: Mutex<HashMap<u32, Record>>,
}

/// Résultat d'une requête en cours d'enregistrement.
struct Record {
    /// Onglet de résultat de la requête.
    tab_id: i64,
    /// Texte de la requête.
    sql: String,
    /// État du résultat.
    result: SavedResult,
    /// Lignes reçues.
    rows: Vec<Vec<Cell>>,
    /// Instant du départ de la requête.
    started_at: Option<Instant>,
    /// Instant de réception des colonnes, pour le temps de transfert.
    columns_at: Option<Instant>,
}

impl Recorder {
    /// Crée l'enregistreur des résultats de l'éditeur `editor_id`.
    pub fn new(state: AppState, editor_id: i64) -> Self {
        Self {
            state,
            editor_id,
            records: Mutex::new(HashMap::new()),
        }
    }

    /// Prend note d'un message avant son envoi au client. Pour `batch`,
    /// complète chaque requête de l'identifiant de son onglet.
    pub async fn observe(&self, message: &mut ServerMessage) {
        let mut records = self.records.lock().await;
        let bytes = serde_json::to_vec(message).map_or(0, |json| json.len() as u64);
        match message {
            ServerMessage::Batch { queries } => {
                let editor_id = self.editor_id;
                let wanted: Vec<_> = queries.iter().map(|q| (q.query, q.sql.clone())).collect();
                let created = self
                    .state
                    .with_db(move |db, _| editors::start_batch(db, editor_id, &wanted))
                    .await;
                records.clear();
                match created {
                    Ok(ids) => {
                        for (query, tab_id) in queries.iter_mut().zip(ids) {
                            query.tab = tab_id;
                            records.insert(query.query, Record::new(tab_id, &query.sql));
                        }
                    }
                    Err(_) => tracing::warn!("onglets de résultat non créés, éditeur {editor_id}"),
                }
            }
            ServerMessage::Started { query } => {
                if let Some(record) = records.get_mut(query) {
                    record.result.started = true;
                    record.started_at = Some(Instant::now());
                }
            }
            ServerMessage::Columns {
                query,
                columns,
                right_aligned,
                sql,
                limit,
                expected_rows,
                execution_us,
            } => {
                if let Some(record) = records.get_mut(query) {
                    record.result.columns = Some(SavedColumns {
                        columns: columns.clone(),
                        right_aligned: right_aligned.clone(),
                        sql: sql.clone(),
                        limit: limit.clone(),
                        expected_rows: *expected_rows,
                        execution_us: *execution_us,
                    });
                    record.result.bytes += bytes;
                    record.columns_at = Some(Instant::now());
                }
            }
            ServerMessage::Rows { query, rows } => {
                if let Some(record) = records.get_mut(query) {
                    record.rows.extend(rows.iter().cloned());
                    record.result.row_count = record.rows.len();
                    record.result.bytes += bytes;
                }
            }
            ServerMessage::Done {
                query,
                truncated,
                affected_rows,
                ..
            } => {
                if let Some(record) = records.get_mut(query) {
                    record.result.done = true;
                    record.result.truncated = *truncated;
                    record.result.affected_rows = *affected_rows;
                    record.result.bytes += bytes;
                    record.result.transfer_us = record
                        .columns_at
                        .map_or(0, |at| at.elapsed().as_micros() as u64);
                    self.save(record).await;
                }
            }
            ServerMessage::Error {
                query: Some(query),
                message,
            } => {
                if let Some(record) = records.get_mut(query) {
                    record.result.done = true;
                    record.result.error = Some(message.clone());
                    self.save(record).await;
                }
            }
            _ => {}
        }
    }

    /// Dernier lot, requête par requête, où il en est.
    pub(crate) async fn snapshot(&self) -> Vec<ResumedQuery> {
        let records = self.records.lock().await;
        let mut queries: Vec<ResumedQuery> = records
            .iter()
            .map(|(&query, record)| ResumedQuery {
                query,
                tab: record.tab_id,
                sql: record.sql.clone(),
                result: record.result.clone(),
                rows: record.rows.clone(),
                started_us_ago: record.started_at.map(micros_since),
                columns_us_ago: record.columns_at.map(micros_since),
            })
            .collect();
        queries.sort_by_key(|query| query.query);
        queries
    }

    /// Fin de la session : les requêtes inachevées sont enregistrées en échec.
    pub async fn finish(&self) {
        let mut records = self.records.lock().await;
        for record in records.values_mut().filter(|record| !record.result.done) {
            record.result.done = true;
            record.result.error = Some(SESSION_CLOSED.to_string());
            self.save(record).await;
        }
        records.clear();
    }

    /// Écrit le résultat d'une requête dans la base interne ; un échec est
    /// inscrit au journal, sans interrompre la session.
    async fn save(&self, record: &Record) {
        let tab_id = record.tab_id;
        let result = record.result.clone();
        let rows = record.rows.clone();
        let saved = self
            .state
            .with_db(move |db, _| editors::save_result(db, tab_id, &result, &rows))
            .await;
        if saved.is_err() {
            tracing::warn!("résultat non enregistré, onglet {tab_id}");
        }
    }
}

impl Record {
    /// Résultat vide de l'onglet `tab_id`, pour la requête `sql`.
    fn new(tab_id: i64, sql: &str) -> Self {
        Self {
            tab_id,
            sql: sql.to_string(),
            result: SavedResult::default(),
            rows: Vec::new(),
            started_at: None,
            columns_at: None,
        }
    }
}

/// Temps écoulé depuis `instant`, en microsecondes.
fn micros_since(instant: Instant) -> u64 {
    u64::try_from(instant.elapsed().as_micros()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editors::NewEditor;
    use crate::master_key::MasterKey;
    use crate::query_session::{BatchQuery, LimitReport};
    use crate::store;

    #[tokio::test]
    async fn records_each_result_and_closes_unfinished_ones() {
        let db = store::open_in_memory();
        let user_id = store::ensure_user(&db, "test@example.com").unwrap();
        let editor = editors::create(&db, user_id, &NewEditor::default()).unwrap();
        let state = AppState::new(db, MasterKey::generate(), user_id);
        let recorder = Recorder::new(state.clone(), editor.id);

        let mut batch = ServerMessage::Batch {
            queries: (0..2)
                .map(|query| BatchQuery {
                    query,
                    tab: 0,
                    sql: format!("SELECT {query}"),
                    start: 0,
                    end: 8,
                })
                .collect(),
        };
        recorder.observe(&mut batch).await;
        let ServerMessage::Batch { queries } = &batch else {
            unreachable!()
        };
        assert!(queries.iter().all(|query| query.tab > 0));

        for mut message in [
            ServerMessage::Started { query: 0 },
            ServerMessage::Columns {
                query: 0,
                columns: vec!["x".to_string()],
                right_aligned: vec![true],
                sql: "SELECT 0".to_string(),
                limit: LimitReport::NotApplicable,
                expected_rows: None,
                execution_us: 120,
            },
            ServerMessage::Rows {
                query: 0,
                rows: vec![vec![Some("0".to_string())]],
            },
            ServerMessage::Done {
                query: 0,
                row_count: 1,
                truncated: false,
                affected_rows: 0,
                elapsed_ms: 1,
                database: None,
            },
            ServerMessage::Started { query: 1 },
        ] {
            recorder.observe(&mut message).await;
        }
        let snapshot = recorder.snapshot().await;
        assert_eq!(snapshot.len(), 2);
        assert!(snapshot[0].result.done && snapshot[0].rows.len() == 1);
        assert!(!snapshot[1].result.done && snapshot[1].started_us_ago.is_some());
        assert!(snapshot[1].columns_us_ago.is_none());
        recorder.finish().await;
        assert!(recorder.snapshot().await.is_empty());

        let editor_id = editor.id;
        let tabs = state
            .with_db(move |db, _| editors::results_of(db, user_id, editor_id))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(tabs[0].rows, [vec![Some("0".to_string())]]);
        assert!(tabs[0].result.done && tabs[0].result.error.is_none());
        assert_eq!(tabs[0].result.columns.as_ref().unwrap().execution_us, 120);
        assert!(tabs[0].result.bytes > 0);
        assert_eq!(tabs[1].result.error.as_deref(), Some(SESSION_CLOSED));
    }
}
