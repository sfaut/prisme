//! Row limit and write refusal of the queries, as the client sees them. The
//! rules are those of `docs/V1.md`, sections 2.3 and 2.4.

use prisme_dialects::Dialect;
use prisme_dialects::classify::{StatementKind, classify};
use prisme_dialects::limit::LimitOutcome;

use super::protocol::LimitReport;
use crate::api::AppState;
use crate::settings::{self, check_row_limit};

/// Row limit of a request: the requested one, otherwise the user's setting.
pub(super) async fn row_limit(
    state: &AppState,
    user_id: i64,
    requested: Option<u32>,
) -> Result<u32, String> {
    if let Some(row_limit) = requested {
        return check_row_limit(row_limit).map_err(|error| error.to_string());
    }
    state
        .with_db(move |db, _key| settings::load(db, user_id))
        .await
        .map(|settings| settings.row_limit)
        .map_err(|_| "unreadable user settings, see the server log".to_string())
}

/// Returns the reason for the refusal if the query is not a read.
///
/// Writes stay refused as long as the write switches (stage 4 of the v1) do
/// not exist. A query that cannot be parsed is refused too, as nothing proves
/// it is a read.
pub(super) fn write_refusal(sql: &str, dialect: Dialect) -> Option<String> {
    match classify(sql, dialect) {
        StatementKind::Read => None,
        StatementKind::Write => {
            Some("write refused: writes are disabled until the write switches exist".to_string())
        }
        StatementKind::Unparsed => Some(
            "query refused: Prisme cannot parse it, so it cannot check that it is a read"
                .to_string(),
        ),
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
