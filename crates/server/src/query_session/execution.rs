//! Execution of the queries of a session: one query on the session's
//! connection, or the queries of a batch in parallel, each on its own
//! connection, opened on the session's current database, with their rows
//! streamed to the client: a short first batch, shown without waiting for a
//! full one, then full batches.

use std::time::{Instant, SystemTime, UNIX_EPOCH};

use futures_util::future::join_all;
use prisme_connectors::mysql::sql_log::{self, Sent};
use prisme_connectors::mysql::{self, Cell, Connection, MysqlTarget};
use prisme_dialects::Dialect;
use prisme_dialects::classify::{StatementKind, changes_database, classify};
use prisme_dialects::limit::{LimitOutcome, apply_default_limit, written_limit};
use prisme_splitter::{Statement, statement_at, statements_in};
use tokio::sync::Semaphore;
use tracing::Instrument;

use super::limits::{RunLimits, limit_error, report, write_refusal};
use super::outbox::SessionStopped;
use super::protocol::{SelectionRange, ServerMessage};
use super::session::{Session, Summary};
use crate::analyze::flavor_of;
use crate::console::{self, Action};
use crate::error_detail::ErrorDetail;

/// Number of rows of the first `rows` message of a result: the first rows
/// show quickly, without waiting for a full batch.
const FIRST_BATCH_SIZE: usize = 50;

/// Number of rows of the next `rows` messages.
const ROW_BATCH_SIZE: usize = 500;

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
        if let Some(refusal) = write_refusal(sql, dialect, limits.writes.as_ref()) {
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
        let mut stream = match mysql::execute(connection, &limited.sql).await {
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
            let row = match stream.next_row().await {
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
        if let Some(rows) = batches.rest() {
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
/// connection. A write, a setting of the session or a query Prisme cannot
/// parse needs the session's connection and the order of the text.
pub(super) fn reads_only(statements: &[&Statement], dialect: Dialect) -> bool {
    statements
        .iter()
        .all(|statement| classify(&statement.sql, dialect) == StatementKind::Read)
}

/// Rows of a result gathered into the batches of the `rows` messages: the
/// first one holds `FIRST_BATCH_SIZE` rows, the next ones `ROW_BATCH_SIZE`.
pub(super) struct RowBatches {
    /// Batch being filled.
    batch: Vec<Vec<Cell>>,
    /// Number of rows that fills it.
    size: usize,
}

impl RowBatches {
    /// Starts the first batch.
    pub(super) fn new() -> Self {
        Self {
            batch: Vec::with_capacity(FIRST_BATCH_SIZE),
            size: FIRST_BATCH_SIZE,
        }
    }

    /// Adds a row; returns the batch once it is full, to send.
    pub(super) fn push(&mut self, row: Vec<Cell>) -> Option<Vec<Vec<Cell>>> {
        self.batch.push(row);
        if self.batch.len() < self.size {
            return None;
        }
        self.size = ROW_BATCH_SIZE;
        Some(std::mem::replace(
            &mut self.batch,
            Vec::with_capacity(ROW_BATCH_SIZE),
        ))
    }

    /// Last batch, not full, at the end of the result; `None` if it is empty.
    pub(super) fn rest(self) -> Option<Vec<Vec<Cell>>> {
        (!self.batch.is_empty()).then_some(self.batch)
    }
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
