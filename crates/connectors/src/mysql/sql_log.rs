//! Trace of the SQL sent to the servers: one `tracing` event per statement,
//! under the target `TARGET`, at the `debug` level, with its text, its
//! durations and its outcome (`Sent`). The server turns these events into
//! the history of the queries shown in the interface's console; it adds the
//! context (user, connection, action) through spans of the same target.
//!
//! The connector traces the statements it sends itself, initialisation
//! queries, server identity, current database, `USE`, `KILL QUERY` and the
//! catalog; a result read in streaming (`execute`) is traced by its reader,
//! which alone knows its transfer time and its end (`record`).

use std::future::Future;
use std::time::{Duration, Instant};

/// Target of the trace events, and of the spans that give their context.
pub const TARGET: &str = "prisme_sql";

/// Statement sent to a server, and its outcome.
#[derive(Debug, Default)]
pub struct Sent<'a> {
    /// Text of the statement, as sent (parameters bound for display).
    pub sql: &'a str,
    /// Execution time: until the result, or, for a result read in
    /// streaming, until its columns.
    pub duration: Duration,
    /// Transfer time of the rows of a result read in streaming; zero when
    /// not measured.
    pub transfer: Duration,
    /// Number of rows read, `None` for a statement without a result set.
    pub rows: Option<u64>,
    /// Rows changed by a write.
    pub affected_rows: u64,
    /// Were rows beyond the limit dropped?
    pub truncated: bool,
    /// Failure message, `None` on success.
    pub error: Option<&'a str>,
}

/// Traces a statement sent to a server.
pub fn record(sent: &Sent<'_>) {
    tracing::debug!(
        target: TARGET,
        sql = sent.sql,
        duration_us = micros(sent.duration),
        transfer_us = micros(sent.transfer),
        returns_rows = sent.rows.is_some(),
        rows = sent.rows.unwrap_or(0),
        affected_rows = sent.affected_rows,
        truncated = sent.truncated,
        error = sent.error.unwrap_or(""),
    );
}

/// Runs a statement and traces it, with the number of rows that `rows_of`
/// counts in its result (`None` without a result set).
pub(crate) async fn traced<T, F>(
    sql: &str,
    query: F,
    rows_of: fn(&T) -> Option<u64>,
) -> Result<T, mysql_async::Error>
where
    F: Future<Output = Result<T, mysql_async::Error>>,
{
    let started = Instant::now();
    let result = query.await;
    let duration = started.elapsed();
    match &result {
        Ok(value) => record(&Sent {
            sql,
            duration,
            rows: rows_of(value),
            ..Sent::default()
        }),
        Err(error) => record(&Sent {
            sql,
            duration,
            error: Some(&error.to_string()),
            ..Sent::default()
        }),
    }
    result
}

/// Text of a prepared statement with its parameters in place of the `?`, as
/// quoted strings, for display only: the statement goes to the server with
/// its parameters apart.
pub(crate) fn bound(sql: &str, parameters: &[&str]) -> String {
    let mut parameters = parameters.iter();
    let mut text = String::with_capacity(sql.len());
    for character in sql.chars() {
        if character == '?'
            && let Some(parameter) = parameters.next()
        {
            text.push('\'');
            text.push_str(&parameter.replace('\'', "''"));
            text.push('\'');
        } else {
            text.push(character);
        }
    }
    text
}

/// Duration in microseconds, saturated.
fn micros(duration: Duration) -> u64 {
    u64::try_from(duration.as_micros()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binds_the_parameters_for_display() {
        assert_eq!(
            bound("SELECT a FROM t WHERE s = ? AND u = ?", &["x'y", "z"]),
            "SELECT a FROM t WHERE s = 'x''y' AND u = 'z'"
        );
        assert_eq!(bound("SELECT ?", &[]), "SELECT ?");
    }
}
