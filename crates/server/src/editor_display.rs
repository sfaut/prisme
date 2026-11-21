//! Display of an editor, saved with it: its panels (open or closed, their
//! size) and the tree of its object navigator (open nodes, scroll, "Tables"
//! folders sorted by row count). It survives a page reload, and the closing
//! of the editor followed by its reopening ("Rouvrir").
//!
//! The page makes the display and sends it whole on each change
//! (`PUT /api/editors/{id}/display`, `editors.rs`); the server stores it as
//! JSON in `editors.display` and gives it back with the open editors. The
//! page holds the defaults, used as long as an editor has saved none. The
//! schema is documented in `docs/DEVELOPER.md`, section "Saved display".

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Display of an editor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct EditorDisplay {
    /// Panels of the editor's tab.
    pub panels: Panels,
    /// Tree of the object navigator.
    pub navigator: NavigatorTree,
}

/// Panels of an editor's tab, each open or closed, with its size.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Panels {
    /// Object navigator, on the left; its size is a width.
    pub navigator: Panel,
    /// Data explorer, on the right; its size is a width.
    pub explorer: Panel,
    /// SQL editor, at the top of the view; it fills the height the others
    /// leave, without a size of its own.
    pub editor: Panel,
    /// Results, under the editor; its size is a height.
    pub results: Panel,
    /// Pivoteur, under the results; its size is a height.
    pub pivot: Panel,
    /// Console, at the bottom of the tab; its size is a height.
    pub console: Panel,
}

/// Panel of an editor's tab.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Panel {
    /// Is the panel open?
    pub open: bool,
    /// Size given by the user, in CSS pixels; `null`: the default size of
    /// the theme, or a share of the room.
    pub size: Option<f64>,
}

/// Tree of the object navigator of an editor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct NavigatorTree {
    /// Keys of the open nodes.
    pub expanded: Vec<String>,
    /// Position of the tree's scroll, in CSS pixels.
    pub scroll_top: f64,
    /// Keys of the "Tables" folders sorted by row count, the largest first;
    /// the other folders are sorted by name.
    pub tables_by_rows: Vec<String>,
}
