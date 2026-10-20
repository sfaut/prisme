//! Query session on a saved connection, over a WebSocket.
//!
//! A session keeps a connection open to the target server (transactions will
//! need it). It outlives its WebSocket: after a disconnection (page reloaded,
//! browser closed), it stays open for the delay set on the connection, and a
//! new WebSocket reattaches to it (`session_relay.rs`, `sessions.rs`). The
//! client sends the editor's whole text; the session splits it into queries
//! (`splitter` crate) and runs:
//!
//! - with a cursor (Ctrl+Enter): the query under the cursor, on the session's
//!   connection;
//! - without a cursor (Ctrl+Shift+Enter): all the queries in parallel, each on
//!   its own connection, opened on the session's current database. A query
//!   that changes the state of its connection (`USE`, `SET`) therefore has no
//!   effect on the others, nor on the session.
//!
//! Results arrive streamed, in batches of rows, never loaded whole in memory.
//! Each message of a result carries the number of its query (`query`): the
//! messages of parallel queries interleave.
//!
//! Protocol, as JSON text:
//!
//! ```text
//! server → client  {"type": "ready", "server": {"flavor": "mariadb", "version": "11.3.2-MariaDB", "ansi_quotes": false}, "database": "xlist"}
//! client → server  {"type": "run", "sql": "SELECT 1; SELECT 2", "cursor": 3, "row_limit": 2000}   cursor and row_limit optional
//! client → server  {"type": "run", "sql": "SELECT 1", "tab": 12}       refreshes the saved result tab 12 with its query
//! client → server  {"type": "cancel"}                                  stops all the running queries
//! client → server  {"type": "cancel_query", "query": 2}                stops a single query of the running batch
//! client → server  {"type": "use", "database": "xlist"}               changes the session's current database
//! server → client  {"type": "database", "database": "xlist"}          answer to use: the current database
//! server → client  {"type": "batch", "queries": [{"query": 0, "sql": "SELECT 1", "start": 0, "end": 9}]}
//! server → client  {"type": "started", "query": 0}                  the query starts (in parallel, it may wait for its turn)
//! server → client  {"type": "columns", "query": 0, "columns": ["1"], "types": [{"family": "integer", "name": "INT"}], "sql": "SELECT 1 LIMIT 1001", "limit": "added", "expected_rows": 1000, "execution_us": 830, "executed_at": 1792500000000}
//! server → client  {"type": "rows", "query": 0, "rows": [["1"]]}       as many batches as needed
//! server → client  {"type": "done", "query": 0, "row_count": 1, "truncated": false, "affected_rows": 0, "elapsed_ms": 2, "database": "xlist"}
//! server → client  {"type": "error", "query": 0, "message": "..."}     instead of the rest, on failure
//! server → client  {"type": "batch_done"}                               all the queries have ended
//! server → client  {"type": "resume", "running": true, "queries": [...]}  on reattaching: the last batch, where it stands
//! ```
//!
//! A WebSocket that reattaches to a session receives `ready`, with the
//! current database, then `resume` for the session of an editor that has
//! already run a batch: each query of the last batch with its tab, its state
//! and the rows received so far; the next messages continue this batch.
//!
//! `cancel` interrupts the running queries (`KILL QUERY` on each of their
//! connections) and prevents those waiting for their turn from starting: each
//! one ends with a "query interrupted" error. `cancel_query` does the same for
//! a single query of the batch (the interface sends it when its tab is
//! closed). The session reads the client's messages while running, to receive
//! these stops in time.
//!
//! An error that concerns no query (invalid message, connection to the target
//! server impossible) has `"query": null`. `database` is the session's current
//! database (`SELECT DATABASE()`, `null` if none): that of the saved
//! connection at first, then the one chosen by `USE`.
//!
//! The row limit follows `docs/V1.md`, section 2.3: the request's
//! `row_limit`, otherwise the user's setting. Until the write switches exist,
//! writes and queries that cannot be parsed are refused (`docs/V1.md`,
//! section 2.4).

mod cancellation;
mod execution;
mod limits;
mod outbox;
pub(crate) mod protocol;
mod session;
#[cfg(test)]
mod tests;

pub(crate) use cancellation::Cancellation;
pub(crate) use protocol::{ClientMessage, LimitReport, ServerMessage, SessionRequest};

use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws::WebSocket;
use prisme_connectors::mysql::MysqlTarget;
use tokio::sync::{mpsc, watch};
use tracing::Instrument;

use crate::api::AppState;
use crate::recorder::Recorder;
use crate::session_relay::{Relay, SessionHandle};

use outbox::Outbox;
use session::serve;

/// Number of messages waiting to be sent beyond which reading the results waits.
const OUTBOX_SIZE: usize = 64;

/// Opens a session on `target`, attached to the WebSocket `socket`, and
/// returns what is needed to reattach another WebSocket to it or to stop it.
///
/// Two tasks share the session: one runs the client's requests (`serve`), the
/// other relays the messages to the current WebSocket and, for an editor's
/// session, saves them (`session_relay.rs`). Both run in the current span,
/// that of the session in the log of the API (`api_log.rs`). Without an
/// attached WebSocket for `keep_alive`, the session stops; `on_end` is then
/// called.
pub(crate) fn start(
    socket: WebSocket,
    state: AppState,
    user_id: i64,
    target: MysqlTarget,
    recorder: Option<Recorder>,
    keep_alive: Duration,
    on_end: Box<dyn FnOnce() + Send + Sync>,
) -> SessionHandle {
    let (sender, messages) = mpsc::channel(OUTBOX_SIZE);
    let (request_sender, requests) = mpsc::channel(1);
    let (stop_sender, stop) = watch::channel(false);
    let (commands, command_receiver) = mpsc::unbounded_channel();
    let cancellation = Arc::new(Cancellation::default());
    let relay = Relay {
        messages,
        commands: command_receiver,
        requests: request_sender,
        outbox: sender.downgrade(),
        cancellation: Arc::clone(&cancellation),
        target: target.clone(),
        recorder,
        stop: stop_sender,
        keep_alive,
        on_end: Some(on_end),
    };
    tokio::spawn(
        serve(
            Outbox { sender },
            requests,
            stop,
            state,
            user_id,
            target,
            cancellation,
        )
        .in_current_span(),
    );
    tokio::spawn(relay.run(socket).in_current_span());
    SessionHandle::new(commands)
}
