//! State of the object navigator of an editor, saved with the editor: the
//! panel open or folded and its width, the open nodes and the scroll. It
//! survives a page reload, and the closing of the editor followed by its
//! reopening ("Rouvrir").
//!
//! The page makes the state and sends it whole on each change
//! (`PUT /api/editors/{id}/navigator`, `editors.rs`); the server stores it as
//! JSON in `editors.navigator` and gives it back with the open editors.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// State of the object navigator of an editor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct NavigatorState {
    /// Is the panel open?
    pub open: bool,
    /// Width given by the user, in CSS pixels; `null`: the default width of
    /// the theme.
    pub width: Option<f64>,
    /// Keys of the open nodes of the tree.
    pub expanded: Vec<String>,
    /// Position of the tree's scroll, in CSS pixels.
    pub scroll_top: f64,
}
