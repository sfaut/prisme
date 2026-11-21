//! Display settings of the user, common to all the editors: the theme of the
//! interface, the active editor, the tab and the filters of the console, the
//! wrapping of the data explorer. They follow the user from one browser to
//! the other, and survive a page reload.
//!
//! The theme has its own column (`users.theme`); the rest is stored as JSON
//! in `users.display`, empty as long as the user has saved nothing: the
//! defaults below then apply, as for a field missing from a document saved
//! by an older version. The page reads the whole display at startup
//! (`GET /api/display`) and sends it whole on each change
//! (`PUT /api/display`). The schema is documented in `docs/DEVELOPER.md`,
//! section "Saved display".
//!
//! Synchronous functions (`rusqlite`), called from blocking tasks.

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::api::AppState;
use crate::api_error::ApiError;
use crate::auth::CurrentUser;

/// Errors reading or saving the display of a user.
#[derive(Debug, thiserror::Error)]
pub enum UserDisplayError {
    /// Error of the internal database.
    #[error("internal database: {0}")]
    Sqlite(#[from] rusqlite::Error),

    /// Unreadable saved display.
    #[error("unreadable saved display: {0}")]
    Json(#[from] serde_json::Error),
}

/// Theme of the interface.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum Theme {
    /// Follows the setting of the machine.
    #[default]
    System,
    /// Light, whatever the machine.
    Light,
    /// Dark, whatever the machine.
    Dark,
}

impl Theme {
    /// Value of the theme in the database, as in JSON.
    fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }

    /// Theme of a value of the database; the system's for an unknown one.
    fn parse(value: &str) -> Self {
        match value {
            "light" => Self::Light,
            "dark" => Self::Dark,
            _ => Self::System,
        }
    }
}

/// Tab of the console.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum ConsoleTab {
    /// History of the queries.
    #[default]
    History,
    /// Debug log.
    Debug,
}

/// Display settings of the user.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct UserDisplay {
    /// Theme of the interface, `system` by default.
    pub theme: Theme,
    /// Id of the active editor; `null`, or an editor no longer open: the
    /// first one.
    pub active_editor: Option<i64>,
    /// Console, common to all the editors.
    pub console: ConsoleDisplay,
    /// Data explorers, common to all the editors.
    pub explorer: ExplorerDisplay,
}

/// Display of the console, common to all the editors.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct ConsoleDisplay {
    /// Tab shown, the history by default.
    pub tab: ConsoleTab,
    /// Do the lists show the elements of all the editors? Off by default:
    /// those of the editor on screen.
    pub all_editors: bool,
    /// Are the buttons of the filters shown? Off by default.
    pub filters_open: bool,
    /// Filter of the history.
    pub history_filter: ConsoleHistoryFilter,
    /// Filter of the debug log.
    pub debug_filter: ConsoleDebugFilter,
}

/// Queries kept by the history of the console.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct ConsoleHistoryFilter {
    /// Actions kept; all of them when empty.
    pub action: Vec<String>,
}

/// Lines kept by the debug log of the console.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct ConsoleDebugFilter {
    /// Levels kept; all of them when empty.
    pub level: Vec<String>,
    /// Sources kept; all of them when empty.
    pub source: Vec<String>,
}

/// Display of the data explorers, common to all the editors.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct ExplorerDisplay {
    /// Are long lines wrapped to the width of the panel? Off by default.
    pub wrap: bool,
}

/// Part of the display stored as JSON in `users.display`: all but the
/// theme. A missing field takes its default.
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct StoredDisplay {
    active_editor: Option<i64>,
    console: ConsoleDisplay,
    explorer: ExplorerDisplay,
}

/// Display settings of the user `user_id`.
pub fn load(db: &Connection, user_id: i64) -> Result<UserDisplay, UserDisplayError> {
    let (theme, display): (String, String) = db.query_row(
        "SELECT theme, display FROM users WHERE id = ?1",
        params![user_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    // Empty string: nothing saved yet.
    let stored: StoredDisplay = if display.is_empty() {
        StoredDisplay::default()
    } else {
        serde_json::from_str(&display)?
    };
    Ok(UserDisplay {
        theme: Theme::parse(&theme),
        active_editor: stored.active_editor,
        console: stored.console,
        explorer: stored.explorer,
    })
}

/// Saves the display settings of the user `user_id`.
pub fn update(
    db: &Connection,
    user_id: i64,
    display: &UserDisplay,
) -> Result<(), UserDisplayError> {
    let stored = StoredDisplay {
        active_editor: display.active_editor,
        console: display.console.clone(),
        explorer: display.explorer.clone(),
    };
    db.execute(
        "UPDATE users SET theme = ?2, display = ?3 WHERE id = ?1",
        params![
            user_id,
            display.theme.as_str(),
            serde_json::to_string(&stored)?
        ],
    )?;
    Ok(())
}

/// Routes of the display settings of the user.
pub fn routes() -> Router<AppState> {
    Router::new().route("/api/display", get(get_display).put(put_display))
}

/// `GET /api/display`: display settings of the current user.
async fn get_display(
    State(state): State<AppState>,
    user: CurrentUser,
) -> Result<Json<UserDisplay>, ApiError> {
    let display = state.with_db(move |db, _| load(db, user.id)).await?;
    Ok(Json(display))
}

/// `PUT /api/display`: saves the display settings of the current user.
async fn put_display(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(display): Json<UserDisplay>,
) -> Result<StatusCode, ApiError> {
    state
        .with_db(move |db, _| update(db, user.id, &display))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store;

    #[test]
    fn gives_the_defaults_until_the_user_saves_a_display() {
        let db = store::open_in_memory();
        let user = store::ensure_user(&db, "owner@example.com").unwrap();
        assert_eq!(load(&db, user).unwrap(), UserDisplay::default());
    }

    #[test]
    fn saves_the_display_of_a_user_alone() {
        let db = store::open_in_memory();
        let user = store::ensure_user(&db, "owner@example.com").unwrap();
        let other = store::ensure_user(&db, "other@example.com").unwrap();
        let display = UserDisplay {
            theme: Theme::Dark,
            active_editor: Some(7),
            console: ConsoleDisplay {
                tab: ConsoleTab::Debug,
                all_editors: true,
                filters_open: true,
                history_filter: ConsoleHistoryFilter {
                    action: vec!["run".to_string()],
                },
                debug_filter: ConsoleDebugFilter {
                    level: vec!["error".to_string()],
                    source: vec![],
                },
            },
            explorer: ExplorerDisplay { wrap: true },
        };
        update(&db, user, &display).unwrap();
        assert_eq!(load(&db, user).unwrap(), display);
        assert_eq!(load(&db, other).unwrap(), UserDisplay::default());
        let theme: String = db
            .query_row(
                "SELECT theme FROM users WHERE id = ?1",
                params![user],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(theme, "dark");
    }

    #[test]
    fn reads_a_display_with_missing_fields() {
        let db = store::open_in_memory();
        let user = store::ensure_user(&db, "owner@example.com").unwrap();
        db.execute(
            r#"UPDATE users SET display = '{"console":{"tab":"debug"}}' WHERE id = ?1"#,
            params![user],
        )
        .unwrap();
        let display = load(&db, user).unwrap();
        assert_eq!(display.console.tab, ConsoleTab::Debug);
        assert!(!display.console.all_editors && !display.explorer.wrap);
    }

    #[test]
    fn refuses_an_unknown_theme_in_the_database() {
        let db = store::open_in_memory();
        let user = store::ensure_user(&db, "owner@example.com").unwrap();
        assert!(
            db.execute(
                "UPDATE users SET theme = 'blue' WHERE id = ?1",
                params![user]
            )
            .is_err()
        );
    }
}
