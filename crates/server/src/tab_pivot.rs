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

/// Elements of each zone, the outermost level first.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PivotConfig {
    /// Levels of the rows.
    pub rows: Vec<PivotLevel>,
    /// Levels of the columns.
    pub columns: Vec<PivotLevel>,
    /// Measures, in the order of their zone.
    pub measures: Vec<NamedMeasure>,
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
}
