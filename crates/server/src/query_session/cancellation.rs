//! Cancellation of a session's queries: stopping the whole batch or a single
//! query, and the list of the connections to interrupt.
//!
//! The batches are numbered in the order the client sent them, when the
//! relay receives them (`receive_batch`). A stop applies to the last batch
//! received, queued or running, and to the batches before it; a batch received
//! after the stop is not affected. A stop therefore reaches a batch even when
//! it is sent just after the stop request, while it waits for the session.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use prisme_connectors::mysql::{self, MysqlTarget};
use tracing::Instrument;

use crate::console::{self, Action};

/// Stopping of the running queries, requested by the client.
///
/// Keeps the list of the running queries with their connection, to be able
/// to interrupt them (`KILL QUERY`), the batches stopped as a whole and the
/// queries stopped one by one. Queries check them before starting and while
/// reading their rows.
#[derive(Default)]
pub(crate) struct Cancellation {
    /// Number of the last batch received.
    received: AtomicU64,
    /// Batches up to this number, included, are stopped as a whole.
    stopped_through: AtomicU64,
    /// Queries stopped one by one, by batch and number.
    stopped_queries: Mutex<Vec<(u64, u32)>>,
    /// Running queries: batch, number of the query and id (`Conn::id()`) of its
    /// connection.
    running: Mutex<Vec<(u64, u32, u32)>>,
}

impl Cancellation {
    /// Numbers a batch that the relay has just received, and returns its number.
    pub(crate) fn receive_batch(&self) -> u64 {
        self.received.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// Has stopping the query `query` of batch `batch` been requested, alone or
    /// with the batch?
    pub(super) fn is_cancelled(&self, batch: u64, query: u32) -> bool {
        self.stopped_through.load(Ordering::SeqCst) >= batch
            || self
                .stopped_queries
                .lock()
                .is_ok_and(|queries| queries.contains(&(batch, query)))
    }

    /// Requests stopping the last batch received, and the batches before it, and
    /// interrupts all the running queries.
    pub(crate) async fn cancel(&self, target: &MysqlTarget) {
        self.stopped_through
            .store(self.received.load(Ordering::SeqCst), Ordering::SeqCst);
        self.kill(target, |_, _| true).await;
    }

    /// Requests stopping a single query of the last batch received, and
    /// interrupts it if it is running.
    pub(crate) async fn cancel_query(&self, target: &MysqlTarget, query: u32) {
        let batch = self.received.load(Ordering::SeqCst);
        if let Ok(mut queries) = self.stopped_queries.lock() {
            queries.push((batch, query));
        }
        self.kill(target, |running_batch, running| {
            running_batch == batch && running == query
        })
        .await;
    }

    /// Interrupts (`KILL QUERY`) the running queries that `selected` keeps, from
    /// their batch and number.
    async fn kill(&self, target: &MysqlTarget, selected: impl Fn(u64, u32) -> bool) {
        let connections: Vec<u32> = self
            .running
            .lock()
            .map(|running| {
                running
                    .iter()
                    .filter(|(batch, query, _)| selected(*batch, *query))
                    .map(|(_, _, connection_id)| *connection_id)
                    .collect()
            })
            .unwrap_or_default();
        for connection_id in connections {
            let killing = mysql::kill_query(target, connection_id)
                .instrument(console::action_span(Action::Cancel));
            if let Err(error) = killing.await {
                tracing::warn!("cannot interrupt the query: {error}");
            }
        }
    }

    /// Records that query `query` of batch `batch` runs on a connection, until
    /// the returned guard is dropped.
    pub(super) fn track(&self, batch: u64, query: u32, connection_id: u32) -> RunningGuard<'_> {
        if let Ok(mut running) = self.running.lock() {
            running.push((batch, query, connection_id));
        }
        RunningGuard {
            cancellation: self,
            connection_id,
        }
    }
}

/// Guard of a running query: when it is dropped, the connection leaves the list.
pub(super) struct RunningGuard<'a> {
    cancellation: &'a Cancellation,
    connection_id: u32,
}

impl Drop for RunningGuard<'_> {
    /// Removes the connection from the list of the running queries.
    fn drop(&mut self) {
        if let Ok(mut running) = self.cancellation.running.lock() {
            running.retain(|&(_, _, id)| id != self.connection_id);
        }
    }
}
