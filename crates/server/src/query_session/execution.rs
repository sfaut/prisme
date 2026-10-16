//! Execution of the queries of a session: one query on the session's
//! connection, or the queries of a batch in parallel, each on its own
//! connection, with their rows streamed to the client.

use std::time::Instant;

use futures_util::future::join_all;
use prisme_connectors::mysql::{self, Connection};
use prisme_dialects::Dialect;
use prisme_dialects::limit::{LimitOutcome, apply_default_limit, written_limit};
use prisme_splitter::{Statement, statement_at};
use tokio::sync::Semaphore;

use super::limits::{report, write_refusal};
use super::outbox::SessionStopped;
use super::protocol::ServerMessage;
use super::session::{Session, Summary};

/// Number of rows per `rows` message.
const ROW_BATCH_SIZE: usize = 500;

/// Maximum number of queries run at the same time (one connection each).
const MAX_PARALLEL: usize = 8;

/// Error message of the queries stopped by the user.
const CANCELLED: &str = "query interrupted by the user";

impl Session<'_> {
    /// Runs a query on the session's connection.
    pub(super) async fn run_on_session(
        &self,
        connection: &mut Connection,
        dialect: Dialect,
        sql: &str,
        max_rows: usize,
    ) -> Result<(), SessionStopped> {
        self.outbox
            .send(ServerMessage::Started { query: 0 })
            .await?;
        let Some(summary) = self
            .run_query(connection, dialect, 0, sql, max_rows)
            .await?
        else {
            return Ok(());
        };
        let database = match mysql::current_database(connection).await {
            Ok(database) => database,
            Err(error) => return self.outbox.error(Some(0), error.to_string()).await,
        };
        self.done(0, summary, database).await
    }

    /// Runs all the queries in parallel, `MAX_PARALLEL` at most at a time, each
    /// on its own connection opened on the database `database`.
    pub(super) async fn run_in_parallel(
        &self,
        statements: &[&Statement],
        dialect: Dialect,
        max_rows: usize,
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
                max_rows,
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
        max_rows: usize,
        database: &Option<String>,
    ) -> Result<(), SessionStopped> {
        let _permit = permits.acquire().await.map_err(|_| SessionStopped)?;
        if self.cancellation.is_cancelled(query) {
            return self.outbox.error(Some(query), CANCELLED.to_string()).await;
        }
        self.outbox.send(ServerMessage::Started { query }).await?;
        self.run_on_new_connection(dialect, query, sql, max_rows, database)
            .await
    }

    /// Runs a query on a new connection, placed on the database `database`.
    pub(super) async fn run_on_new_connection(
        &self,
        dialect: Dialect,
        query: u32,
        sql: &str,
        max_rows: usize,
        database: &Option<String>,
    ) -> Result<(), SessionStopped> {
        let mut connection = match mysql::connect(self.target).await {
            Ok(connection) => connection,
            Err(error) => return self.outbox.error(Some(query), error.to_string()).await,
        };
        let result = self
            .run_on_connection(&mut connection, dialect, query, sql, max_rows, database)
            .await;
        let _ = connection.disconnect().await;
        result
    }

    /// Places the connection on the database `database`, then runs the query on it.
    pub(super) async fn run_on_connection(
        &self,
        connection: &mut Connection,
        dialect: Dialect,
        query: u32,
        sql: &str,
        max_rows: usize,
        database: &Option<String>,
    ) -> Result<(), SessionStopped> {
        if let Some(name) = database
            && let Err(error) = mysql::use_database(connection, name).await
        {
            return self.outbox.error(Some(query), error.to_string()).await;
        }
        match self
            .run_query(connection, dialect, query, sql, max_rows)
            .await?
        {
            Some(summary) => self.done(query, summary, database.clone()).await,
            None => Ok(()),
        }
    }

    /// Runs a query and streams its columns and its rows.
    ///
    /// Returns the summary of the result, or `None` if the query failed (the
    /// error has then already gone to the client).
    pub(super) async fn run_query(
        &self,
        connection: &mut Connection,
        dialect: Dialect,
        query: u32,
        sql: &str,
        max_rows: usize,
    ) -> Result<Option<Summary>, SessionStopped> {
        if let Some(refusal) = write_refusal(sql, dialect) {
            self.outbox.error(Some(query), refusal).await?;
            return Ok(None);
        }
        let limited = match apply_default_limit(sql, dialect, max_rows as u64) {
            Ok(limited) => limited,
            Err(error) => {
                self.outbox.error(Some(query), error.to_string()).await?;
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
        let _running = self.cancellation.track(query, connection_id);
        // Stop requested while the connection was opening: the query does not start.
        if self.cancellation.is_cancelled(query) {
            self.outbox
                .error(Some(query), CANCELLED.to_string())
                .await?;
            return Ok(None);
        }
        let executing = Instant::now();
        let mut stream = match mysql::execute(connection, &limited.sql).await {
            Ok(stream) => stream,
            Err(error) => {
                self.outbox
                    .error(Some(query), self.failure(query, error))
                    .await?;
                return Ok(None);
            }
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
                execution_us: u64::try_from(executing.elapsed().as_micros()).unwrap_or(u64::MAX),
            })
            .await?;

        let mut batch = Vec::with_capacity(ROW_BATCH_SIZE);
        let mut row_count = 0;
        let mut truncated = false;
        loop {
            let row = match stream.next_row().await {
                Ok(Some(row)) => row,
                Ok(None) => break,
                Err(error) => {
                    stream.discard().await;
                    self.outbox
                        .error(Some(query), self.failure(query, error))
                        .await?;
                    return Ok(None);
                }
            };
            if self.cancellation.is_cancelled(query) {
                // The server has already received KILL QUERY: reading stops.
                stream.discard().await;
                self.outbox
                    .error(Some(query), CANCELLED.to_string())
                    .await?;
                return Ok(None);
            }
            if capped && row_count == max_rows {
                // Row N + 1: the result is truncated, no need to read further.
                truncated = true;
                if limited.outcome == LimitOutcome::Unparsed {
                    // Without LIMIT, the server would keep sending rows: it is interrupted.
                    if let Err(error) = mysql::kill_query(self.target, connection_id).await {
                        tracing::warn!("cannot interrupt the query: {error}");
                    }
                }
                break;
            }
            batch.push(row);
            row_count += 1;
            if batch.len() == ROW_BATCH_SIZE {
                let rows = std::mem::replace(&mut batch, Vec::with_capacity(ROW_BATCH_SIZE));
                self.outbox
                    .send(ServerMessage::Rows { query, rows })
                    .await?;
            }
        }
        if !batch.is_empty() {
            self.outbox
                .send(ServerMessage::Rows { query, rows: batch })
                .await?;
        }

        let affected_rows = stream.affected_rows();
        stream.discard().await;
        Ok(Some(Summary {
            row_count,
            truncated,
            affected_rows,
            elapsed_ms: started.elapsed().as_millis(),
        }))
    }

    /// Message of a failed query: "interrupted" if the stop was requested (the
    /// server's error is then that of `KILL QUERY`).
    pub(super) fn failure(&self, query: u32, error: mysql::MysqlError) -> String {
        if self.cancellation.is_cancelled(query) {
            CANCELLED.to_string()
        } else {
            error.to_string()
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

/// Queries to run: the one under the cursor, or all of them without a cursor.
pub(super) fn chosen_statements(
    statements: &[Statement],
    cursor: Option<usize>,
) -> Vec<&Statement> {
    match cursor {
        Some(cursor) => statement_at(statements, cursor)
            .map(|index| vec![&statements[index]])
            .unwrap_or_default(),
        None => statements.iter().collect(),
    }
}
