//! Engines whose driver blocks (SQLite, later DuckDB): each connection lives
//! on a thread of its own, which owns it, and runs the orders that the async
//! side sends it, one after the other.
//!
//! The server is asynchronous (`tokio`): a call that blocks would hold one
//! of its threads. A `Worker` moves the connection to a dedicated thread
//! instead, where it never leaves: the connection need not be `Send`. An
//! order is a closure run on the connection; its result comes back through
//! a channel the async side awaits. The orders queue: the next one starts
//! when the previous one ended, as statements do on a connection.
//!
//! A result read row by row is an order that sends its rows through a
//! bounded channel (`tokio::sync::mpsc`), which holds the thread while the
//! reader is behind (back-pressure), and ends the order when the reader is
//! gone. Stopping a running order goes through the interruption of the
//! engine, which can be called from any thread (`InterruptHandle` of
//! SQLite).
//!
//! An order runs in the `tracing` span of the task that sent it: the
//! statements it traces (`sql_log`) keep the context the server gives them
//! (user, connection, action), as on the async side.

use std::sync::mpsc;
use std::thread;

use tokio::sync::oneshot;

/// Order sent to the thread of a connection.
type Order<T> = Box<dyn FnOnce(&mut T) + Send>;

/// The thread of a connection stopped: the connection is closed, or its
/// thread could not start.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the thread of the connection stopped")]
pub struct Stopped;

/// Connection held by a thread of its own. Dropping the worker ends the
/// thread once its last order ran, which closes the connection.
pub struct Worker<T> {
    /// Orders to the thread.
    orders: mpsc::Sender<Order<T>>,
}

impl<T: 'static> Worker<T> {
    /// Starts a thread named `name`, which opens its connection with `open`,
    /// then waits for its orders. Returns the error of `open` if it fails, the
    /// thread ending then.
    ///
    /// # Errors
    ///
    /// Fails with the error of `open`, or `Stopped` if the thread could not
    /// start.
    pub async fn spawn<E: Send + 'static>(
        name: &str,
        open: impl FnOnce() -> Result<T, E> + Send + 'static,
    ) -> Result<Result<Self, E>, Stopped> {
        let (orders, received) = mpsc::channel::<Order<T>>();
        let (opened, opening) = oneshot::channel();
        let span = tracing::Span::current();
        thread::Builder::new()
            .name(name.to_string())
            .spawn(move || {
                let mut connection = match span.in_scope(open) {
                    Ok(connection) => {
                        let _ = opened.send(Ok(()));
                        connection
                    }
                    Err(error) => {
                        let _ = opened.send(Err(error));
                        return;
                    }
                };
                // Ends when every sender is dropped: the worker is gone.
                for order in received {
                    order(&mut connection);
                }
            })
            .map_err(|_| Stopped)?;
        Ok(opening
            .await
            .map_err(|_| Stopped)?
            .map(|()| Self { orders }))
    }

    /// Runs `order` on the connection, after the orders sent before it, and
    /// returns its result.
    ///
    /// # Errors
    ///
    /// Fails if the thread stopped (a panic of an order).
    pub async fn run<R: Send + 'static>(
        &self,
        order: impl FnOnce(&mut T) -> R + Send + 'static,
    ) -> Result<R, Stopped> {
        let (done, result) = oneshot::channel();
        self.send(move |connection| {
            let _ = done.send(order(connection));
        })?;
        result.await.map_err(|_| Stopped)
    }

    /// Sends `order` to the connection, without waiting for it: an order
    /// that answers through a channel of its own (the rows of a result). It
    /// runs in the current span.
    ///
    /// # Errors
    ///
    /// Fails if the thread stopped.
    pub fn send(&self, order: impl FnOnce(&mut T) + Send + 'static) -> Result<(), Stopped> {
        let span = tracing::Span::current();
        self.orders
            .send(Box::new(move |connection| {
                span.in_scope(|| order(connection))
            }))
            .map_err(|_| Stopped)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn runs_the_orders_in_turn_on_its_own_thread() {
        let worker = Worker::spawn("test", || Ok::<_, ()>(Vec::new()))
            .await
            .unwrap()
            .unwrap();
        let here = thread::current().id();
        for value in 0..3 {
            worker
                .send(move |values: &mut Vec<i32>| values.push(value))
                .unwrap();
        }
        let (values, there) = worker
            .run(|values| (values.clone(), thread::current().id()))
            .await
            .unwrap();
        assert_eq!(values, [0, 1, 2]);
        assert_ne!(there, here);
    }

    #[tokio::test]
    async fn gives_the_error_of_the_opening() {
        let opened = Worker::<()>::spawn("test", || Err("no file"))
            .await
            .unwrap();
        assert_eq!(opened.err(), Some("no file"));
    }

    #[tokio::test]
    async fn a_panic_stops_the_thread() {
        let worker = Worker::spawn("test", || Ok::<_, ()>(0))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            worker.run(|_: &mut i32| panic!("order")).await,
            Err::<(), _>(Stopped)
        );
        assert_eq!(worker.run(|value| *value).await, Err(Stopped));
    }
}
