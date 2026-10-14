//! Relais entre une session de requêtes et le WebSocket qui y est rattaché.
//!
//! Une session survit à son WebSocket : la page peut être rafraîchie, ou le
//! navigateur fermé puis rouvert, sans perdre la connexion au serveur cible,
//! ses transactions ni ses requêtes en cours. Le relais est la tâche qui
//! fait le lien :
//!
//! - il vide la file des messages de la session (`query_session.rs`), les
//!   montre à l'enregistreur des résultats de l'éditeur (`recorder.rs`), puis
//!   les envoie au WebSocket rattaché, s'il y en a un ; sans WebSocket, les
//!   messages sont seulement enregistrés ;
//! - il lit les messages du client (`read_messages`) et les transmet à la
//!   session ; un arrêt (`cancel`) est traité aussitôt ;
//! - un nouveau WebSocket remplace le précédent (fermé) et reçoit d'abord
//!   l'état de la session : `ready`, avec la base en cours, puis `resume`, le
//!   dernier lot où il en est ;
//! - sans WebSocket pendant le délai de garde, il arrête la session : les
//!   requêtes en cours sont interrompues, la connexion au serveur cible est
//!   fermée (une transaction ouverte est alors annulée par le serveur).
//!
//! Tous les messages passent par le relais, dans l'ordre : ce qu'il envoie à
//! un nouveau WebSocket est donc exactement l'état atteint par les messages
//! déjà relayés, et les suivants le continuent sans doublon.

use std::future::pending;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws::{Message, WebSocket};
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use prisme_connectors::mysql::{MysqlTarget, ServerInfo};
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tokio::time::{Instant, sleep_until};

use crate::query_session::{Cancellation, ClientMessage, ServerMessage, SessionRequest};
use crate::recorder::Recorder;

/// Ordre transmis au relais par le registre des sessions (`sessions.rs`).
pub(crate) enum Command {
    /// Rattacher ce WebSocket à la session, à la place du précédent.
    Attach(Box<WebSocket>),
    /// Arrêter la session.
    Stop,
}

/// Accès à une session ouverte : y raccrocher un WebSocket, ou l'arrêter.
pub(crate) struct SessionHandle {
    /// Vers le relais de la session.
    commands: mpsc::UnboundedSender<Command>,
}

impl SessionHandle {
    /// Accès à la session dont le relais lit `commands`.
    pub(crate) fn new(commands: mpsc::UnboundedSender<Command>) -> Self {
        Self { commands }
    }

    /// Raccroche `socket` à la session. Si la session est déjà terminée, le
    /// WebSocket est rendu, pour en ouvrir une autre.
    pub(crate) fn attach(&self, socket: WebSocket) -> Result<(), Box<WebSocket>> {
        self.commands
            .send(Command::Attach(Box::new(socket)))
            .map_err(|error| match error.0 {
                Command::Attach(socket) => socket,
                Command::Stop => unreachable!("seul un rattachement a été envoyé"),
            })
    }

    /// Arrête la session : requêtes en cours interrompues, connexion fermée.
    pub(crate) fn stop(&self) {
        let _ = self.commands.send(Command::Stop);
    }
}

/// État de la session qu'un nouveau WebSocket reçoit avant la suite.
#[derive(Default)]
struct Replay {
    /// Identité du serveur cible, connue une fois la session prête.
    server: Option<ServerInfo>,
    /// Base en cours de la session.
    database: Option<String>,
    /// Un lot est-il en cours ?
    running: bool,
}

/// Relais d'une session, construit par `query_session::start`.
pub(crate) struct Relay {
    /// File des messages de la session.
    pub(crate) messages: mpsc::Receiver<ServerMessage>,
    /// Ordres du registre des sessions.
    pub(crate) commands: mpsc::UnboundedReceiver<Command>,
    /// Vers la session : demandes d'exécution et de changement de base.
    pub(crate) requests: mpsc::Sender<SessionRequest>,
    /// Vers la file des messages, pour les erreurs de lecture des messages du
    /// client. Faible : la file se ferme quand la session s'arrête, même si
    /// un WebSocket est encore lu.
    pub(crate) outbox: mpsc::WeakSender<ServerMessage>,
    /// Arrêt des requêtes en cours.
    pub(crate) cancellation: Arc<Cancellation>,
    /// Paramètres de connexion, pour interrompre les requêtes.
    pub(crate) target: MysqlTarget,
    /// Enregistreur des résultats de l'éditeur, absent hors éditeur.
    pub(crate) recorder: Option<Recorder>,
    /// Signal d'arrêt de la session.
    pub(crate) stop: watch::Sender<bool>,
    /// Délai de garde de la session sans WebSocket.
    pub(crate) keep_alive: Duration,
    /// Appelée une fois, quand la session commence à s'arrêter.
    pub(crate) on_end: Option<Box<dyn FnOnce() + Send + Sync>>,
}

/// WebSocket rattaché : écriture des messages et tâche de lecture.
struct Attached {
    /// Écriture vers le client.
    sink: SplitSink<WebSocket, Message>,
    /// Lecture des messages du client.
    reader: JoinHandle<()>,
}

impl Relay {
    /// Relaie les messages de la session, d'abord vers `socket`, jusqu'à son arrêt.
    pub(crate) async fn run(mut self, socket: WebSocket) {
        let mut replay = Replay::default();
        let mut attached = Some(self.attach(socket));
        let mut deadline: Option<Instant> = None;
        let mut commands_open = true;
        loop {
            tokio::select! {
                biased;
                command = self.commands.recv(), if commands_open => match command {
                    Some(Command::Attach(socket)) => {
                        if let Some(old) = attached.take() {
                            old.close().await;
                        }
                        let mut new = self.attach(*socket);
                        if self.send_replay(&mut new.sink, &replay).await.is_ok() {
                            attached = Some(new);
                            deadline = None;
                        } else {
                            new.reader.abort();
                            deadline = Some(Instant::now() + self.keep_alive);
                        }
                    }
                    Some(Command::Stop) => self.begin_stop(&mut deadline).await,
                    None => commands_open = false,
                },
                message = self.messages.recv() => {
                    let Some(mut message) = message else { break };
                    self.observe(&mut message, &mut replay).await;
                    if let Some(current) = attached.as_mut()
                        && send(&mut current.sink, &message).await.is_err()
                    {
                        if let Some(old) = attached.take() {
                            old.reader.abort();
                        }
                        deadline = Some(Instant::now() + self.keep_alive);
                    }
                }
                () = reader_end(&mut attached) => {
                    attached = None;
                    deadline = Some(Instant::now() + self.keep_alive);
                }
                () = until(deadline) => self.begin_stop(&mut deadline).await,
            }
        }
        if let Some(old) = attached.take() {
            old.close().await;
        }
        if let Some(recorder) = &self.recorder {
            recorder.finish().await;
        }
        if let Some(on_end) = self.on_end.take() {
            on_end();
        }
    }

    /// Rattache `socket` : la lecture de ses messages démarre.
    fn attach(&self, socket: WebSocket) -> Attached {
        let (sink, incoming) = socket.split();
        let reader = tokio::spawn(read_messages(
            incoming,
            self.requests.clone(),
            self.outbox.clone(),
            Arc::clone(&self.cancellation),
            self.target.clone(),
        ));
        Attached { sink, reader }
    }

    /// Envoie à un WebSocket qui se raccroche l'état de la session : `ready`
    /// si elle est prête, puis le dernier lot de l'éditeur s'il y en a un.
    async fn send_replay(
        &self,
        sink: &mut SplitSink<WebSocket, Message>,
        replay: &Replay,
    ) -> Result<(), axum::Error> {
        let Some(server) = &replay.server else {
            // Session encore en cours d'ouverture : `ready` viendra avec la suite.
            return Ok(());
        };
        let ready = ServerMessage::Ready {
            server: server.clone(),
            database: replay.database.clone(),
        };
        send(sink, &ready).await?;
        if let Some(recorder) = &self.recorder {
            let queries = recorder.snapshot().await;
            if !queries.is_empty() {
                let resume = ServerMessage::Resume {
                    running: replay.running,
                    queries,
                };
                send(sink, &resume).await?;
            }
        }
        Ok(())
    }

    /// Prend note d'un message avant son envoi : enregistrement du résultat,
    /// état à rejouer pour un prochain WebSocket.
    async fn observe(&self, message: &mut ServerMessage, replay: &mut Replay) {
        if let Some(recorder) = &self.recorder {
            recorder.observe(message).await;
        }
        match message {
            ServerMessage::Ready { server, database } => {
                replay.server = Some(server.clone());
                replay.database = database.clone();
            }
            ServerMessage::Database { database } | ServerMessage::Done { database, .. } => {
                replay.database = database.clone();
            }
            ServerMessage::Batch { .. } => replay.running = true,
            ServerMessage::BatchDone | ServerMessage::Error { query: None, .. } => {
                replay.running = false;
            }
            _ => {}
        }
    }

    /// Commence l'arrêt de la session : elle cesse de lire les demandes, ses
    /// requêtes en cours sont interrompues, et elle disparaît du registre
    /// (un nouveau WebSocket ouvrira une autre session). Le relais continue
    /// de vider la file jusqu'à la fin de la session.
    async fn begin_stop(&mut self, deadline: &mut Option<Instant>) {
        *deadline = None;
        if *self.stop.borrow() {
            return;
        }
        let _ = self.stop.send(true);
        self.cancellation.cancel(&self.target).await;
        if let Some(on_end) = self.on_end.take() {
            on_end();
        }
    }
}

impl Attached {
    /// Ferme le WebSocket et arrête la lecture de ses messages.
    async fn close(mut self) {
        self.reader.abort();
        let _ = self.sink.close().await;
    }
}

/// Attend la fin de la lecture des messages du WebSocket rattaché (le client
/// s'est déconnecté) ; sans WebSocket, n'arrive jamais.
async fn reader_end(attached: &mut Option<Attached>) {
    match attached {
        Some(current) => {
            let _ = (&mut current.reader).await;
        }
        None => pending().await,
    }
}

/// Attend l'échéance `deadline` ; sans échéance, n'arrive jamais.
async fn until(deadline: Option<Instant>) {
    match deadline {
        Some(deadline) => sleep_until(deadline).await,
        None => pending().await,
    }
}

/// Envoie un message au client, en JSON.
async fn send(
    sink: &mut SplitSink<WebSocket, Message>,
    message: &ServerMessage,
) -> Result<(), axum::Error> {
    let json = serde_json::to_string(message).expect("message sérialisable");
    sink.send(Message::Text(json.into())).await
}

/// Lit les messages du client tant que son WebSocket est ouvert.
///
/// Un arrêt (`cancel`, `cancel_query`) est traité aussitôt, même pendant
/// l'exécution d'un lot ; une demande d'exécution (`run`) ou de changement
/// de base (`use`) est transmise à la session.
async fn read_messages(
    mut incoming: SplitStream<WebSocket>,
    requests: mpsc::Sender<SessionRequest>,
    outbox: mpsc::WeakSender<ServerMessage>,
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
                if requests.send(SessionRequest::Run(request)).await.is_err() {
                    break;
                }
            }
            Ok(ClientMessage::Use { database }) => {
                if requests.send(SessionRequest::Use(database)).await.is_err() {
                    break;
                }
            }
            Ok(ClientMessage::Cancel) => cancellation.cancel(&target).await,
            Ok(ClientMessage::CancelQuery { query }) => {
                cancellation.cancel_query(&target, query).await
            }
            Err(error) => {
                let Some(sender) = outbox.upgrade() else {
                    break;
                };
                let message = ServerMessage::Error {
                    query: None,
                    message: format!("message invalide : {error}"),
                };
                if sender.send(message).await.is_err() {
                    break;
                }
            }
        }
    }
}
