//! Cancellation of a session's queries: stopping the whole batch or a single
//! query, and the list of the connections to interrupt.
//!
//! The batches are numbered in the order the client sent them, when the
//! relay receives them (`receive_batch`). A stop applies to the last batch
//! received, queued or running, and to the batches before it; a batch received
//! after the stop is not affected. A stop therefore reaches a batch even when
//! it is sent just after the stop request, while it waits for the session.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use prisme_connectors::api::Canceller;
use tracing::Instrument;

use crate::console::{self, Action};

/// Stopping of the running queries, requested by the client.
///
/// Keeps the list of the running queries with the canceller of their
/// connection, to be able to interrupt them, the batches stopped as a whole
/// and the queries stopped one by one. Queries check them before starting and while
/// reading their rows.
#[derive(Default)]
pub(crate) struct Cancellation {
    /// Number of the last batch received.
    received: AtomicU64,
    /// Batches up to this number, included, are stopped as a whole.
    stopped_through: AtomicU64,
    /// Queries stopped one by one, by batch and number.
    stopped_queries: Mutex<Vec<(u64, u32)>>,
    /// Running queries: batch, number of the query, number of the run (the
    /// key of its guard) and canceller of its connection.
    running: Mutex<Vec<Running>>,
    /// Number of the last run tracked.
    runs: AtomicU64,
}

/// A running query.
struct Running {
    /// Batch of the query.
    batch: u64,
    /// Number of the query in its batch.
    query: u32,
    /// Number of the run, the key of its guard.
    run: u64,
    /// Stop of the statement of its connection.
    canceller: Arc<dyn Canceller>,
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
    pub(crate) async fn cancel(&self) {
        self.stopped_through
            .store(self.received.load(Ordering::SeqCst), Ordering::SeqCst);
        self.interrupt(|_, _| true).await;
    }

    /// Requests stopping a single query of the last batch received, and
    /// interrupts it if it is running.
    pub(crate) async fn cancel_query(&self, query: u32) {
        let batch = self.received.load(Ordering::SeqCst);
        if let Ok(mut queries) = self.stopped_queries.lock() {
            queries.push((batch, query));
        }
        self.interrupt(|running_batch, running| running_batch == batch && running == query)
            .await;
    }

    /// Interrupts the running queries that `selected` keeps, from their batch
    /// and number.
    async fn interrupt(&self, selected: impl Fn(u64, u32) -> bool) {
        let cancellers: Vec<Arc<dyn Canceller>> = self
            .running
            .lock()
            .map(|running| {
                running
                    .iter()
                    .filter(|running| selected(running.batch, running.query))
                    .map(|running| Arc::clone(&running.canceller))
                    .collect()
            })
            .unwrap_or_default();
        for canceller in cancellers {
            let interrupting = canceller
                .interrupt()
                .instrument(console::action_span(Action::Cancel));
            if let Err(error) = interrupting.await {
                tracing::warn!("cannot interrupt the query: {error}");
            }
        }
    }

    /// Records that query `query` of batch `batch` runs on the connection of
    /// `canceller`, until the returned guard is dropped.
    pub(super) fn track(
        &self,
        batch: u64,
        query: u32,
        canceller: Arc<dyn Canceller>,
    ) -> RunningGuard<'_> {
        let run = self.runs.fetch_add(1, Ordering::SeqCst) + 1;
        if let Ok(mut running) = self.running.lock() {
            running.push(Running {
                batch,
                query,
                run,
                canceller,
            });
        }
        RunningGuard {
            cancellation: self,
            run,
        }
    }
}

/// Guard of a running query: when it is dropped, the query leaves the list.
pub(super) struct RunningGuard<'a> {
    cancellation: &'a Cancellation,
    run: u64,
}

impl Drop for RunningGuard<'_> {
    /// Removes the query from the list of the running queries.
    fn drop(&mut self) {
        if let Ok(mut running) = self.cancellation.running.lock() {
            running.retain(|running| running.run != self.run);
        }
    }
}
