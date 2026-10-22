//! Row limit and write refusal of the queries, as the client sees them. The
//! rules are those of `docs/V1.md`, sections 2.3 and 2.4.

use std::collections::BTreeSet;

use prisme_dialects::Dialect;
use prisme_dialects::classify::{StatementKind, WriteKind, classify};
use prisme_dialects::limit::{LimitError, LimitOutcome};

use super::protocol::LimitReport;
use crate::api::AppState;
use crate::error_detail::ErrorDetail;
use crate::settings::{self, check_row_limit};

/// What the settings of the user let the queries of a request do, read when
/// the request arrives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RunLimits {
    /// Rows of a result shown at most: the requested limit, otherwise the
    /// user's setting.
    pub(super) max_rows: usize,
    /// Writes allowed, `None` while the sessions are read-only.
    pub(super) writes: Option<BTreeSet<WriteKind>>,
}

/// Limits of a request of user `user_id`, with its requested row limit.
pub(super) async fn run_limits(
    state: &AppState,
    user_id: i64,
    requested: Option<u32>,
) -> Result<RunLimits, ErrorDetail> {
    if let Some(row_limit) = requested {
        check_row_limit(row_limit).map_err(|error| error.detail())?;
    }
    let settings = state
        .with_db(move |db, _key| settings::load(db, user_id))
        .await
        .map_err(|_| ErrorDetail::SettingsUnreadable)?;
    Ok(RunLimits {
        max_rows: requested.unwrap_or(settings.row_limit) as usize,
        writes: (!settings.read_only_session)
            .then(|| settings.allowed_writes.into_iter().collect()),
    })
}

/// Returns the reason for the refusal if the query is neither a read nor a
/// write of the kinds `writes` allows (`None`: read-only sessions).
///
/// A write that no switch covers is refused, and so is a query that cannot
/// be parsed, as nothing proves what it does.
pub(super) fn write_refusal(
    sql: &str,
    dialect: Dialect,
    writes: Option<&BTreeSet<WriteKind>>,
) -> Option<ErrorDetail> {
    match classify(sql, dialect) {
        StatementKind::Read => None,
        StatementKind::Write(kinds) => {
            let Some(allowed) = writes else {
                return Some(ErrorDetail::WriteReadOnly);
            };
            let kinds: Vec<WriteKind> = kinds.difference(allowed).copied().collect();
            (!kinds.is_empty()).then_some(ErrorDetail::WriteNotAllowed { kinds })
        }
        StatementKind::Forbidden => Some(ErrorDetail::WriteForbidden),
        StatementKind::Unparsed => Some(ErrorDetail::UnparsedQueryRefused),
    }
}

/// Detail of a text that cannot run: no query, or several.
pub(super) fn limit_error(error: &LimitError) -> ErrorDetail {
    match error {
        LimitError::Empty => ErrorDetail::NoQuery,
        LimitError::MultipleStatements(count) => ErrorDetail::MultipleQueries {
            count: u32::try_from(*count).unwrap_or(u32::MAX),
        },
    }
}

/// Translates the handling of the limit for the client.
pub(super) fn report(outcome: LimitOutcome) -> LimitReport {
    match outcome {
        LimitOutcome::Added => LimitReport::Added,
        LimitOutcome::UserLimit => LimitReport::UserLimit,
        LimitOutcome::NotApplicable => LimitReport::NotApplicable,
        LimitOutcome::Unparsed => LimitReport::Unparsed,
    }
}
