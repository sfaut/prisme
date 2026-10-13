//! Session de requêtes sur une connexion enregistrée, par WebSocket.
//!
//! Une session garde une connexion ouverte au serveur cible pendant toute la
//! durée du WebSocket (les transactions en auront besoin). Le client envoie
//! le texte complet de l'éditeur ; la session le découpe en requêtes (crate
//! `splitter`) et exécute :
//!
//! - avec un curseur (Ctrl-Entrée) : la requête sous le curseur, sur la
//!   connexion de la session ;
//! - sans curseur (Ctrl-Maj-Entrée) : toutes les requêtes en parallèle, chacune
//!   sur sa propre connexion, ouverte sur la base en cours de la session.
//!   Une requête qui change l'état de sa connexion (`USE`, `SET`) n'a donc
//!   aucun effet sur les autres, ni sur la session.
//!
//! Les résultats arrivent en streaming, par lots de lignes, sans jamais être
//! chargés en entier en mémoire. Chaque message d'un résultat porte le numéro
//! de sa requête (`query`) : les messages de requêtes parallèles s'entremêlent.
//!
//! Protocole, en JSON texte :
//!
//! ```text
//! serveur → client  {"type": "ready", "server": {"flavor": "mariadb", "version": "11.3.2-MariaDB"}, "database": "xlist"}
//! client → serveur  {"type": "run", "sql": "SELECT 1; SELECT 2", "cursor": 3, "row_limit": 2000}   cursor et row_limit facultatifs
//! client → serveur  {"type": "cancel"}                                  arrête toutes les requêtes en cours
//! serveur → client  {"type": "batch", "queries": [{"query": 0, "sql": "SELECT 1", "start": 0, "end": 9}]}
//! serveur → client  {"type": "started", "query": 0}                  la requête part (en parallèle, elle peut attendre son tour)
//! serveur → client  {"type": "columns", "query": 0, "columns": ["1"], "sql": "SELECT 1 LIMIT 1001", "limit": "added"}
//! serveur → client  {"type": "rows", "query": 0, "rows": [["1"]]}       autant de lots que nécessaire
//! serveur → client  {"type": "done", "query": 0, "row_count": 1, "truncated": false, "affected_rows": 0, "elapsed_ms": 2, "database": "xlist"}
//! serveur → client  {"type": "error", "query": 0, "message": "..."}     à la place de la suite, en cas d'échec
//! serveur → client  {"type": "batch_done"}                               toutes les requêtes sont terminées
//! ```
//!
//! `cancel` interrompt les requêtes en cours (`KILL QUERY` sur chacune de
//! leurs connexions) et empêche le départ de celles qui attendent leur tour :
//! chacune se termine par une erreur « requête interrompue ». La session lit
//! les messages du client pendant l'exécution, pour recevoir `cancel` à temps.
//!
//! Une erreur qui ne concerne aucune requête (message invalide, connexion au
//! serveur cible impossible) a `"query": null`. `database` est la base en
//! cours de la session (`SELECT DATABASE()`, `null` si aucune) : celle de la
//! connexion enregistrée au départ, puis celle choisie par `USE`.
//!
//! La limite de lignes suit `docs/V1.md`, section 2.3 : `row_limit` de la
//! demande, sinon le réglage de l'utilisateur. En attendant les interrupteurs
//! d'écriture, les écritures et les requêtes non analysables sont refusées
//! (`docs/V1.md`, section 2.4).

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use axum::extract::ws::{Message, WebSocket};
use futures_util::future::join_all;
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use prisme_connectors::mysql::{self, Cell, Connection, Flavor, MysqlTarget, ServerInfo};
use prisme_dialects::Dialect;
use prisme_dialects::classify::{StatementKind, classify};
use prisme_dialects::limit::{LimitOutcome, apply_default_limit};
use prisme_splitter::{Statement, split, statement_at};
use serde::{Deserialize, Serialize};
use tokio::sync::{Semaphore, mpsc};
use ts_rs::TS;

use crate::api::AppState;
use crate::settings::{self, check_row_limit};

/// Nombre de lignes par message `rows`.
const ROW_BATCH_SIZE: usize = 500;

/// Nombre maximal de requêtes exécutées en même temps (une connexion chacune).
const MAX_PARALLEL: usize = 8;

/// Nombre de messages en attente d'envoi au-delà duquel la lecture des résultats patiente.
const OUTBOX_SIZE: usize = 64;

/// Message envoyé par le client.
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ClientMessage {
    /// Exécuter des requêtes.
    Run(QueryRequest),
    /// Arrêter toutes les requêtes en cours.
    Cancel,
}

/// Message d'erreur des requêtes arrêtées par l'utilisateur.
const CANCELLED: &str = "requête interrompue par l'utilisateur";

/// Demande d'exécution envoyée par le client.
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
struct QueryRequest {
    /// Texte complet de l'éditeur, une ou plusieurs requêtes.
    sql: String,
    /// Position du curseur en unités UTF-16 : seule la requête sous le
    /// curseur s'exécute. Absent : toutes les requêtes, en parallèle.
    #[ts(optional)]
    cursor: Option<usize>,
    /// Limite de lignes pour ces requêtes seules, parmi les choix permis.
    #[ts(optional)]
    row_limit: Option<u32>,
}

/// Requête d'un lot, telle qu'annoncée au client.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
struct BatchQuery {
    /// Numéro de la requête dans le lot, repris par ses messages.
    query: u32,
    /// Texte de la requête, sans séparateur.
    sql: String,
    /// Début de la requête dans le texte de l'éditeur, en unités UTF-16.
    start: usize,
    /// Fin de la requête dans le texte de l'éditeur, en unités UTF-16.
    end: usize,
}

/// Traitement de la limite, tel qu'annoncé au client.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
enum LimitReport {
    /// Prisme a ajouté `LIMIT N + 1`.
    Added,
    /// La limite écrite par l'utilisateur s'applique.
    UserLimit,
    /// Requête sans lignes à limiter.
    NotApplicable,
    /// Requête non analysée : la lecture s'arrête d'elle-même après N lignes.
    Unparsed,
}

/// Message envoyé au client.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ServerMessage {
    /// La connexion au serveur cible est ouverte.
    Ready {
        server: ServerInfo,
        database: Option<String>,
    },
    /// Requêtes qui vont s'exécuter.
    Batch { queries: Vec<BatchQuery> },
    /// Une requête du lot démarre. En parallèle, les requêtes au-delà de
    /// `MAX_PARALLEL` attendent leur tour avant ce message.
    Started { query: u32 },
    /// Début d'un résultat : colonnes, texte réellement exécuté, traitement de la limite.
    Columns {
        query: u32,
        columns: Vec<String>,
        /// Pour chaque colonne, ses valeurs s'alignent-elles à droite (nombres,
        /// dates et heures) ?
        right_aligned: Vec<bool>,
        sql: String,
        limit: LimitReport,
    },
    /// Lot de lignes.
    Rows { query: u32, rows: Vec<Vec<Cell>> },
    /// Fin d'un résultat.
    Done {
        query: u32,
        row_count: usize,
        truncated: bool,
        affected_rows: u64,
        elapsed_ms: u128,
        /// Base en cours de la session après la requête (elle change avec `USE`).
        database: Option<String>,
    },
    /// Échec d'une requête, ou de la session si `query` est absent.
    Error { query: Option<u32>, message: String },
    /// Toutes les requêtes du lot sont terminées.
    BatchDone,
}

/// Le client a fermé le WebSocket : la session s'arrête.
struct SocketClosed;

/// File des messages à envoyer au client, partagée par les requêtes parallèles.
///
/// Une tâche dédiée vide la file vers le WebSocket. La file est bornée : si
/// le client lit lentement, la lecture des résultats patiente.
#[derive(Clone)]
struct Outbox(mpsc::Sender<ServerMessage>);

impl Outbox {
    /// Met un message dans la file.
    async fn send(&self, message: ServerMessage) -> Result<(), SocketClosed> {
        self.0.send(message).await.map_err(|_| SocketClosed)
    }

    /// Met un message d'erreur dans la file.
    async fn error(&self, query: Option<u32>, message: String) -> Result<(), SocketClosed> {
        self.send(ServerMessage::Error { query, message }).await
    }
}

/// Arrêt des requêtes en cours, demandé par le client.
///
/// Garde la liste des connexions qui exécutent une requête, pour pouvoir les
/// interrompre (`KILL QUERY`), et un drapeau que les requêtes consultent avant
/// de partir et pendant la lecture de leurs lignes.
#[derive(Default)]
struct Cancellation {
    /// L'arrêt a-t-il été demandé pour le lot en cours ?
    cancelled: AtomicBool,
    /// Identifiants (`Conn::id()`) des connexions qui exécutent une requête.
    running: Mutex<Vec<u32>>,
}

impl Cancellation {
    /// Prépare un nouveau lot : plus aucun arrêt demandé.
    fn reset(&self) {
        self.cancelled.store(false, Ordering::SeqCst);
    }

    /// L'arrêt a-t-il été demandé ?
    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    /// Demande l'arrêt et interrompt toutes les requêtes en cours.
    async fn cancel(&self, target: &MysqlTarget) {
        self.cancelled.store(true, Ordering::SeqCst);
        let running = self
            .running
            .lock()
            .map(|ids| ids.clone())
            .unwrap_or_default();
        for connection_id in running {
            if let Err(error) = mysql::kill_query(target, connection_id).await {
                tracing::warn!("interruption de la requête impossible : {error}");
            }
        }
    }

    /// Note qu'une connexion exécute une requête, jusqu'à la disparition du garde renvoyé.
    fn track(&self, connection_id: u32) -> RunningGuard<'_> {
        if let Ok(mut ids) = self.running.lock() {
            ids.push(connection_id);
        }
        RunningGuard {
            cancellation: self,
            connection_id,
        }
    }
}

/// Garde d'une requête en cours : à sa disparition, la connexion sort de la liste.
struct RunningGuard<'a> {
    cancellation: &'a Cancellation,
    connection_id: u32,
}

impl Drop for RunningGuard<'_> {
    /// Retire la connexion de la liste des requêtes en cours.
    fn drop(&mut self) {
        if let Ok(mut ids) = self.cancellation.running.lock() {
            ids.retain(|&id| id != self.connection_id);
        }
    }
}

/// Lit les messages du client pendant toute la session.
///
/// Un arrêt (`cancel`) est traité aussitôt, même pendant l'exécution d'un
/// lot ; une demande d'exécution (`run`) est transmise à la session.
async fn read_messages(
    mut incoming: SplitStream<WebSocket>,
    requests: mpsc::Sender<QueryRequest>,
    outbox: Outbox,
    cancellation: Arc<Cancellation>,
    target: MysqlTarget,
) {
    while let Some(Ok(message)) = incoming.next().await {
        let Message::Text(text) = message else {
            // Ping, pong et binaire sont ignorés ; Close termine la boucle au tour suivant.
            continue;
        };
        match serde_json::from_str::<ClientMessage>(text.as_str()) {
            Ok(ClientMessage::Run(request)) => {
                if requests.send(request).await.is_err() {
                    break;
                }
            }
            Ok(ClientMessage::Cancel) => cancellation.cancel(&target).await,
            Err(error) => {
                let message = format!("message invalide : {error}");
                if outbox.error(None, message).await.is_err() {
                    break;
                }
            }
        }
    }
}

/// Envoie au client les messages de la file, jusqu'à ce qu'elle se ferme.
async fn write_messages(
    mut sink: SplitSink<WebSocket, Message>,
    mut messages: mpsc::Receiver<ServerMessage>,
) {
    while let Some(message) = messages.recv().await {
        let json = serde_json::to_string(&message).expect("message sérialisable");
        if sink.send(Message::Text(json.into())).await.is_err() {
            break;
        }
    }
}

/// Déroule une session : connexion au serveur cible, puis demandes du client
/// jusqu'à la fermeture du WebSocket.
pub async fn run(socket: WebSocket, state: AppState, user_id: i64, target: MysqlTarget) {
    let (sink, incoming) = socket.split();
    let (sender, receiver) = mpsc::channel(OUTBOX_SIZE);
    let writer = tokio::spawn(write_messages(sink, receiver));
    let outbox = Outbox(sender);
    let cancellation = Arc::new(Cancellation::default());
    let (request_sender, mut requests) = mpsc::channel(1);
    let reader = tokio::spawn(read_messages(
        incoming,
        request_sender,
        outbox.clone(),
        Arc::clone(&cancellation),
        target.clone(),
    ));

    let session = Session {
        outbox: &outbox,
        target: &target,
        state: &state,
        user_id,
        cancellation: &cancellation,
    };
    if let Some(mut connection) = session.open().await {
        if let Ok(dialect) = session.ready(&mut connection).await {
            while let Some(request) = requests.recv().await {
                cancellation.reset();
                if session
                    .handle(&mut connection, dialect, request)
                    .await
                    .is_err()
                {
                    break;
                }
            }
        }
        let _ = connection.disconnect().await;
    }

    reader.abort();
    drop(outbox);
    let _ = writer.await;
}

/// Ce dont les requêtes ont besoin pendant leur exécution.
struct Session<'a> {
    /// File des messages vers le client.
    outbox: &'a Outbox,
    /// Paramètres de connexion, pour les connexions supplémentaires et les interruptions.
    target: &'a MysqlTarget,
    /// État de l'application, pour lire les réglages.
    state: &'a AppState,
    /// Utilisateur de la session.
    user_id: i64,
    /// Arrêt des requêtes en cours, demandé par le client.
    cancellation: &'a Cancellation,
}

/// Résumé d'un résultat lu jusqu'au bout.
struct Summary {
    row_count: usize,
    truncated: bool,
    affected_rows: u64,
    elapsed_ms: u128,
}

impl Session<'_> {
    /// Ouvre la connexion de la session ; en cas d'échec, l'erreur part au client.
    async fn open(&self) -> Option<Connection> {
        match mysql::connect(self.target).await {
            Ok(connection) => Some(connection),
            Err(error) => {
                let _ = self.outbox.error(None, error.to_string()).await;
                None
            }
        }
    }

    /// Annonce la session au client : identité du serveur et base en cours.
    /// Renvoie le dialecte du serveur.
    async fn ready(&self, connection: &mut Connection) -> Result<Dialect, SocketClosed> {
        let info = match mysql::server_info(connection).await {
            Ok(info) => info,
            Err(error) => {
                self.outbox.error(None, error.to_string()).await?;
                return Err(SocketClosed);
            }
        };
        let database = match mysql::current_database(connection).await {
            Ok(database) => database,
            Err(error) => {
                self.outbox.error(None, error.to_string()).await?;
                return Err(SocketClosed);
            }
        };
        let dialect = match info.flavor {
            Flavor::MySql => Dialect::MySql,
            Flavor::MariaDb => Dialect::MariaDb,
        };
        self.outbox
            .send(ServerMessage::Ready {
                server: info,
                database,
            })
            .await?;
        Ok(dialect)
    }

    /// Traite une demande du client : découpe le texte, puis exécute la
    /// requête sous le curseur, ou toutes les requêtes en parallèle.
    async fn handle(
        &self,
        connection: &mut Connection,
        dialect: Dialect,
        request: QueryRequest,
    ) -> Result<(), SocketClosed> {
        let max_rows = match row_limit(self.state, self.user_id, request.row_limit).await {
            Ok(max_rows) => max_rows as usize,
            Err(message) => return self.outbox.error(None, message).await,
        };
        let statements = split(&request.sql);
        let chosen = chosen_statements(&statements, request.cursor);
        if chosen.is_empty() {
            let message = "aucune requête à exécuter".to_string();
            return self.outbox.error(None, message).await;
        }

        let queries = chosen
            .iter()
            .enumerate()
            .map(|(index, statement)| BatchQuery {
                query: index as u32,
                sql: statement.sql.clone(),
                start: statement.start,
                end: statement.end,
            })
            .collect();
        self.outbox.send(ServerMessage::Batch { queries }).await?;

        if request.cursor.is_some() {
            self.run_on_session(connection, dialect, &chosen[0].sql, max_rows)
                .await?;
        } else {
            let database = match mysql::current_database(connection).await {
                Ok(database) => database,
                Err(error) => return self.outbox.error(None, error.to_string()).await,
            };
            self.run_in_parallel(&chosen, dialect, max_rows, &database)
                .await?;
        }
        self.outbox.send(ServerMessage::BatchDone).await
    }

    /// Exécute une requête sur la connexion de la session.
    async fn run_on_session(
        &self,
        connection: &mut Connection,
        dialect: Dialect,
        sql: &str,
        max_rows: usize,
    ) -> Result<(), SocketClosed> {
        self.outbox
            .send(ServerMessage::Started { query: 0 })
            .await?;
        let Some(summary) = self
            .run_query(connection, dialect, 0, sql, max_rows)
            .await?
        else {
            return Ok(());
        };
        let database = match mysql::current_database(connection).await {
            Ok(database) => database,
            Err(error) => return self.outbox.error(Some(0), error.to_string()).await,
        };
        self.done(0, summary, database).await
    }

    /// Exécute toutes les requêtes en parallèle, `MAX_PARALLEL` au plus à la fois,
    /// chacune sur sa propre connexion ouverte sur la base `database`.
    async fn run_in_parallel(
        &self,
        statements: &[&Statement],
        dialect: Dialect,
        max_rows: usize,
        database: &Option<String>,
    ) -> Result<(), SocketClosed> {
        // Les requêtes partent toutes, mais attendent un jeton avant d'ouvrir leur connexion.
        let permits = Semaphore::new(MAX_PARALLEL);
        let mut runs = Vec::with_capacity(statements.len());
        for (index, statement) in statements.iter().enumerate() {
            let query = index as u32;
            runs.push(self.run_with_permit(
                &permits,
                dialect,
                query,
                &statement.sql,
                max_rows,
                database,
            ));
        }
        let results = join_all(runs).await;
        results.into_iter().collect()
    }

    /// Attend un jeton parmi `permits`, puis exécute la requête sur une nouvelle connexion.
    async fn run_with_permit(
        &self,
        permits: &Semaphore,
        dialect: Dialect,
        query: u32,
        sql: &str,
        max_rows: usize,
        database: &Option<String>,
    ) -> Result<(), SocketClosed> {
        let _permit = permits.acquire().await.map_err(|_| SocketClosed)?;
        if self.cancellation.is_cancelled() {
            return self.outbox.error(Some(query), CANCELLED.to_string()).await;
        }
        self.outbox.send(ServerMessage::Started { query }).await?;
        self.run_on_new_connection(dialect, query, sql, max_rows, database)
            .await
    }

    /// Exécute une requête sur une nouvelle connexion, placée sur la base `database`.
    async fn run_on_new_connection(
        &self,
        dialect: Dialect,
        query: u32,
        sql: &str,
        max_rows: usize,
        database: &Option<String>,
    ) -> Result<(), SocketClosed> {
        let mut connection = match mysql::connect(self.target).await {
            Ok(connection) => connection,
            Err(error) => return self.outbox.error(Some(query), error.to_string()).await,
        };
        let result = self
            .run_on_connection(&mut connection, dialect, query, sql, max_rows, database)
            .await;
        let _ = connection.disconnect().await;
        result
    }

    /// Place la connexion sur la base `database`, puis y exécute la requête.
    async fn run_on_connection(
        &self,
        connection: &mut Connection,
        dialect: Dialect,
        query: u32,
        sql: &str,
        max_rows: usize,
        database: &Option<String>,
    ) -> Result<(), SocketClosed> {
        if let Some(name) = database
            && let Err(error) = mysql::use_database(connection, name).await
        {
            return self.outbox.error(Some(query), error.to_string()).await;
        }
        match self
            .run_query(connection, dialect, query, sql, max_rows)
            .await?
        {
            Some(summary) => self.done(query, summary, database.clone()).await,
            None => Ok(()),
        }
    }

    /// Exécute une requête et envoie ses colonnes et ses lignes en streaming.
    ///
    /// Renvoie le résumé du résultat, ou `None` si la requête a échoué (l'erreur
    /// est alors déjà partie au client).
    async fn run_query(
        &self,
        connection: &mut Connection,
        dialect: Dialect,
        query: u32,
        sql: &str,
        max_rows: usize,
    ) -> Result<Option<Summary>, SocketClosed> {
        if let Some(refusal) = write_refusal(sql, dialect) {
            self.outbox.error(Some(query), refusal).await?;
            return Ok(None);
        }
        let limited = match apply_default_limit(sql, dialect, max_rows as u64) {
            Ok(limited) => limited,
            Err(error) => {
                self.outbox.error(Some(query), error.to_string()).await?;
                return Ok(None);
            }
        };
        // La lecture ne s'arrête d'elle-même que si Prisme a fixé la limite.
        let capped = matches!(
            limited.outcome,
            LimitOutcome::Added | LimitOutcome::Unparsed
        );

        let started = Instant::now();
        let connection_id = connection.id();
        let _running = self.cancellation.track(connection_id);
        // Arrêt demandé pendant l'ouverture de la connexion : la requête ne part pas.
        if self.cancellation.is_cancelled() {
            self.outbox
                .error(Some(query), CANCELLED.to_string())
                .await?;
            return Ok(None);
        }
        let mut stream = match mysql::execute(connection, &limited.sql).await {
            Ok(stream) => stream,
            Err(error) => {
                self.outbox.error(Some(query), self.failure(error)).await?;
                return Ok(None);
            }
        };
        self.outbox
            .send(ServerMessage::Columns {
                query,
                columns: stream.column_names(),
                right_aligned: stream.right_aligned_columns(),
                sql: limited.sql.clone(),
                limit: report(limited.outcome),
            })
            .await?;

        let mut batch = Vec::with_capacity(ROW_BATCH_SIZE);
        let mut row_count = 0;
        let mut truncated = false;
        loop {
            let row = match stream.next_row().await {
                Ok(Some(row)) => row,
                Ok(None) => break,
                Err(error) => {
                    stream.discard().await;
                    self.outbox.error(Some(query), self.failure(error)).await?;
                    return Ok(None);
                }
            };
            if self.cancellation.is_cancelled() {
                // Le serveur a déjà reçu KILL QUERY : on cesse de lire.
                stream.discard().await;
                self.outbox
                    .error(Some(query), CANCELLED.to_string())
                    .await?;
                return Ok(None);
            }
            if capped && row_count == max_rows {
                // La ligne N + 1 : le résultat est tronqué, inutile de lire plus loin.
                truncated = true;
                if limited.outcome == LimitOutcome::Unparsed {
                    // Sans LIMIT, le serveur continuerait d'envoyer des lignes : on l'interrompt.
                    if let Err(error) = mysql::kill_query(self.target, connection_id).await {
                        tracing::warn!("interruption de la requête impossible : {error}");
                    }
                }
                break;
            }
            batch.push(row);
            row_count += 1;
            if batch.len() == ROW_BATCH_SIZE {
                let rows = std::mem::replace(&mut batch, Vec::with_capacity(ROW_BATCH_SIZE));
                self.outbox
                    .send(ServerMessage::Rows { query, rows })
                    .await?;
            }
        }
        if !batch.is_empty() {
            self.outbox
                .send(ServerMessage::Rows { query, rows: batch })
                .await?;
        }

        let affected_rows = stream.affected_rows();
        stream.discard().await;
        Ok(Some(Summary {
            row_count,
            truncated,
            affected_rows,
            elapsed_ms: started.elapsed().as_millis(),
        }))
    }

    /// Message d'une requête en échec : « interrompue » si l'arrêt a été
    /// demandé (l'erreur du serveur est alors celle de `KILL QUERY`).
    fn failure(&self, error: mysql::MysqlError) -> String {
        if self.cancellation.is_cancelled() {
            CANCELLED.to_string()
        } else {
            error.to_string()
        }
    }

    /// Annonce la fin d'un résultat.
    async fn done(
        &self,
        query: u32,
        summary: Summary,
        database: Option<String>,
    ) -> Result<(), SocketClosed> {
        self.outbox
            .send(ServerMessage::Done {
                query,
                row_count: summary.row_count,
                truncated: summary.truncated,
                affected_rows: summary.affected_rows,
                elapsed_ms: summary.elapsed_ms,
                database,
            })
            .await
    }
}

/// Requêtes à exécuter : celle sous le curseur, ou toutes sans curseur.
fn chosen_statements(statements: &[Statement], cursor: Option<usize>) -> Vec<&Statement> {
    match cursor {
        Some(cursor) => statement_at(statements, cursor)
            .map(|index| vec![&statements[index]])
            .unwrap_or_default(),
        None => statements.iter().collect(),
    }
}

/// Limite de lignes d'une demande : celle demandée, sinon le réglage de l'utilisateur.
async fn row_limit(state: &AppState, user_id: i64, requested: Option<u32>) -> Result<u32, String> {
    if let Some(row_limit) = requested {
        return check_row_limit(row_limit).map_err(|error| error.to_string());
    }
    state
        .with_db(move |db, _key| settings::load(db, user_id))
        .await
        .map(|settings| settings.row_limit)
        .map_err(|_| "réglages de l'utilisateur illisibles, voir le journal du serveur".to_string())
}

/// Renvoie le motif du refus si la requête n'est pas une lecture.
///
/// Les écritures restent refusées tant que les interrupteurs d'écriture
/// (étape 4 de la v1) n'existent pas. Une requête non analysable est refusée
/// aussi, car rien ne prouve qu'elle est une lecture.
fn write_refusal(sql: &str, dialect: Dialect) -> Option<String> {
    match classify(sql, dialect) {
        StatementKind::Read => None,
        StatementKind::Write => Some(
            "écriture refusée : les écritures sont désactivées en attendant les interrupteurs d'écriture".to_string(),
        ),
        StatementKind::Unparsed => Some(
            "requête refusée : Prisme ne sait pas l'analyser, donc ne peut pas vérifier qu'elle est une lecture".to_string(),
        ),
    }
}

/// Traduit le traitement de la limite pour le client.
fn report(outcome: LimitOutcome) -> LimitReport {
    match outcome {
        LimitOutcome::Added => LimitReport::Added,
        LimitOutcome::UserLimit => LimitReport::UserLimit,
        LimitOutcome::NotApplicable => LimitReport::NotApplicable,
        LimitOutcome::Unparsed => LimitReport::Unparsed,
    }
}

#[cfg(test)]
mod tests {
    use futures_util::{SinkExt, StreamExt};
    use secrecy::SecretString;
    use serde_json::{Value, json};
    use testcontainers_modules::mariadb::Mariadb;
    use testcontainers_modules::testcontainers::runners::AsyncRunner;
    use tokio::net::TcpStream;
    use tokio_tungstenite::tungstenite::Message as ClientMessage;
    use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

    use crate::api::{AppState, router};
    use crate::connections::{self, NewConnection};
    use crate::master_key::MasterKey;
    use crate::store;
    use prisme_dialects::Dialect;

    type ClientSocket = WebSocketStream<MaybeTlsStream<TcpStream>>;

    /// Démarre Prisme sur un port libre, avec une connexion enregistrée vers
    /// `host:port` (compte root sans mot de passe, base `test`), et renvoie
    /// l'adresse du serveur et l'identifiant de la connexion.
    async fn start(host: &str, port: u16) -> (String, i64) {
        let db = store::open_in_memory();
        let key = MasterKey::generate();
        let user_id = store::ensure_user(&db, "test@example.com").unwrap();
        let new = NewConnection {
            name: "test".to_string(),
            host: host.to_string(),
            port,
            username: "root".to_string(),
            password: SecretString::from(""),
            database_name: "test".to_string(),
        };
        let id = connections::insert(&db, &key, user_id, &new).unwrap();

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let app = router(AppState::new(db, key, user_id));
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (address, id)
    }

    /// Lit le prochain message texte du serveur, en JSON.
    async fn next_json(socket: &mut ClientSocket) -> Value {
        loop {
            if let ClientMessage::Text(text) = socket.next().await.unwrap().unwrap() {
                return serde_json::from_str(&text).unwrap();
            }
        }
    }

    /// Résultat d'une requête d'un lot, tel que reçu par le client.
    #[derive(Debug, Default)]
    struct Received {
        /// Message `columns`, ou Null.
        columns: Value,
        /// Nombre de lignes reçues.
        rows: usize,
        /// Dernier message de la requête : `done` ou `error`.
        end: Value,
        /// Le message `started` est-il arrivé ?
        started: bool,
    }

    /// Envoie une demande, puis lit les messages jusqu'à `batch_done`.
    /// Renvoie le message `batch` (ou une erreur de session) et les résultats, par numéro de requête.
    async fn run(socket: &mut ClientSocket, mut request: Value) -> (Value, Vec<Received>) {
        request["type"] = json!("run");
        socket
            .send(ClientMessage::Text(request.to_string().into()))
            .await
            .unwrap();
        let mut batch = Value::Null;
        let mut results: Vec<Received> = Vec::new();
        loop {
            let message = next_json(socket).await;
            let kind = message["type"].as_str().unwrap().to_string();
            if kind == "batch_done" {
                return (batch, results);
            }
            if kind == "error" && message["query"].is_null() {
                return (message, results);
            }
            if kind == "batch" {
                let count = message["queries"].as_array().unwrap().len();
                results = (0..count).map(|_| Received::default()).collect();
                batch = message;
                continue;
            }
            let result = &mut results[message["query"].as_u64().unwrap() as usize];
            match kind.as_str() {
                "started" => result.started = true,
                "columns" => result.columns = message,
                "rows" => result.rows += message["rows"].as_array().unwrap().len(),
                _ => result.end = message,
            }
        }
    }

    /// Exécute la requête sous le curseur (ici, le seul texte envoyé) et renvoie son résultat.
    async fn run_one(socket: &mut ClientSocket, sql: &str, row_limit: Option<u32>) -> Received {
        let mut request = json!({ "sql": sql, "cursor": 0 });
        if let Some(row_limit) = row_limit {
            request["row_limit"] = json!(row_limit);
        }
        run(socket, request).await.1.remove(0)
    }

    #[test]
    fn chooses_the_statement_under_the_cursor_or_all_of_them() {
        let statements = prisme_splitter::split("SELECT 1;\nSELECT 2;");
        let sql = |chosen: Vec<&prisme_splitter::Statement>| {
            chosen
                .into_iter()
                .map(|statement| statement.sql.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            sql(super::chosen_statements(&statements, Some(12))),
            ["SELECT 2"]
        );
        assert_eq!(
            sql(super::chosen_statements(&statements, None)),
            ["SELECT 1", "SELECT 2"]
        );
        assert!(super::chosen_statements(&[], Some(0)).is_empty());
    }

    #[test]
    fn refuses_writes_and_unparsable_queries() {
        assert_eq!(super::write_refusal("SELECT 1", Dialect::MariaDb), None);
        assert_eq!(super::write_refusal("SHOW TABLES", Dialect::MariaDb), None);
        assert!(super::write_refusal("DELETE FROM t", Dialect::MariaDb).is_some());
        assert!(
            super::write_refusal("WITH x AS (SELECT 1) DELETE FROM t", Dialect::MariaDb).is_some()
        );
        assert!(
            super::write_refusal("SELECT * FROM t PROCEDURE ANALYSE()", Dialect::MariaDb).is_some()
        );
    }

    #[tokio::test]
    async fn unreachable_server_sends_an_error() {
        // Port 1 : aucun serveur n'y écoute.
        let (address, id) = start("127.0.0.1", 1).await;
        let (mut socket, _) = connect_async(format!("ws://{address}/api/connections/{id}/query"))
            .await
            .unwrap();

        assert_eq!(next_json(&mut socket).await["type"], "error");
    }

    #[tokio::test]
    async fn unknown_connection_is_refused() {
        let (address, _) = start("127.0.0.1", 1).await;
        assert!(
            connect_async(format!("ws://{address}/api/connections/999/query"))
                .await
                .is_err()
        );
    }

    #[tokio::test]
    #[ignore = "nécessite Docker Desktop démarré"]
    async fn streams_results_with_the_default_limit() {
        let server = Mariadb::default().start().await.unwrap();
        let host = server.get_host().await.unwrap().to_string();
        let port = server.get_host_port_ipv4(3306).await.unwrap();
        let (address, id) = start(&host, port).await;
        let (mut socket, _) = connect_async(format!("ws://{address}/api/connections/{id}/query"))
            .await
            .unwrap();

        let ready = next_json(&mut socket).await;
        assert_eq!(ready["type"], "ready");
        assert_eq!(ready["server"]["flavor"], "mariadb");
        assert_eq!(ready["database"], "test");

        // Pas de limite écrite : 1 000 lignes affichées sur 2 500, résultat tronqué.
        let result = run_one(&mut socket, "SELECT seq FROM seq_1_to_2500", Some(1000)).await;
        assert_eq!(result.columns["limit"], "added");
        assert_eq!(
            result.columns["sql"],
            "SELECT seq FROM seq_1_to_2500 LIMIT 1001"
        );
        assert_eq!(result.rows, 1000);
        assert_eq!(result.end["truncated"], true);

        // Limite écrite : elle s'applique telle quelle, au-delà de la limite par défaut.
        let result = run_one(
            &mut socket,
            "SELECT seq FROM seq_1_to_2500 LIMIT 1500",
            None,
        )
        .await;
        assert_eq!(result.columns["limit"], "user_limit");
        assert_eq!(result.rows, 1500);
        assert_eq!(result.end["truncated"], false);

        // Une erreur n'interrompt pas la session.
        let result = run_one(&mut socket, "SELECT * FROM missing", None).await;
        assert_eq!(result.end["type"], "error");
        let result = run_one(&mut socket, "SELECT 1", None).await;
        assert_eq!(
            (result.rows, result.end["type"].as_str()),
            (1, Some("done"))
        );

        // USE change la base en cours, annoncée à la fin de la requête.
        let result = run_one(&mut socket, "USE mysql", None).await;
        assert_eq!(result.end["database"], "mysql");

        // Curseur sur la ligne vide entre deux requêtes : la précédente s'exécute.
        let text = "SELECT 'a';\n\nSELECT 'b', 'c';";
        let (batch, results) = run(&mut socket, json!({ "sql": text, "cursor": 12 })).await;
        assert_eq!(batch["queries"][0]["sql"], "SELECT 'a'");
        assert_eq!(results.len(), 1);

        // Sans curseur : toutes les requêtes, en parallèle, sur la base en cours (mysql).
        let request = json!({ "sql": "SELECT 1; SELECT DATABASE(), 2;" });
        let (batch, results) = run(&mut socket, request).await;
        assert_eq!(batch["queries"].as_array().unwrap().len(), 2);
        assert_eq!(results[0].rows, 1);
        assert_eq!(results[1].columns["columns"], json!(["DATABASE()", "2"]));
        assert_eq!(results[1].columns["right_aligned"], json!([false, true]));
        assert_eq!(results[1].end["database"], "mysql");
        assert!(results.iter().all(|result| result.started));

        // Stopper : les requêtes en cours sont interrompues bien avant leur fin.
        let started = std::time::Instant::now();
        let request = json!({ "type": "run", "sql": "SELECT SLEEP(30); SELECT SLEEP(30);" });
        socket
            .send(ClientMessage::Text(request.to_string().into()))
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        let cancel = json!({ "type": "cancel" }).to_string();
        socket
            .send(ClientMessage::Text(cancel.into()))
            .await
            .unwrap();
        let mut errors = 0;
        loop {
            let message = next_json(&mut socket).await;
            match message["type"].as_str().unwrap() {
                "error" => {
                    assert_eq!(message["message"], "requête interrompue par l'utilisateur");
                    errors += 1;
                }
                "batch_done" => break,
                _ => {}
            }
        }
        assert_eq!(errors, 2);
        assert!(started.elapsed() < std::time::Duration::from_secs(10));

        // Rien à exécuter : erreur de session.
        let (error, _) = run(&mut socket, json!({ "sql": " -- rien" })).await;
        assert_eq!(error["message"], "aucune requête à exécuter");
    }
}
