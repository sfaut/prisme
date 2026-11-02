//! Configuration of the Pivoteur of a result tab, saved with the tab: it
//! survives a page reload, a refresh and a new run of the queries.
//!
//! The page makes the configuration, computes its table and fits it to the
//! columns of a new result (`web/src/components/pivotConfig.ts`). The server
//! stores it, and carries it over when a new run replaces the unpinned tabs of
//! an editor (`carry_over`): it alone sees the tabs that leave and those that
//! replace them at the same time, whichever queries of the text are run.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Aggregate of a measure of the Pivoteur.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum PivotAggregate {
    /// `COUNT(*)`.
    CountStar,
    /// `COUNT(col)`.
    Count,
    /// `COUNT(DISTINCT col)`.
    CountDistinct,
    /// `SUM(col)`.
    Sum,
    /// `SUM(DISTINCT col)`.
    SumDistinct,
    /// `AVG(col)`.
    Avg,
    /// `AVG(DISTINCT col)`.
    AvgDistinct,
    /// `MIN(col)`.
    Min,
    /// `MAX(col)`.
    Max,
}

/// What a level of the Pivoteur keeps of the dates of its column.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum DatePrecision {
    /// The values of the column, as they are.
    #[default]
    None,
    /// ISO year, that of the ISO week of the date: `2026`.
    Year,
    /// Quarter: `2026-T2`.
    Quarter,
    /// Month: `2026-01`.
    Month,
    /// ISO week: `2026-S53`.
    Week,
    /// Day, without the time: `2026-01-15`.
    Date,
}

/// Level of the rows or of the columns.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PivotLevel {
    /// Index of the column of the result.
    pub column: u32,
    /// Name given to the level, empty string without one: the level then shows
    /// its expression.
    pub name: String,
    /// What the level keeps of the dates of its column.
    #[serde(default)]
    pub precision: DatePrecision,
    /// Are the values of the level in descending order?
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[ts(as = "Option<bool>", optional)]
    pub descending: bool,
}

/// Measure of the configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct NamedMeasure {
    /// Aggregate.
    pub aggregate: PivotAggregate,
    /// Column the measure comes from; `COUNT(*)` keeps it, for its label and
    /// its menu.
    pub column: u32,
    /// Name given to the measure, empty string without one: the measure then
    /// shows its expression.
    pub name: String,
}

/// Comparison of a filter on a measure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum PivotComparison {
    /// `=`.
    Equal,
    /// `<>`.
    NotEqual,
    /// `<`.
    Less,
    /// `<=`.
    LessOrEqual,
    /// `>`.
    Greater,
    /// `>=`.
    GreaterOrEqual,
}

/// Filter of the Pivoteur.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export)]
pub enum PivotFilter {
    /// Keeps the rows whose cell is one of some values.
    Values {
        /// Index of the column of the result.
        column: u32,
        /// Name given to the filter, empty string without one: the filter then
        /// shows its expression.
        name: String,
        /// Values kept, `null` for NULL. Without any, the filter keeps every row.
        values: Vec<Option<String>>,
    },
    /// Keeps the rows of the groups whose measure passes a comparison.
    Measure {
        /// Aggregate of the measure.
        aggregate: PivotAggregate,
        /// Column the measure comes from; `COUNT(*)` keeps it, for its label and
        /// its menu.
        column: u32,
        /// Name given to the filter, empty string without one: the filter then
        /// shows its expression.
        name: String,
        /// Comparison of the value of each group to the operand.
        comparison: PivotComparison,
        /// Value compared to, empty string without one: the filter then keeps
        /// every row.
        operand: String,
        /// Column of the level the measure reads. A column that is no level any
        /// more, or none, means the deepest level of the rows.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        level: Option<u32>,
        /// Precision of that level: a column can be a level several times.
        #[serde(default)]
        level_precision: DatePrecision,
    },
}

/// Values of a node of the other axis, which rank the nodes of an axis: named
/// by its values, so that it survives a new computation of the table. A node
/// the table no longer holds ranks nothing, and ranks again once it comes back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PivotValueSort {
    /// Values of the levels of the node, the outermost first, `null` for NULL.
    /// A shorter path reads a subtotal, an empty one the grand total.
    pub path: Vec<Option<String>>,
    /// Index of the measure.
    pub measure: u32,
    /// Is the order descending?
    pub descending: bool,
}

/// Elements of each zone, the outermost level first, and the sorts by values.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PivotConfig {
    /// Levels of the rows.
    pub rows: Vec<PivotLevel>,
    /// Levels of the columns.
    pub columns: Vec<PivotLevel>,
    /// Measures, in the order of their zone.
    pub measures: Vec<NamedMeasure>,
    /// Filters, in the order of their zone.
    #[serde(default)]
    pub filters: Vec<PivotFilter>,
    /// Column of values that ranks the lines.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub sort_lines: Option<PivotValueSort>,
    /// Line whose values rank the columns, by their first measure.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub sort_columns: Option<PivotValueSort>,
}

/// Configuration of the Pivoteur of a result tab.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TabPivot {
    /// Names of the columns of the result the configuration was made for: its
    /// indexes read them, and a new result is compared to them.
    #[serde(default)]
    pub fields: Vec<String>,
    /// Elements of the zones.
    #[serde(default)]
    pub config: PivotConfig,
}

/// Result tab that a new run replaces.
#[derive(Debug)]
pub struct ReplacedTab {
    /// Number of the query in its batch.
    pub query: u32,
    /// Text of the query.
    pub sql: String,
    /// Configuration of the Pivoteur of the tab.
    pub pivot: TabPivot,
}

/// Does a replaced tab give its configuration to a query, by number and text?
type Rule = fn(&ReplacedTab, &(u32, String)) -> bool;

/// Configuration of the new tab of each query: that of the tab it replaces,
/// which is the tab of the same text at the same rank, else of the same text
/// (a single query of the text run alone), else of the same rank (a query
/// changed). A replaced tab gives its configuration once.
///
/// * `replaced`: tabs replaced.
/// * `queries`: number and text of each query.
pub fn carry_over(mut replaced: Vec<ReplacedTab>, queries: &[(u32, String)]) -> Vec<TabPivot> {
    let rules: [Rule; 3] = [
        |tab, (query, sql)| tab.query == *query && tab.sql == *sql,
        |tab, (_, sql)| tab.sql == *sql,
        |tab, (query, _)| tab.query == *query,
    ];
    let mut pivots: Vec<Option<TabPivot>> = vec![None; queries.len()];
    for rule in rules {
        for (pivot, query) in pivots.iter_mut().zip(queries) {
            if pivot.is_none()
                && let Some(found) = replaced.iter().position(|tab| rule(tab, query))
            {
                *pivot = Some(replaced.remove(found).pivot);
            }
        }
    }
    pivots.into_iter().map(Option::unwrap_or_default).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Configuration told apart by the name of its first field.
    fn pivot(field: &str) -> TabPivot {
        TabPivot {
            fields: vec![field.to_string()],
            config: PivotConfig::default(),
        }
    }

    /// Tab replaced, with the configuration `field`.
    fn tab(query: u32, sql: &str, field: &str) -> ReplacedTab {
        ReplacedTab {
            query,
            sql: sql.to_string(),
            pivot: pivot(field),
        }
    }

    /// First field of each configuration carried over, empty without one.
    fn carried(replaced: Vec<ReplacedTab>, queries: &[(u32, &str)]) -> Vec<String> {
        let queries: Vec<_> = queries.iter().map(|(q, s)| (*q, s.to_string())).collect();
        carry_over(replaced, &queries)
            .into_iter()
            .map(|pivot| pivot.fields.first().cloned().unwrap_or_default())
            .collect()
    }

    #[test]
    fn a_run_of_the_same_queries_keeps_each_configuration() {
        assert_eq!(
            carried(
                vec![tab(0, "a", "A"), tab(1, "b", "B")],
                &[(0, "a"), (1, "b")]
            ),
            ["A", "B"]
        );
    }

    #[test]
    fn a_query_run_alone_finds_the_configuration_of_its_text() {
        assert_eq!(
            carried(
                vec![tab(0, "a", "A"), tab(1, "b", "B"), tab(2, "c", "C")],
                &[(0, "b")]
            ),
            ["B"]
        );
    }

    #[test]
    fn a_changed_query_takes_the_configuration_of_its_rank() {
        assert_eq!(
            carried(
                vec![tab(0, "a", "A"), tab(1, "b", "B")],
                &[(0, "a"), (1, "b WHERE x")]
            ),
            ["A", "B"]
        );
    }

    #[test]
    fn a_text_goes_first_to_the_query_of_its_rank_and_gives_its_configuration_once() {
        assert_eq!(
            carried(
                vec![tab(0, "a", "A"), tab(1, "a", "B")],
                &[(1, "a"), (0, "a"), (2, "a")]
            ),
            ["B", "A", ""]
        );
    }

    #[test]
    fn a_new_query_starts_empty() {
        assert_eq!(carried(Vec::new(), &[(0, "a")]), [""]);
    }

    #[test]
    fn reads_a_saved_configuration_as_the_page_writes_it() {
        let saved: TabPivot = serde_json::from_str(
            r#"{"fields":["x","y"],"config":{"rows":[{"column":1,"name":"Y"}],
                "columns":[{"column":0,"name":"","precision":"week"}],
                "measures":[{"aggregate":"count_star","column":0,"name":""}]}}"#,
        )
        .unwrap();
        assert_eq!(
            saved.config.measures[0].aggregate,
            PivotAggregate::CountStar
        );
        assert_eq!(saved.config.rows[0].name, "Y");
        assert_eq!(saved.config.rows[0].precision, DatePrecision::None);
        assert_eq!(saved.config.columns[0].precision, DatePrecision::Week);
        assert_eq!(
            serde_json::from_str::<TabPivot>("{}").unwrap(),
            TabPivot::default()
        );
    }

    #[test]
    fn a_configuration_saved_before_the_sorts_and_the_filters_keeps_its_elements() {
        let saved =
            r#"{"rows":[{"column":1,"name":"","precision":"none"}],"columns":[],"measures":[]}"#;
        let config: PivotConfig = serde_json::from_str(saved).unwrap();
        assert!(!config.rows[0].descending);
        assert_eq!(config.sort_lines, None);
        assert_eq!(config.sort_columns, None);
        assert_eq!(config.filters, []);
        assert_eq!(
            serde_json::to_string(&config).unwrap(),
            saved.replace(r#""measures":[]"#, r#""measures":[],"filters":[]"#)
        );
    }

    #[test]
    fn keeps_the_filters_of_a_configuration() {
        let saved = r#"{"rows":[],"columns":[],"measures":[],"filters":[{"kind":"values","column":0,"name":"","values":["Nord",null]},{"kind":"measure","aggregate":"sum","column":2,"name":"Gros","comparison":"greater","operand":"100","level":1,"level_precision":"month"},{"kind":"measure","aggregate":"count_star","column":0,"name":"","comparison":"not_equal","operand":"","level_precision":"none"}]}"#;
        let config: PivotConfig = serde_json::from_str(saved).unwrap();
        assert_eq!(
            config.filters[0],
            PivotFilter::Values {
                column: 0,
                name: String::new(),
                values: vec![Some("Nord".to_string()), None],
            }
        );
        assert_eq!(
            config.filters[1],
            PivotFilter::Measure {
                aggregate: PivotAggregate::Sum,
                column: 2,
                name: "Gros".to_string(),
                comparison: PivotComparison::Greater,
                operand: "100".to_string(),
                level: Some(1),
                level_precision: DatePrecision::Month,
            }
        );
        assert_eq!(serde_json::to_string(&config).unwrap(), saved);
    }

    #[test]
    fn keeps_the_sorts_of_a_configuration() {
        let saved = r#"{"rows":[{"column":1,"name":"","precision":"none","descending":true}],"columns":[],"measures":[],"filters":[],"sort_lines":{"path":["2026",null],"measure":1,"descending":true},"sort_columns":{"path":[],"measure":0,"descending":false}}"#;
        let config: PivotConfig = serde_json::from_str(saved).unwrap();
        assert!(config.rows[0].descending);
        assert_eq!(
            config.sort_lines,
            Some(PivotValueSort {
                path: vec![Some("2026".to_string()), None],
                measure: 1,
                descending: true,
            })
        );
        assert_eq!(serde_json::to_string(&config).unwrap(), saved);
    }
}
