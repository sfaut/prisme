//! Task of an export: runs the query on the export's own connection and
//! writes its rows to the export's file as they arrive.
//!
//! The memory stays the same whatever the size of the result: the driver
//! reads the rows one by one, without buffering the result, and they are
//! written by chunks of `CHUNK` bytes. The progress is announced every
//! `PROGRESS_INTERVAL` at most. The query goes to the history of the queries
//! (`sql_log`), with its execution time, until the columns, and the transfer
//! time of its rows.
//!
//! An export that stops before the end of its result (stop of the user,
//! failure to write) closes its connection on the server (`KILL
//! CONNECTION`), and ends at once, without reading the rows already on their
//! way: between the server and Prisme, the buffers of the network can hold
//! millions of them (measured: 975,792 rows of `seq_1_to_1000000000` still
//! arrived after a `KILL QUERY`, through the ports of Docker Desktop, read in
//! 9 s by a debug build). Its connection closes in the background, as
//! `mysql_async` reads them before closing.

use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use prisme_connectors::mysql::sql_log::{self, Sent};
use prisme_connectors::mysql::{self, Connection, Flavor, MysqlError, ResultStream, TypeFamily};
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

use super::{ExportEvent, Exports, Running, csv};
use crate::api::AppState;
use crate::error_detail::ErrorDetail;

/// Bytes of rows gathered before a write to the file.
const CHUNK: usize = 256 * 1024;

/// Least interval between two announcements of the progress.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(500);

/// Outcome of an export.
pub(super) enum Outcome {
    /// All the rows are written.
    Ready,
    /// Stopped by the user.
    Cancelled,
    /// The query or the writing failed.
    Failed(ErrorDetail),
}

impl Outcome {
    /// Failure message for the history of the queries, `None` for a success.
    fn error(&self) -> Option<String> {
        match self {
            Self::Ready => None,
            Self::Cancelled => Some(ErrorDetail::QueryInterrupted.to_string()),
            Self::Failed(detail) => Some(detail.to_string()),
        }
    }
}

/// Export to write, with what it needs.
pub(super) struct Job {
    /// State of the application: internal database and registry of the exports.
    pub state: AppState,
    /// The export, followed by the registry.
    pub running: Arc<Running>,
    /// Connection of the export, read-only.
    pub connection: Connection,
    /// Flavor of the server, for its error messages.
    pub flavor: Flavor,
    /// Query, run as it is written.
    pub sql: String,
    /// Index of each exported column in the result; empty: all, in order.
    pub columns: Vec<usize>,
    /// File of the export, created empty.
    pub file: File,
}

impl Job {
    /// Writes the export, closes its file and its connection (in the
    /// background), then saves its outcome: once announced, the file can be
    /// downloaded or deleted.
    pub(super) async fn run(self) {
        let Self {
            state,
            running,
            mut connection,
            flavor,
            sql,
            columns,
            mut file,
        } = self;
        let writer = Writer {
            exports: &state.exports,
            running: &running,
            flavor,
        };
        let outcome = writer
            .write(&mut connection, &sql, &columns, &mut file)
            .await;
        drop(file);
        // Closed in the background: `mysql_async` first reads what remains
        // of an abandoned result, which the closing of the connection on the
        // server limits to the rows already on their way.
        tokio::spawn(connection.disconnect());
        super::finish(&state, &running, outcome).await;
    }
}

/// Writing of an export: where its progress goes.
struct Writer<'a> {
    /// Registry of the exports, which announces the progress.
    exports: &'a Exports,
    /// The export and its counters.
    running: &'a Running,
    /// Flavor of the server, for its error messages.
    flavor: Flavor,
}

impl Writer<'_> {
    /// Runs the query and writes its rows, then traces the query for the
    /// history of the queries.
    async fn write(
        &self,
        connection: &mut Connection,
        sql: &str,
        columns: &[usize],
        file: &mut File,
    ) -> Outcome {
        // Stop requested before the query.
        if self.running.is_cancelled() {
            return Outcome::Cancelled;
        }
        let executing = Instant::now();
        let executed = mysql::execute(connection, sql).await;
        let execution = executing.elapsed();
        let (outcome, rows, transfer) = match executed {
            Ok(stream) => {
                let transferring = Instant::now();
                let outcome = self.transfer(stream, columns, file).await;
                let rows = self.running.rows.load(Ordering::Relaxed);
                (outcome, Some(rows), transferring.elapsed())
            }
            Err(error) => (self.failure(&error), None, Duration::ZERO),
        };
        sql_log::record(&Sent {
            sql,
            duration: execution,
            transfer,
            rows,
            error: outcome.error().as_deref(),
            ..Sent::default()
        });
        outcome
    }

    /// Writes the rows of the result to the file, in the order of `columns`.
    /// The rows read before a stop or a failure of the query are written
    /// too: the partial file holds as many rows as the export counts.
    async fn transfer(
        &self,
        mut stream: ResultStream<'_>,
        columns: &[usize],
        file: &mut File,
    ) -> Outcome {
        let names = stream.column_names();
        let order = match column_order(columns, &names) {
            Ok(order) => order,
            Err(detail) => return self.abandon(Outcome::Failed(detail)).await,
        };
        let types = stream.column_types();
        let families: Vec<TypeFamily> = order.iter().map(|&index| types[index].family).collect();

        let mut chunk = String::with_capacity(CHUNK);
        csv::write_start(&mut chunk, order.iter().map(|&index| names[index].as_str()));
        let mut rows = 0;
        let mut announced = Instant::now();
        loop {
            let row = match stream.next_row().await {
                Ok(Some(row)) => row,
                Ok(None) => break,
                Err(error) => {
                    return self.close(file, &mut chunk, self.failure(&error)).await;
                }
            };
            if self.running.is_cancelled() {
                // The stop closes the connection on the server: the rows on
                // their way stay unread.
                return self.close(file, &mut chunk, Outcome::Cancelled).await;
            }
            csv::write_row(&mut chunk, &row, &order, &families);
            rows += 1;
            self.running.rows.store(rows, Ordering::Relaxed);
            if chunk.len() >= CHUNK
                && let Err(error) = self.save(file, &mut chunk).await
            {
                return self.abandon(file_failure(&error)).await;
            }
            if announced.elapsed() >= PROGRESS_INTERVAL {
                let event = ExportEvent::Changed {
                    export: Box::new(self.running.snapshot()),
                };
                self.exports.announce(self.running.user_id, event);
                announced = Instant::now();
            }
        }
        self.close(file, &mut chunk, Outcome::Ready).await
    }

    /// Writes the rows of the last chunk, and waits until the file is
    /// written down. A failure to write turns `outcome` into a failure.
    async fn close(&self, file: &mut File, chunk: &mut String, outcome: Outcome) -> Outcome {
        let saved = async {
            self.save(file, chunk).await?;
            file.flush().await
        };
        match saved.await {
            Ok(()) => outcome,
            Err(error) => file_failure(&error),
        }
    }

    /// Writes the chunk to the file and empties it.
    async fn save(&self, file: &mut File, chunk: &mut String) -> std::io::Result<()> {
        file.write_all(chunk.as_bytes()).await?;
        let bytes = u64::try_from(chunk.len()).unwrap_or(u64::MAX);
        self.running.bytes.fetch_add(bytes, Ordering::Relaxed);
        chunk.clear();
        Ok(())
    }

    /// Gives up the rest of the result: the connection is closed on the
    /// server, which stops the query; the rows on their way stay unread.
    async fn abandon(&self, outcome: Outcome) -> Outcome {
        self.running.interrupt().await;
        outcome
    }

    /// Outcome of a failure of the query: stopped by the user (the closing
    /// of its connection makes the query fail), or failed.
    fn failure(&self, error: &MysqlError) -> Outcome {
        if self.running.is_cancelled() {
            Outcome::Cancelled
        } else {
            Outcome::Failed(ErrorDetail::of_mysql(error, Some(self.flavor)))
        }
    }
}

/// Outcome of a failure to write the file.
fn file_failure(error: &std::io::Error) -> Outcome {
    Outcome::Failed(ErrorDetail::ExportFile {
        cause: error.to_string(),
    })
}

/// Index in the result of each exported column: `columns`, checked against
/// the `names` of the result, or all the columns when empty. Refuses an
/// index beyond the result, and columns of the same name, which could not
/// be told apart in the file.
fn column_order(columns: &[usize], names: &[String]) -> Result<Vec<usize>, ErrorDetail> {
    let order: Vec<usize> = if columns.is_empty() {
        (0..names.len()).collect()
    } else {
        columns.to_vec()
    };
    if let Some(index) = order.iter().find(|&&index| index >= names.len()) {
        return Err(ErrorDetail::InvalidMessage {
            cause: format!(
                "column {index} requested, the result has {} columns",
                names.len()
            ),
        });
    }
    let mut seen = HashSet::new();
    let mut duplicates = Vec::new();
    for &index in &order {
        let name = &names[index];
        if !seen.insert(name) && !duplicates.contains(name) {
            duplicates.push(name.clone());
        }
    }
    if duplicates.is_empty() {
        Ok(order)
    } else {
        Err(ErrorDetail::DuplicateColumns { names: duplicates })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(names: &[&str]) -> Vec<String> {
        names.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn orders_the_columns_as_displayed() {
        let result = names(&["id", "name", "price"]);
        assert_eq!(column_order(&[], &result), Ok(vec![0, 1, 2]));
        assert_eq!(column_order(&[2, 0], &result), Ok(vec![2, 0]));
        assert!(matches!(
            column_order(&[3], &result),
            Err(ErrorDetail::InvalidMessage { .. })
        ));
    }

    #[test]
    fn refuses_columns_of_the_same_name() {
        let result = names(&["id", "name", "id", "name", "id"]);
        assert_eq!(
            column_order(&[], &result),
            Err(ErrorDetail::DuplicateColumns {
                names: names(&["id", "name"])
            })
        );
        // The columns left out do not count.
        assert_eq!(column_order(&[1, 2], &result), Ok(vec![1, 2]));
    }
}
