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
    /// `GROUP_CONCAT(col)`, of MariaDB, with its settings (`PivotConcat`).
    GroupConcat,
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
    /// Hour of an instant, its minutes and seconds at 0: `2026-01-15 10:00:00`.
    Hour,
    /// Minute of an instant, its seconds at 0: `2026-01-15 10:30:00`.
    Minute,
    /// Second of an instant, without its fraction: `2026-01-15 10:30:15`.
    Second,
    /// Millisecond of an instant: `2026-01-15 10:30:15.123`.
    Millisecond,
    /// Microsecond of an instant: `2026-01-15 10:30:15.123456`.
    Microsecond,
}

/// Form of the lines of the Pivoteur.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum PivotForm {
    /// One column of labels, indented: a group has a line of its own, above its
    /// members, with its subtotal.
    #[default]
    Compact,
    /// One column of labels per level: a group has a line of its own, above its
    /// members, with its subtotal.
    Outline,
    /// One column of labels per level: a group starts on the line of its first
    /// member, and its subtotal has a line of its own, below its members.
    Tabular,
}

/// Is this the form of a configuration that does not name one?
fn is_compact(form: &PivotForm) -> bool {
    *form == PivotForm::Compact
}

/// Does a measure read the values of its column as they are, without a
/// precision of their dates?
fn is_whole(precision: &DatePrecision) -> bool {
    *precision == DatePrecision::None
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
    /// Does the level leave its subtotal out? The setting follows the level
    /// wherever it goes; the last level of an axis has no subtotal.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[ts(as = "Option<bool>", optional)]
    pub without_subtotal: bool,
    /// Width of its column of labels, when the lines give one per level, in
    /// steps of the spacing scale (`--spacing`); the default width without
    /// one. The width follows the level wherever it goes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub width: Option<u32>,
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
    /// What `COUNT(DISTINCT col)`, `MIN(col)`, `MAX(col)` and
    /// `GROUP_CONCAT(col)` keep of the dates of their column: they then read
    /// its periods.
    #[serde(default, skip_serializing_if = "is_whole")]
    #[ts(as = "Option<DatePrecision>", optional)]
    pub precision: DatePrecision,
    /// Settings of a `GROUP_CONCAT(col)`, none for another aggregate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub concat: Option<PivotConcat>,
}

/// Settings of a `GROUP_CONCAT(col)`, as MariaDB writes them. The longest
/// text is a setting of the user (`concat_max_bytes`), the same for every
/// measure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PivotConcat {
    /// Is each value taken once (`DISTINCT`)?
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[ts(as = "Option<bool>", optional)]
    pub distinct: bool,
    /// Keys of the order of the values (`ORDER BY`), the first one first;
    /// without any, the values follow the order of the rows.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(as = "Option<Vec<PivotConcatKey>>", optional)]
    pub order: Vec<PivotConcatKey>,
    /// Text between two values (`SEPARATOR`).
    pub separator: String,
    /// Values skipped first (`OFFSET`).
    #[serde(default, skip_serializing_if = "is_zero")]
    #[ts(as = "Option<u32>", optional)]
    pub offset: u32,
    /// Values kept at most after them (`LIMIT`), all of them without one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub limit: Option<u32>,
}

/// Key of the order of the values of a `GROUP_CONCAT(col)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PivotConcatKey {
    /// Index of the column of the result.
    pub column: u32,
    /// Is the order descending?
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[ts(as = "Option<bool>", optional)]
    pub descending: bool,
}

/// Is this the `OFFSET` of a `GROUP_CONCAT` that names none?
fn is_zero(offset: &u32) -> bool {
    *offset == 0
}

/// What a filter on a measure asks of its value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum PivotTest {
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
    /// `BETWEEN`, its bounds included.
    Between,
    /// `NOT BETWEEN`.
    NotBetween,
    /// `IS NULL`.
    IsNull,
    /// `IS NOT NULL`.
    IsNotNull,
}

/// What a filter on a column asks of its value: a list of values, or the same
/// tests as a filter on a measure.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum PivotCondition {
    /// `IN`.
    #[default]
    InList,
    /// `NOT IN`.
    NotInList,
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
    /// `BETWEEN`, its bounds included.
    Between,
    /// `NOT BETWEEN`.
    NotBetween,
    /// `IS NULL`.
    IsNull,
    /// `IS NOT NULL`.
    IsNotNull,
}

/// Filter of the Pivoteur. A filter keeps the values typed under another
/// condition: changing it back gives them back. What it has not given yet reads
/// empty, which keeps a filter saved before its conditions (`kind` `values`,
/// field `comparison`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export)]
pub enum PivotFilter {
    /// Keeps the rows whose value of a column passes a condition.
    #[serde(alias = "values")]
    Column {
        /// Index of the column of the result.
        column: u32,
        /// Name given to the filter, empty string without one: the filter then
        /// shows its expression.
        name: String,
        /// What the filter keeps of the dates of its column.
        #[serde(default)]
        precision: DatePrecision,
        /// What the value of each row passes.
        #[serde(default)]
        condition: PivotCondition,
        /// Values of the list, `null` for NULL. Without any, the filter keeps
        /// every row.
        #[serde(default)]
        values: Vec<Option<String>>,
        /// Value compared to, or lower bound, empty string without one: the
        /// filter then keeps every row.
        #[serde(default)]
        operand: String,
        /// Upper bound, empty string without one: the filter then keeps every
        /// row.
        #[serde(default)]
        upper: String,
    },
    /// Keeps the rows of the groups whose measure passes a test.
    Measure {
        /// Aggregate of the measure.
        aggregate: PivotAggregate,
        /// Column the measure comes from; `COUNT(*)` keeps it, for its label and
        /// its menu.
        column: u32,
        /// Name given to the filter, empty string without one: the filter then
        /// shows its expression.
        name: String,
        /// What `COUNT(DISTINCT col)`, `MIN(col)` and `MAX(col)` keep of the
        /// dates of their column.
        #[serde(default, skip_serializing_if = "is_whole")]
        #[ts(as = "Option<DatePrecision>", optional)]
        precision: DatePrecision,
        /// What the value of each group passes.
        #[serde(alias = "comparison")]
        condition: PivotTest,
        /// Value compared to, or lower bound, empty string without one: the
        /// filter then keeps every row.
        operand: String,
        /// Upper bound, empty string without one: the filter then keeps every
        /// row.
        #[serde(default)]
        upper: String,
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

/// Width of a column of values, named by the values of its node and its
/// measure, as a sort names it: a column the table no longer shows keeps its
/// width, and finds it again once it comes back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PivotColumnWidth {
    /// Values of the levels of the node, the outermost first, `null` for NULL.
    /// A shorter path reads a subtotal, an empty one the grand total.
    pub path: Vec<Option<String>>,
    /// Index of the measure; 0 for the column of a node in a table without
    /// measures.
    pub measure: u32,
    /// Width, in steps of the spacing scale (`--spacing`).
    pub width: u32,
}

/// Elements of each zone, the outermost level first, the sorts by values, the
/// layout of the table and the widths of its columns.
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
    /// Form of the lines.
    #[serde(default, skip_serializing_if = "is_compact")]
    #[ts(as = "Option<PivotForm>", optional)]
    pub form: PivotForm,
    /// Does each line give the labels of its groups, where its form leaves them
    /// empty?
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[ts(as = "Option<bool>", optional)]
    pub repeat_labels: bool,
    /// Do the rows leave their grand total out?
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[ts(as = "Option<bool>", optional)]
    pub without_row_total: bool,
    /// Do the columns leave their grand total out?
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[ts(as = "Option<bool>", optional)]
    pub without_column_total: bool,
    /// Width of the labels of the lines when they stand in a single column,
    /// in steps of the spacing scale (`--spacing`); the default width without
    /// one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub labels_width: Option<u32>,
    /// Widths of the columns of values given one.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(as = "Option<Vec<PivotColumnWidth>>", optional)]
    pub column_widths: Vec<PivotColumnWidth>,
    /// Width of the section of the columns of the result, in steps of the
    /// spacing scale (`--spacing`); the default width without one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub fields_width: Option<u32>,
    /// Width of the section of the zones, in steps of the spacing scale; the
    /// default width without one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub zones_width: Option<u32>,
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
        let times = r#"["hour","minute","second","millisecond","microsecond"]"#;
        let precisions: Vec<DatePrecision> = serde_json::from_str(times).unwrap();
        assert_eq!(
            precisions,
            [
                DatePrecision::Hour,
                DatePrecision::Minute,
                DatePrecision::Second,
                DatePrecision::Millisecond,
                DatePrecision::Microsecond,
            ]
        );
        assert_eq!(serde_json::to_string(&precisions).unwrap(), times);
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
        assert_eq!(config.form, PivotForm::Compact);
        assert!(!config.repeat_labels);
        assert!(!config.without_row_total);
        assert!(!config.without_column_total);
        assert!(!config.rows[0].without_subtotal);
        assert_eq!(config.rows[0].width, None);
        assert_eq!(config.labels_width, None);
        assert_eq!(config.column_widths, []);
        assert_eq!(config.fields_width, None);
        assert_eq!(config.zones_width, None);
        assert_eq!(
            serde_json::to_string(&config).unwrap(),
            saved.replace(r#""measures":[]"#, r#""measures":[],"filters":[]"#)
        );
    }

    #[test]
    fn keeps_the_filters_of_a_configuration() {
        let saved = r#"{"rows":[],"columns":[],"measures":[],"filters":[{"kind":"column","column":0,"name":"","precision":"month","condition":"not_in_list","values":["Nord",null],"operand":"1","upper":"2"},{"kind":"measure","aggregate":"sum","column":2,"name":"Gros","condition":"between","operand":"100","upper":"400","level":1,"level_precision":"month"},{"kind":"measure","aggregate":"count_star","column":0,"name":"","condition":"not_equal","operand":"","upper":"","level_precision":"none"},{"kind":"measure","aggregate":"max","column":3,"name":"","precision":"quarter","condition":"equal","operand":"2026-T1","upper":"","level_precision":"none"}]}"#;
        let config: PivotConfig = serde_json::from_str(saved).unwrap();
        assert_eq!(
            config.filters[0],
            PivotFilter::Column {
                column: 0,
                name: String::new(),
                precision: DatePrecision::Month,
                condition: PivotCondition::NotInList,
                values: vec![Some("Nord".to_string()), None],
                operand: "1".to_string(),
                upper: "2".to_string(),
            }
        );
        assert_eq!(
            config.filters[1],
            PivotFilter::Measure {
                aggregate: PivotAggregate::Sum,
                column: 2,
                name: "Gros".to_string(),
                condition: PivotTest::Between,
                operand: "100".to_string(),
                upper: "400".to_string(),
                level: Some(1),
                level_precision: DatePrecision::Month,
                precision: DatePrecision::None,
            }
        );
        assert!(matches!(
            config.filters[3],
            PivotFilter::Measure {
                precision: DatePrecision::Quarter,
                ..
            }
        ));
        assert_eq!(serde_json::to_string(&config).unwrap(), saved);
    }

    #[test]
    fn a_filter_saved_before_its_conditions_keeps_what_it_held() {
        let saved = r#"{"rows":[],"columns":[],"measures":[],"filters":[{"kind":"values","column":0,"name":"Zone","values":["Nord",null]},{"kind":"measure","aggregate":"sum","column":2,"name":"","comparison":"greater","operand":"100","level":1}]}"#;
        let config: PivotConfig = serde_json::from_str(saved).unwrap();
        assert_eq!(
            config.filters[0],
            PivotFilter::Column {
                column: 0,
                name: "Zone".to_string(),
                precision: DatePrecision::None,
                condition: PivotCondition::InList,
                values: vec![Some("Nord".to_string()), None],
                operand: String::new(),
                upper: String::new(),
            }
        );
        assert_eq!(
            config.filters[1],
            PivotFilter::Measure {
                aggregate: PivotAggregate::Sum,
                column: 2,
                name: String::new(),
                condition: PivotTest::Greater,
                operand: "100".to_string(),
                upper: String::new(),
                level: Some(1),
                level_precision: DatePrecision::None,
                precision: DatePrecision::None,
            }
        );
    }

    #[test]
    fn a_measure_keeps_the_precision_of_its_dates() {
        let saved = r#"{"rows":[],"columns":[],"measures":[{"aggregate":"count","column":0,"name":""},{"aggregate":"min","column":1,"name":"","precision":"month"}],"filters":[]}"#;
        let config: PivotConfig = serde_json::from_str(saved).unwrap();
        assert_eq!(config.measures[0].precision, DatePrecision::None);
        assert_eq!(config.measures[1].precision, DatePrecision::Month);
        assert_eq!(serde_json::to_string(&config).unwrap(), saved);
    }

    #[test]
    fn a_group_concat_keeps_its_settings() {
        let saved = r#"{"rows":[],"columns":[],"measures":[{"aggregate":"group_concat","column":1,"name":"","precision":"month","concat":{"distinct":true,"order":[{"column":2,"descending":true},{"column":0}],"separator":" | ","offset":1,"limit":5}},{"aggregate":"group_concat","column":0,"name":"","concat":{"separator":", "}}],"filters":[]}"#;
        let config: PivotConfig = serde_json::from_str(saved).unwrap();
        assert_eq!(
            config.measures[0].concat,
            Some(PivotConcat {
                distinct: true,
                order: vec![
                    PivotConcatKey {
                        column: 2,
                        descending: true,
                    },
                    PivotConcatKey {
                        column: 0,
                        descending: false,
                    },
                ],
                separator: " | ".to_string(),
                offset: 1,
                limit: Some(5),
            })
        );
        assert_eq!(serde_json::to_string(&config).unwrap(), saved);
    }

    #[test]
    fn keeps_the_layout_of_a_configuration() {
        let saved = r#"{"rows":[{"column":1,"name":"","precision":"none","without_subtotal":true}],"columns":[],"measures":[],"filters":[],"form":"tabular","repeat_labels":true,"without_row_total":true,"without_column_total":true}"#;
        let config: PivotConfig = serde_json::from_str(saved).unwrap();
        assert_eq!(config.form, PivotForm::Tabular);
        assert!(config.repeat_labels);
        assert!(config.without_row_total);
        assert!(config.without_column_total);
        assert!(config.rows[0].without_subtotal);
        assert_eq!(serde_json::to_string(&config).unwrap(), saved);
    }

    #[test]
    fn keeps_the_widths_of_a_configuration() {
        let saved = r#"{"rows":[{"column":1,"name":"","precision":"none","width":30}],"columns":[],"measures":[],"filters":[],"labels_width":50,"column_widths":[{"path":["2025",null],"measure":1,"width":44}],"fields_width":70,"zones_width":80}"#;
        let config: PivotConfig = serde_json::from_str(saved).unwrap();
        assert_eq!(config.rows[0].width, Some(30));
        assert_eq!(config.labels_width, Some(50));
        assert_eq!(config.fields_width, Some(70));
        assert_eq!(config.zones_width, Some(80));
        assert_eq!(
            config.column_widths,
            [PivotColumnWidth {
                path: vec![Some("2025".to_string()), None],
                measure: 1,
                width: 44,
            }]
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
