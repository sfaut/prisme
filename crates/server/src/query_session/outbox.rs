//! Outbox of a session: the queue of messages shared by the parallel queries,
//! drained to the client by the relay.

use tokio::sync::mpsc;

use super::protocol::ServerMessage;

/// The message relay is gone: the session stops.
pub(super) struct SessionStopped;

/// Queue of the messages to send to the client, shared by the parallel queries.
///
/// The relay (`session_relay.rs`) drains the queue to the attached WebSocket,
/// if there is one. The queue is bounded: if the client reads slowly, reading
/// the results waits.
#[derive(Clone)]
pub(super) struct Outbox {
    /// To the relay.
    pub(super) sender: mpsc::Sender<ServerMessage>,
}

impl Outbox {
    /// Puts a message in the queue.
    pub(super) async fn send(&self, message: ServerMessage) -> Result<(), SessionStopped> {
        self.sender.send(message).await.map_err(|_| SessionStopped)
    }

    /// Puts an error message in the queue.
    pub(super) async fn error(
        &self,
        query: Option<u32>,
        message: String,
    ) -> Result<(), SessionStopped> {
        self.send(ServerMessage::Error { query, message }).await
    }
}
