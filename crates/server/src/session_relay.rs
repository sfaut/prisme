//! Relay between a query session and the WebSocket attached to it.
//!
//! A session outlives its WebSocket: the page can be reloaded, or the browser
//! closed then reopened, without losing the connection to the target server,
//! its transactions or its running queries. The relay is the task that links
//! them:
//!
//! - it drains the session's message queue (`query_session/outbox.rs`), shows the
//!   messages to the editor's result recorder (`recorder.rs`), then sends
//!   them to the attached WebSocket, if there is one; without a WebSocket,
//!   messages are only recorded;
//! - it reads the client's messages (`read_messages`) and passes them on to
//!   the session; a stop (`cancel`) is handled at once;
//! - a new WebSocket replaces the previous one (closed) and first receives
//!   the state of the session: `ready`, with the current database, then
//!   `resume`, the last batch where it stands;
//! - without a WebSocket for the keep-alive delay, it stops the session: the
//!   running queries are interrupted, the connection to the target server is
//!   closed (an open transaction is then rolled back by the server).
//!
//! All messages go through the relay, in order: what it sends to a new
//! WebSocket is therefore exactly the state reached by the messages already
//! relayed, and the next ones continue it without duplicates.

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
use tracing::Instrument;

use crate::api_log;
use crate::query_session::{Cancellation, ClientMessage, ServerMessage, SessionRequest};
use crate::recorder::Recorder;

/// Order passed to the relay by the session registry (`sessions.rs`).
pub(crate) enum Command {
    /// Attach this WebSocket to the session, instead of the previous one.
    Attach(Box<WebSocket>),
    /// Stop the session.
    Stop,
}

/// Access to an open session: reattach a WebSocket to it, or stop it.
pub(crate) struct SessionHandle {
    /// To the relay of the session.
    commands: mpsc::UnboundedSender<Command>,
}

impl SessionHandle {
    /// Access to the session whose relay reads `commands`.
    pub(crate) fn new(commands: mpsc::UnboundedSender<Command>) -> Self {
        Self { commands }
    }

    /// Reattaches `socket` to the session. If the session has already ended,
    /// the WebSocket is given back, to open another one.
    pub(crate) fn attach(&self, socket: WebSocket) -> Result<(), Box<WebSocket>> {
        self.commands
            .send(Command::Attach(Box::new(socket)))
            .map_err(|error| match error.0 {
                Command::Attach(socket) => socket,
                Command::Stop => unreachable!("only an attachment was sent"),
            })
    }

    /// Stops the session: running queries interrupted, connection closed.
    pub(crate) fn stop(&self) {
        let _ = self.commands.send(Command::Stop);
    }
}

/// State of the session that a new WebSocket receives before what follows.
#[derive(Default)]
struct Replay {
    /// Identity of the target server, known once the session is ready.
    server: Option<ServerInfo>,
    /// Current database of the session.
    database: Option<String>,
    /// Is a batch running?
    running: bool,
}

/// Relay of a session, built by `query_session::start`.
pub(crate) struct Relay {
    /// Message queue of the session.
    pub(crate) messages: mpsc::Receiver<ServerMessage>,
    /// Orders of the session registry.
    pub(crate) commands: mpsc::UnboundedReceiver<Command>,
    /// To the session: requests to run and to change the database.
    pub(crate) requests: mpsc::Sender<SessionRequest>,
    /// To the message queue, for errors reading the client's messages. Weak:
    /// the queue closes when the session stops, even if a WebSocket is still
    /// being read.
    pub(crate) outbox: mpsc::WeakSender<ServerMessage>,
    /// Stopping of the running queries.
    pub(crate) cancellation: Arc<Cancellation>,
    /// Connection settings, to interrupt the queries.
    pub(crate) target: MysqlTarget,
    /// Recorder of the editor's results, absent outside an editor.
    pub(crate) recorder: Option<Recorder>,
    /// Stop signal of the session.
    pub(crate) stop: watch::Sender<bool>,
    /// Keep-alive delay of the session without a WebSocket.
    pub(crate) keep_alive: Duration,
    /// Called once, when the session starts stopping.
    pub(crate) on_end: Option<Box<dyn FnOnce() + Send + Sync>>,
}

/// Attached WebSocket: writing of the messages and reading task.
struct Attached {
    /// Writing to the client.
    sink: SplitSink<WebSocket, Message>,
    /// Reading of the client's messages.
    reader: JoinHandle<()>,
}

impl Relay {
    /// Relays the session's messages, first to `socket`, until it stops.
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

    /// Attaches `socket`: the reading of its messages starts, in the span of
    /// the session.
    fn attach(&self, socket: WebSocket) -> Attached {
        let (sink, incoming) = socket.split();
        let reader = tokio::spawn(
            read_messages(
                incoming,
                self.requests.clone(),
                self.outbox.clone(),
                Arc::clone(&self.cancellation),
                self.target.clone(),
            )
            .in_current_span(),
        );
        Attached { sink, reader }
    }

    /// Sends a reattaching WebSocket the state of the session: `ready` if it
    /// is ready, then the editor's last batch if there is one.
    async fn send_replay(
        &self,
        sink: &mut SplitSink<WebSocket, Message>,
        replay: &Replay,
    ) -> Result<(), axum::Error> {
        let Some(server) = &replay.server else {
            // Session still opening: `ready` will come with what follows.
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

    /// Takes note of a message before sending it: recording of the result,
    /// state to replay for a next WebSocket.
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

    /// Starts stopping the session: it stops reading requests, its running
    /// queries are interrupted, and it disappears from the registry (a new
    /// WebSocket will open another session). The relay keeps draining the
    /// queue until the end of the session.
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
    /// Closes the WebSocket and stops reading its messages.
    async fn close(mut self) {
        self.reader.abort();
        let _ = self.sink.close().await;
    }
}

/// Waits for the end of the reading of the attached WebSocket's messages (the
/// client disconnected); without a WebSocket, never happens.
async fn reader_end(attached: &mut Option<Attached>) {
    match attached {
        Some(current) => {
            let _ = (&mut current.reader).await;
        }
        None => pending().await,
    }
}

/// Waits for `deadline`; without a deadline, never happens.
async fn until(deadline: Option<Instant>) {
    match deadline {
        Some(deadline) => sleep_until(deadline).await,
        None => pending().await,
    }
}

/// Sends a message to the client, in JSON, and writes it in the log of the API.
async fn send(
    sink: &mut SplitSink<WebSocket, Message>,
    message: &ServerMessage,
) -> Result<(), axum::Error> {
    let json = serde_json::to_string(message)
        .expect("a server message always serialises: its fields are plain data");
    sink.send(Message::Text(json.into())).await?;
    api_log::sent(message);
    Ok(())
}

/// Reads the client's messages as long as its WebSocket is open.
///
/// A stop (`cancel`, `cancel_query`) is handled at once, even while a batch
/// runs; a request to run (`run`) or to change the database (`use`) is passed
/// on to the session.
async fn read_messages(
    mut incoming: SplitStream<WebSocket>,
    requests: mpsc::Sender<SessionRequest>,
    outbox: mpsc::WeakSender<ServerMessage>,
    cancellation: Arc<Cancellation>,
    target: MysqlTarget,
) {
    while let Some(Ok(message)) = incoming.next().await {
        let Message::Text(text) = message else {
            // Ping, pong and binary are ignored; Close ends the loop on the next turn.
            continue;
        };
        let message = serde_json::from_str::<ClientMessage>(text.as_str());
        api_log::received(message.as_ref().ok());
        match message {
            Ok(ClientMessage::Run(request)) => {
                if requests
                    .send(SessionRequest::Run {
                        batch: cancellation.receive_batch(),
                        request,
                    })
                    .await
                    .is_err()
                {
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
                    message: format!("invalid message: {error}"),
                };
                if sender.send(message).await.is_err() {
                    break;
                }
            }
        }
    }
}
