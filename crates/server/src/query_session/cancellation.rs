//! Cancellation of a session's queries: stopping the whole batch or a single
//! query, and the list of the connections to interrupt.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use prisme_connectors::mysql::{self, MysqlTarget};

/// Stopping of the running queries, requested by the client.
///
/// Keeps the list of the running queries with their connection, to be able
/// to interrupt them (`KILL QUERY`), a flag for stopping the whole batch and
/// the list of the queries stopped one by one. Queries check both before
/// starting and while reading their rows.
#[derive(Default)]
pub(crate) struct Cancellation {
    /// Has stopping the whole batch been requested?
    cancelled: AtomicBool,
    /// Numbers of the queries of the batch stopped one by one.
    cancelled_queries: Mutex<Vec<u32>>,
    /// Running queries: number of the query and id (`Conn::id()`) of its
    /// connection.
    running: Mutex<Vec<(u32, u32)>>,
}

impl Cancellation {
    /// Prepares a new batch: no stop requested anymore.
    pub(super) fn reset(&self) {
        self.cancelled.store(false, Ordering::SeqCst);
        if let Ok(mut queries) = self.cancelled_queries.lock() {
            queries.clear();
        }
    }

    /// Has stopping the query `query` been requested, alone or with the whole batch?
    pub(super) fn is_cancelled(&self, query: u32) -> bool {
        self.cancelled.load(Ordering::SeqCst)
            || self
                .cancelled_queries
                .lock()
                .is_ok_and(|queries| queries.contains(&query))
    }

    /// Requests stopping the whole batch and interrupts all the running queries.
    pub(crate) async fn cancel(&self, target: &MysqlTarget) {
        self.cancelled.store(true, Ordering::SeqCst);
        self.kill(target, |_| true).await;
    }

    /// Requests stopping a single query and interrupts it if it is running.
    pub(crate) async fn cancel_query(&self, target: &MysqlTarget, query: u32) {
        if let Ok(mut queries) = self.cancelled_queries.lock() {
            queries.push(query);
        }
        self.kill(target, |running| running == query).await;
    }

    /// Interrupts (`KILL QUERY`) the running queries that `selected` keeps.
    async fn kill(&self, target: &MysqlTarget, selected: impl Fn(u32) -> bool) {
        let connections: Vec<u32> = self
            .running
            .lock()
            .map(|running| {
                running
                    .iter()
                    .filter(|(query, _)| selected(*query))
                    .map(|(_, connection_id)| *connection_id)
                    .collect()
            })
            .unwrap_or_default();
        for connection_id in connections {
            if let Err(error) = mysql::kill_query(target, connection_id).await {
                tracing::warn!("cannot interrupt the query: {error}");
            }
        }
    }

    /// Records that the query `query` runs on a connection, until the returned
    /// guard is dropped.
    pub(super) fn track(&self, query: u32, connection_id: u32) -> RunningGuard<'_> {
        if let Ok(mut running) = self.running.lock() {
            running.push((query, connection_id));
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
            running.retain(|&(_, id)| id != self.connection_id);
        }
    }
}
