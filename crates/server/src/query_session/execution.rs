//! Execution of the queries of a session: one query on the session's
//! connection, or the queries of a batch in parallel, each on its own
//! connection, opened on the session's current database, with their rows
//! streamed to the client in batches that follow their arrival
//! (`RowBatches`): a row received does not wait for others to show.

use std::task::Poll;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use futures_util::future::join_all;
use prisme_connectors::mysql::sql_log::{self, Sent};
use prisme_connectors::mysql::{self, Cell, Connection, MysqlTarget};
use prisme_dialects::Dialect;
use prisme_dialects::classify::{StatementKind, changes_database, classify};
use prisme_dialects::limit::{LimitOutcome, apply_default_limit, written_limit};
use prisme_dialects::temporary::TableName;
use prisme_splitter::{Statement, pieces_in, statement_at, statements_in};
use tokio::sync::Semaphore;
use tracing::Instrument;

use super::limits::{RunLimits, limit_error, report, write_refusal};
use super::outbox::SessionStopped;
use super::protocol::{SelectionRange, ServerMessage};
use super::session::{Session, Summary};
use crate::analyze::flavor_of;
use crate::console::{self, Action};
use crate::error_detail::ErrorDetail;

/// Weight of a batch at which it leaves, in bytes of its cells in JSON: the
/// page parses its message within a frame (about 8 ms for 1 MB of short
/// cells), whatever the width of the rows. A heavier row leaves alone.
const BATCH_BYTES: usize = 1_000_000;

/// Longest wait of the first row of the first batch of a result: the first
/// rows of a result that arrives without a pause show at once.
const FIRST_BATCH_WAIT: Duration = Duration::from_millis(20);

/// Longest wait of the first row of the next batches: a result that arrives
/// without a pause updates the page about as often as the page applies its
/// messages (every 100 ms).
const BATCH_WAIT: Duration = Duration::from_millis(100);

/// Maximum number of queries run at the same time (one connection each).
const MAX_PARALLEL: usize = 8;

impl Session<'_> {
    /// Runs the query `query` of the batch on the session's connection. The
    /// current database is read again only if the query can change it.
    ///
    /// Returns `false` if the query failed (the error has then already gone
    /// to the client).
    pub(super) async fn run_on_session(
        &self,
        connection: &mut Connection,
        dialect: Dialect,
        query: u32,
        sql: &str,
        limits: &RunLimits,
    ) -> Result<bool, SessionStopped> {
        self.outbox.send(ServerMessage::Started { query }).await?;
        let Some(summary) = self
            .run_query(connection, dialect, query, sql, limits)
            .await?
        else {
            return Ok(false);
        };
        let database = if changes_database(sql, dialect) {
            match mysql::current_database(connection).await {
                Ok(database) => {
                    self.remember_database(&database);
                    database
                }
                Err(error) => {
                    let detail = ErrorDetail::of_mysql(&error, Some(flavor_of(dialect)));
                    self.outbox.error(Some(query), detail).await?;
                    return Ok(false);
                }
            }
        } else {
            self.current_database()
        };
        self.done(query, summary, database).await?;
        Ok(true)
    }

    /// Runs the queries one after the other on the session's connection, in
    /// the order of the text: a batch that writes or sets the session. After a
    /// failure, the next queries do not run.
    pub(super) async fn run_in_sequence(
        &self,
        connection: &mut Connection,
        statements: &[&Statement],
        dialect: Dialect,
        limits: &RunLimits,
    ) -> Result<(), SessionStopped> {
        let mut failed = false;
        for (query, statement) in (0_u32..).zip(statements.iter()) {
            if failed {
                let detail = if self.is_cancelled(query) {
                    ErrorDetail::QueryInterrupted
                } else {
                    ErrorDetail::QuerySkipped
                };
                self.outbox.error(Some(query), detail).await?;
                continue;
            }
            failed = !self
                .run_on_session(connection, dialect, query, &statement.sql, limits)
                .await?;
        }
        Ok(())
    }

    /// Runs all the queries in parallel, `MAX_PARALLEL` at most at a time, each
    /// on its own connection opened on the database `database`.
    pub(super) async fn run_in_parallel(
        &self,
        statements: &[&Statement],
        dialect: Dialect,
        limits: &RunLimits,
        database: &Option<String>,
    ) -> Result<(), SessionStopped> {
        // All the queries start, but wait for a permit before opening their connection.
        let permits = Semaphore::new(MAX_PARALLEL);
        let mut runs = Vec::with_capacity(statements.len());
        for (query, statement) in (0_u32..).zip(statements.iter()) {
            runs.push(self.run_with_permit(
                &permits,
                dialect,
                query,
                &statement.sql,
                limits,
                database,
            ));
        }
        let results = join_all(runs).await;
        results.into_iter().collect()
    }

    /// Waits for a permit among `permits`, then runs the query on a new connection.
    pub(super) async fn run_with_permit(
        &self,
        permits: &Semaphore,
        dialect: Dialect,
        query: u32,
        sql: &str,
        limits: &RunLimits,
        database: &Option<String>,
    ) -> Result<(), SessionStopped> {
        let _permit = permits.acquire().await.map_err(|_| SessionStopped)?;
        if self.is_cancelled(query) {
            return self
                .outbox
                .error(Some(query), ErrorDetail::QueryInterrupted)
                .await;
        }
        self.outbox.send(ServerMessage::Started { query }).await?;
        self.run_on_new_connection(dialect, query, sql, limits, database)
            .await
    }

    /// Runs a query on a new connection, opened on the database `database`
    /// (none when absent), without a `USE` of its own.
    pub(super) async fn run_on_new_connection(
        &self,
        dialect: Dialect,
        query: u32,
        sql: &str,
        limits: &RunLimits,
        database: &Option<String>,
    ) -> Result<(), SessionStopped> {
        let target = MysqlTarget {
            database_name: database.clone().unwrap_or_default(),
            ..self.target.clone()
        };
        let mut connection = match mysql::connect(&target).await {
            Ok(connection) => connection,
            Err(error) => {
                let detail = ErrorDetail::of_mysql(&error, Some(flavor_of(dialect)));
                return self.outbox.error(Some(query), detail).await;
            }
        };
        let result = match self
            .run_query(&mut connection, dialect, query, sql, limits)
            .await
        {
            Ok(Some(summary)) => self.done(query, summary, database.clone()).await,
            Ok(None) => Ok(()),
            Err(stopped) => Err(stopped),
        };
        let _ = connection.disconnect().await;
        result
    }

    /// Runs a query and streams its columns and its rows, then traces it for
    /// the history of the queries (`sql_log`): its execution time, until the
    /// columns, and the transfer time of its rows.
    ///
    /// Returns the summary of the result, or `None` if the query failed (the
    /// error has then already gone to the client).
    pub(super) async fn run_query(
        &self,
        connection: &mut Connection,
        dialect: Dialect,
        query: u32,
        sql: &str,
        limits: &RunLimits,
    ) -> Result<Option<Summary>, SessionStopped> {
        let max_rows = limits.max_rows;
        let database = self.current_database();
        let temporary = |table: &TableName| self.is_temporary(table, database.as_deref());
        if let Some(refusal) = write_refusal(sql, dialect, limits.writes.as_ref(), &temporary) {
            self.outbox.error(Some(query), refusal).await?;
            return Ok(None);
        }
        let limited = match apply_default_limit(sql, dialect, max_rows as u64) {
            Ok(limited) => limited,
            Err(error) => {
                self.outbox.error(Some(query), limit_error(&error)).await?;
                return Ok(None);
            }
        };
        // Reading only stops by itself if Prisme set the limit.
        let capped = matches!(
            limited.outcome,
            LimitOutcome::Added | LimitOutcome::Unparsed
        );

        let started = Instant::now();
        let connection_id = connection.id();
        let _running = self.cancellation.track(self.batch, query, connection_id);
        // Stop requested while the connection was opening: the query does not start.
        if self.is_cancelled(query) {
            self.outbox
                .error(Some(query), ErrorDetail::QueryInterrupted)
                .await?;
            return Ok(None);
        }
        let executing = Instant::now();
        let executed = mysql::execute(connection, &limited.sql).await;
        // Only the session's connection writes: the parallel queries only read.
        self.follow_temporary(sql, dialect, database.as_deref(), executed.is_ok());
        let mut stream = match executed {
            Ok(stream) => stream,
            Err(error) => {
                let detail = self.failure(query, dialect, &error);
                sql_log::record(&Sent {
                    sql: &limited.sql,
                    duration: executing.elapsed(),
                    error: Some(&detail.to_string()),
                    ..Sent::default()
                });
                self.outbox.error(Some(query), detail).await?;
                return Ok(None);
            }
        };
        let execution = executing.elapsed();
        let transferring = Instant::now();
        let returns_rows = !stream.column_names().is_empty();
        let trace = |rows: usize, truncated: bool, affected_rows: u64, error: Option<&str>| {
            sql_log::record(&Sent {
                sql: &limited.sql,
                duration: execution,
                transfer: transferring.elapsed(),
                rows: returns_rows.then_some(rows as u64),
                affected_rows,
                truncated,
                error,
            });
        };
        self.outbox
            .send(ServerMessage::Columns {
                query,
                columns: stream.column_names(),
                right_aligned: stream.right_aligned_columns(),
                types: stream.column_types(),
                sql: limited.sql.clone(),
                limit: report(limited.outcome),
                expected_rows: match limited.outcome {
                    LimitOutcome::Added | LimitOutcome::Unparsed => Some(max_rows as u64),
                    LimitOutcome::UserLimit => written_limit(sql, dialect),
                    LimitOutcome::NotApplicable => None,
                },
                execution_us: u64::try_from(execution.as_micros()).unwrap_or(u64::MAX),
                executed_at: unix_millis(),
            })
            .await?;

        let mut batches = RowBatches::new();
        let mut row_count = 0;
        let mut truncated = false;
        loop {
            let read = {
                let next = stream.next_row();
                tokio::pin!(next);
                match futures_util::poll!(next.as_mut()) {
                    Poll::Ready(read) => read,
                    Poll::Pending => {
                        // The source pauses: the rows already read leave
                        // while the next one is awaited. The read stays the
                        // same future: interrupting it would cut a packet of
                        // the protocol.
                        if let Some(rows) = batches.take() {
                            self.outbox
                                .send(ServerMessage::Rows { query, rows })
                                .await?;
                        }
                        next.await
                    }
                }
            };
            let row = match read {
                Ok(Some(row)) => row,
                Ok(None) => break,
                Err(error) => {
                    stream.discard().await;
                    let detail = self.failure(query, dialect, &error);
                    trace(row_count, false, 0, Some(&detail.to_string()));
                    self.outbox.error(Some(query), detail).await?;
                    return Ok(None);
                }
            };
            if self.is_cancelled(query) {
                // The server has already received KILL QUERY: reading stops.
                stream.discard().await;
                let detail = ErrorDetail::QueryInterrupted;
                trace(row_count, false, 0, Some(&detail.to_string()));
                self.outbox.error(Some(query), detail).await?;
                return Ok(None);
            }
            if capped && row_count == max_rows {
                // Row N + 1: the result is truncated, no need to read further.
                truncated = true;
                if limited.outcome == LimitOutcome::Unparsed {
                    // Without LIMIT, the server would keep sending rows: it is interrupted.
                    let killing = mysql::kill_query(self.target, connection_id)
                        .instrument(console::action_span(Action::Cancel));
                    if let Err(error) = killing.await {
                        tracing::warn!("cannot interrupt the query: {error}");
                    }
                }
                break;
            }
            row_count += 1;
            if let Some(rows) = batches.push(row) {
                self.outbox
                    .send(ServerMessage::Rows { query, rows })
                    .await?;
            }
        }
        if let Some(rows) = batches.take() {
            self.outbox
                .send(ServerMessage::Rows { query, rows })
                .await?;
        }

        let affected_rows = stream.affected_rows();
        stream.discard().await;
        trace(row_count, truncated, affected_rows, None);
        Ok(Some(Summary {
            row_count,
            truncated,
            affected_rows,
            elapsed_ms: started.elapsed().as_millis(),
        }))
    }

    /// Has stopping the query `query` of this batch been requested?
    fn is_cancelled(&self, query: u32) -> bool {
        self.cancellation.is_cancelled(self.batch, query)
    }

    /// Detail of a failed query: "interrupted" if the stop was requested (the
    /// server's error is then that of `KILL QUERY`).
    pub(super) fn failure(
        &self,
        query: u32,
        dialect: Dialect,
        error: &mysql::MysqlError,
    ) -> ErrorDetail {
        if self.is_cancelled(query) {
            ErrorDetail::QueryInterrupted
        } else {
            ErrorDetail::of_mysql(error, Some(flavor_of(dialect)))
        }
    }

    /// Announces the end of a result.
    pub(super) async fn done(
        &self,
        query: u32,
        summary: Summary,
        database: Option<String>,
    ) -> Result<(), SessionStopped> {
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

/// Do the queries only read? They can then run in parallel, each on its own
/// connection. A write, a setting of the session, a control of the
/// transaction or a query Prisme cannot parse needs the session's connection
/// and the order of the text.
pub(super) fn reads_only(statements: &[&Statement], dialect: Dialect) -> bool {
    statements
        .iter()
        .all(|statement| classify(&statement.sql, dialect) == StatementKind::Read)
}

/// Rows of a result gathered into the batches of the `rows` messages. A
/// batch leaves at the first of these moments:
///
/// - the source pauses: the next row has not arrived yet (`take`, called by
///   the reading of the rows), so a result that arrives slowly shows as it
///   arrives;
/// - it weighs `BATCH_BYTES`: a message of heavy rows stays small;
/// - its first row has waited `FIRST_BATCH_WAIT` for the first batch of the
///   result, `BATCH_WAIT` for the next ones: a result that never pauses
///   shows its first rows at once, then updates regularly.
///
/// The number of rows of a batch thus follows the arrival of the rows: one
/// packet of the target server for a slow result, more rows when the client
/// reads slowly, since the rows then wait in the network buffer and the
/// source no longer pauses.
pub(super) struct RowBatches {
    /// Batch being filled.
    batch: Vec<Vec<Cell>>,
    /// Weight of the batch, in bytes of its cells in JSON.
    bytes: usize,
    /// Instant its first row was added; `None` while it is empty.
    opened_at: Option<Instant>,
    /// Has a batch of the result already left?
    sent: bool,
}

impl RowBatches {
    /// Starts the first batch of a result.
    pub(super) fn new() -> Self {
        Self {
            batch: Vec::new(),
            bytes: 0,
            opened_at: None,
            sent: false,
        }
    }

    /// Adds a row; returns the batch if it leaves now, by its weight or by
    /// the wait of its first row.
    pub(super) fn push(&mut self, row: Vec<Cell>) -> Option<Vec<Vec<Cell>>> {
        self.push_at(row, Instant::now())
    }

    /// Adds a row at the instant `now`; returns the batch if it leaves now,
    /// by its weight or by the wait of its first row.
    pub(super) fn push_at(&mut self, row: Vec<Cell>, now: Instant) -> Option<Vec<Vec<Cell>>> {
        let opened_at = *self.opened_at.get_or_insert(now);
        self.bytes += weight(&row);
        self.batch.push(row);
        let wait = if self.sent {
            BATCH_WAIT
        } else {
            FIRST_BATCH_WAIT
        };
        if self.bytes >= BATCH_BYTES || now.duration_since(opened_at) >= wait {
            self.take()
        } else {
            None
        }
    }

    /// Takes the batch, to send it: when the source pauses, or at the end of
    /// the result. `None` if it is empty.
    pub(super) fn take(&mut self) -> Option<Vec<Vec<Cell>>> {
        self.bytes = 0;
        self.opened_at = None;
        if self.batch.is_empty() {
            return None;
        }
        self.sent = true;
        Some(std::mem::take(&mut self.batch))
    }
}

/// Weight of a row in its message, in bytes: the text of its cells with the
/// quotes and separators of their JSON, `null` for NULL; escapes left out.
fn weight(row: &[Cell]) -> usize {
    let cells: usize = row
        .iter()
        .map(|cell| cell.as_ref().map_or(5, |text| text.len() + 3))
        .sum();
    cells + 2
}

/// Present instant, in milliseconds since the Unix epoch; 0 if the system clock
/// is set before it.
fn unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| {
            u64::try_from(since.as_millis()).unwrap_or(u64::MAX)
        })
}

/// Queries to run, in the order of the text: the one under the cursor; without
/// a cursor, those the selected passages touch; without either, all of them.
pub(super) fn chosen_statements<'a>(
    statements: &'a [Statement],
    cursor: Option<usize>,
    selection: &[SelectionRange],
) -> Vec<&'a Statement> {
    if let Some(cursor) = cursor {
        return statement_at(statements, cursor)
            .map(|index| vec![&statements[index]])
            .unwrap_or_default();
    }
    if selection.is_empty() {
        return statements.iter().collect();
    }
    let mut touched = vec![false; statements.len()];
    for range in selection {
        for index in statements_in(statements, range.start, range.end) {
            touched[index] = true;
        }
    }
    statements
        .iter()
        .zip(touched)
        .filter_map(|(statement, touched)| touched.then_some(statement))
        .collect()
}

/// Selected text only, as queries in the order of the text: of each query
/// the passages touch, the part inside them (`pieces_in`). Overlapping or
/// adjacent passages count as one; a query touched by two separate passages
/// gives two queries. Without a passage, none.
pub(super) fn selected_pieces(
    statements: &[Statement],
    selection: &[SelectionRange],
) -> Vec<Statement> {
    let mut ranges: Vec<(usize, usize)> = selection
        .iter()
        .map(|range| (range.start, range.end))
        .collect();
    ranges.sort_unstable();
    let mut merged: Vec<(usize, usize)> = Vec::with_capacity(ranges.len());
    for (start, end) in ranges {
        match merged.last_mut() {
            Some(last) if start <= last.1 => last.1 = last.1.max(end),
            _ => merged.push((start, end)),
        }
    }
    merged
        .into_iter()
        .flat_map(|(start, end)| pieces_in(statements, start, end))
        .collect()
}
