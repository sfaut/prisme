//! Display of the result grid of a tab, saved with the tab: the widths given
//! to its columns, their order, the sort, the user filter and its form, the
//! scroll. It survives a page reload, a refresh of the tab, and a new run of
//! the queries, which carries it over to the tab that replaces this one
//! (`tab_pivot::carry_over`).
//!
//! The page makes the display and sends it whole on each change
//! (`PUT /api/results/{id}/grid`, `editors.rs`); the server stores it as JSON
//! in `result_tabs.grid`. It applies only to a result with the same columns,
//! in the same order (`columns`): a query whose columns changed starts again
//! from the defaults of the grid. The schema is documented in
//! `docs/DEVELOPER.md`, section "Saved display".

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Display of the result grid of a tab.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct GridDisplay {
    /// Names of the columns of the result the display was made on, in the
    /// order of the `SELECT`.
    pub columns: Vec<String>,
    /// Width given to each column, by its index in the `SELECT`: a CSS
    /// length relative to the root; `null`: the width estimated from its
    /// values.
    pub widths: Vec<Option<String>>,
    /// Index in the `SELECT` of each column shown, from left to right;
    /// empty: the order of the `SELECT`.
    pub order: Vec<u32>,
    /// Sort of the rows; `null`: the order of the result.
    pub sort: Option<GridSort>,
    /// User filter of the rows.
    pub filter: GridFilter,
    /// Is the form of the user filter open?
    pub filter_open: bool,
    /// Position of the vertical scroll, in CSS pixels.
    pub scroll_top: f64,
    /// Position of the horizontal scroll, in CSS pixels.
    pub scroll_left: f64,
}

/// Sort of the rows of a grid.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct GridSort {
    /// Index in the `SELECT` of the sorted column.
    pub column: u32,
    /// Direction of the sort.
    pub direction: SortDirection,
}

/// Direction of a sort.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum SortDirection {
    /// Ascending.
    Asc,
    /// Descending.
    Desc,
}

/// User filter of the rows of a grid.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct GridFilter {
    /// Searched text, case-insensitive; empty: no filter.
    pub text: String,
    /// Index in the `SELECT` of the targeted column; `null`: all of them.
    pub column: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_display_with_missing_fields() {
        let display: GridDisplay =
            serde_json::from_str(r#"{"columns":["a"],"sort":{"column":0,"direction":"desc"}}"#)
                .unwrap();
        assert_eq!(display.columns, ["a"]);
        assert_eq!(
            display.sort,
            Some(GridSort {
                column: 0,
                direction: SortDirection::Desc
            })
        );
        assert!(display.widths.is_empty() && !display.filter_open);
    }
}
