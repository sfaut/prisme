//! Tests of the engine on the example dataset of the specification: the
//! expected outputs are those of its cases, in the layout of its copy (one
//! column per level of the rows, `Total` in the first level a line
//! aggregates, flattened headers such as `2025 / T1 / ca`).

use prisme_store::{NumberError, Store};

use super::*;

/// Columns of the example dataset.
const REGION: usize = 0;
const CITY: usize = 1;
const DATE: usize = 2;
const YEAR: usize = 3;
const QUARTER: usize = 4;
const AMOUNT: usize = 5;

/// Example dataset of the specification, with its year and quarter already
/// computed (`EXTRACT(YEAR FROM d) AS annee`, `'T' || … AS trimestre`).
fn example() -> Engine {
    let rows = [
        ["Nord", "Lille", "2025-01-15", "2025", "T1", "100"],
        ["Nord", "Lille", "2025-05-10", "2025", "T2", "120"],
        ["Nord", "Lille", "2026-02-03", "2026", "T1", "150"],
        ["Nord", "Arras", "2025-02-20", "2025", "T1", "40"],
        ["Nord", "Arras", "2026-04-12", "2026", "T2", "60"],
        ["Sud", "Nice", "2025-03-05", "2025", "T1", "80"],
        ["Sud", "Nice", "2025-06-18", "2025", "T2", "90"],
        ["Sud", "Nice", "2026-01-22", "2026", "T1", "110"],
        ["Sud", "Nice", "2026-05-30", "2026", "T2", "70"],
        ["Sud", "Toulon", "2026-03-14", "2026", "T1", "30"],
    ];
    let mut store = Store::new(6);
    for row in rows {
        store.push_row(&row.map(Some));
    }
    let mut families = vec![Family::Other; 5];
    families.push(Family::Exact);
    Engine::new(store, families)
}

/// Engine over cells of a single column, of family `family`.
fn single(cells: &[Option<&str>], family: Family) -> Engine {
    let mut store = Store::new(1);
    for &cell in cells {
        store.push_row(&[cell]);
    }
    Engine::new(store, vec![family])
}

/// Table in the layout of its copy, fields separated by `;`.
///
/// `levels` names the levels of the rows, `measures` the measures.
fn flat(engine: &Engine, table: &Table, levels: &[&str], measures: &[&str]) -> String {
    let mut header: Vec<String> = levels.iter().map(|&level| level.to_owned()).collect();
    for column in 0..table.column_count() {
        let column = table.header(engine, column);
        let mut parts: Vec<&str> = column
            .path
            .iter()
            .map(|&(_, label)| label.unwrap_or("NULL"))
            .collect();
        if column.total {
            parts.push("Total");
        }
        parts.extend(column.measure.map(|measure| measures[measure]));
        header.push(parts.join(" / "));
    }
    let mut lines = vec![header.join(";")];
    for line in 0..table.line_count() {
        let mut fields: Vec<String> = table
            .line_path(engine, line)
            .into_iter()
            .map(|label| label.unwrap_or("NULL").to_owned())
            .collect();
        if fields.len() < table.row_levels() {
            fields.push("Total".to_owned());
        }
        fields.resize(table.row_levels(), String::new());
        for column in 0..table.column_count() {
            fields.push(match table.value(line, column) {
                Value::Null => "NULL".to_owned(),
                value => value.text(engine).unwrap_or_default(),
            });
        }
        lines.push(fields.join(";"));
    }
    lines.join("\n")
}

/// Computes a table of the example dataset and gives it in the layout of its
/// copy.
fn example_table(rows: &[usize], columns: &[usize], measures: &[Measure]) -> String {
    let engine = example();
    let request = Request {
        rows: rows.iter().map(|&column| column.into()).collect(),
        columns: columns.iter().map(|&column| column.into()).collect(),
        measures: measures.to_vec(),
        ..Request::default()
    };
    let table = engine.compute(&request).unwrap();
    let names = |columns: &[usize]| -> Vec<&str> {
        columns
            .iter()
            .map(|&column| ["region", "ville", "d", "annee", "trimestre", "montant"][column])
            .collect()
    };
    let measure_names: Vec<&str> = measures
        .iter()
        .map(|measure| match measure {
            Measure::Sum(AMOUNT) => "ca",
            Measure::CountStar => "nb",
            _ => "?",
        })
        .collect();
    flat(&engine, &table, &names(rows), &measure_names)
}

#[test]
fn case_1_two_levels_of_rows() {
    let expected = "\
region;ville;2025 / ca;2026 / ca;Total / ca
Nord;Total;260;210;470
Nord;Arras;40;60;100
Nord;Lille;220;150;370
Sud;Total;170;210;380
Sud;Nice;170;180;350
Sud;Toulon;;30;30
Total;;430;420;850";
    assert_eq!(
        example_table(&[REGION, CITY], &[YEAR], &[Measure::Sum(AMOUNT)]),
        expected
    );
}

#[test]
fn case_2_two_levels_of_columns() {
    let expected = "\
region;2025 / T1 / ca;2025 / T2 / ca;2025 / Total / ca;2026 / T1 / ca;2026 / T2 / ca;2026 / Total / ca;Total / ca
Nord;140;120;260;150;60;210;470
Sud;80;90;170;140;70;210;380
Total;220;210;430;290;130;420;850";
    assert_eq!(
        example_table(&[REGION], &[YEAR, QUARTER], &[Measure::Sum(AMOUNT)]),
        expected
    );
}

#[test]
fn case_3_two_measures() {
    let expected = "\
region;2025 / ca;2025 / nb;2026 / ca;2026 / nb;Total / ca;Total / nb
Nord;260;3;210;2;470;5
Sud;170;2;210;3;380;5
Total;430;5;420;5;850;10";
    let measures = [Measure::Sum(AMOUNT), Measure::CountStar];
    assert_eq!(example_table(&[REGION], &[YEAR], &measures), expected);
}

#[test]
fn case_4_two_levels_on_each_axis_and_two_measures() {
    let expected = "\
region;ville;2025 / T1 / ca;2025 / T1 / nb;2025 / T2 / ca;2025 / T2 / nb;2025 / Total / ca;2025 / Total / nb;2026 / T1 / ca;2026 / T1 / nb;2026 / T2 / ca;2026 / T2 / nb;2026 / Total / ca;2026 / Total / nb;Total / ca;Total / nb
Nord;Total;140;2;120;1;260;3;150;1;60;1;210;2;470;5
Nord;Arras;40;1;;;40;1;;;60;1;60;1;100;2
Nord;Lille;100;1;120;1;220;2;150;1;;;150;1;370;3
Sud;Total;80;1;90;1;170;2;140;2;70;1;210;3;380;5
Sud;Nice;80;1;90;1;170;2;110;1;70;1;180;2;350;4
Sud;Toulon;;;;;;;30;1;;;30;1;30;1
Total;;220;3;210;2;430;5;290;3;130;2;420;5;850;10";
    let measures = [Measure::Sum(AMOUNT), Measure::CountStar];
    assert_eq!(
        example_table(&[REGION, CITY], &[YEAR, QUARTER], &measures),
        expected
    );
}

#[test]
fn case_5_without_levels_of_columns() {
    let expected = "\
region;ca;nb
Nord;470;5
Sud;380;5
Total;850;10";
    let measures = [Measure::Sum(AMOUNT), Measure::CountStar];
    assert_eq!(example_table(&[REGION], &[], &measures), expected);
}

#[test]
fn case_6_without_levels_of_rows() {
    let expected = "\
2025 / ca;2026 / ca;Total / ca
430;420;850";
    assert_eq!(
        example_table(&[], &[YEAR], &[Measure::Sum(AMOUNT)]),
        expected
    );
}

#[test]
fn case_7_without_any_level() {
    let expected = "\
ca;nb
850;10";
    let measures = [Measure::Sum(AMOUNT), Measure::CountStar];
    assert_eq!(example_table(&[], &[], &measures), expected);
}

#[test]
fn without_measure_the_table_shows_the_labels_only() {
    let expected = "\
region;2025;2026;Total
Nord;;;
Sud;;;
Total;;;";
    assert_eq!(example_table(&[REGION], &[YEAR], &[]), expected);
}

#[test]
fn the_kept_rows_alone_are_measured() {
    let mut engine = example();
    // The rows of Lille only.
    engine.set_kept(Some(vec![0, 1, 2])).unwrap();
    let request = Request {
        rows: vec![CITY.into()],
        columns: vec![],
        measures: vec![Measure::Sum(AMOUNT), Measure::CountStar],
        ..Request::default()
    };
    let table = engine.compute(&request).unwrap();
    assert_eq!(table.selected_rows(), 3);
    assert_eq!(table.finest_groups(), 1);
    assert_eq!(
        flat(&engine, &table, &["ville"], &["ca", "nb"]),
        "ville;ca;nb\nLille;370;3\nTotal;370;3"
    );
    assert_eq!(engine.set_kept(Some(vec![10])), Err(Error::UnknownRow(10)));
}

#[test]
fn the_ranks_given_by_the_page_order_the_values() {
    let mut engine = example();
    // Sud first, then Nord, by code (Nord appears first).
    engine.set_ranks(REGION, vec![1, 0]).unwrap();
    let request = Request {
        rows: vec![REGION.into()],
        columns: vec![],
        measures: vec![Measure::CountStar],
        ..Request::default()
    };
    let table = engine.compute(&request).unwrap();
    assert_eq!(
        flat(&engine, &table, &["region"], &["nb"]),
        "region;nb\nSud;5\nNord;5\nTotal;10"
    );
    assert_eq!(
        engine.set_ranks(REGION, vec![0, 0]),
        Err(Error::Ranks(REGION))
    );
    assert_eq!(engine.set_ranks(REGION, vec![0]), Err(Error::Ranks(REGION)));
    assert_eq!(
        engine.set_ranks(REGION, vec![0, 2]),
        Err(Error::Ranks(REGION))
    );
}

#[test]
fn null_is_a_group_apart_from_the_text_null_and_the_empty_text() {
    let engine = single(
        &[Some("NULL"), None, Some(""), None, Some("a")],
        Family::Other,
    );
    assert_eq!(
        engine.distinct(0).unwrap(),
        [Some("NULL"), None, Some(""), Some("a")]
    );
    let request = Request {
        rows: vec![0.into()],
        columns: vec![],
        measures: vec![Measure::CountStar, Measure::Count(0)],
        ..Request::default()
    };
    let table = engine.compute(&request).unwrap();
    assert_eq!(
        flat(&engine, &table, &["x"], &["n", "c"]),
        "x;n;c\nNULL;2;0\n;1;1\nNULL;1;1\na;1;1\nTotal;5;3"
    );
    assert_eq!(table.line(&engine, 0).label, Label::Value(None));
    assert_eq!(table.line(&engine, 2).label, Label::Value(Some("NULL")));
    assert_eq!(
        table.line(&engine, 4),
        Line {
            depth: 0,
            label: Label::Total,
            shared: 0,
        }
    );
}

#[test]
fn sums_ignore_null_and_give_null_without_a_value() {
    let mut store = Store::new(2);
    for row in [["a", "1.5"], ["a", "2.25"], ["b", "NULL"]] {
        let amount = (row[1] != "NULL").then_some(row[1]);
        store.push_row(&[Some(row[0]), amount]);
    }
    let engine = Engine::new(store, vec![Family::Other, Family::Exact]);
    let request = Request {
        rows: vec![0.into()],
        columns: vec![],
        measures: vec![Measure::Sum(1), Measure::Count(1)],
        ..Request::default()
    };
    let table = engine.compute(&request).unwrap();
    assert_eq!(
        flat(&engine, &table, &["k"], &["s", "c"]),
        "k;s;c\na;3.75;2\nb;NULL;0\nTotal;3.75;2"
    );
}

#[test]
fn floats_are_summed_as_double() {
    let engine = single(&[Some("0.1"), Some("0.2"), None], Family::Float);
    let request = Request {
        rows: vec![],
        columns: vec![],
        measures: vec![Measure::Sum(0)],
        ..Request::default()
    };
    let table = engine.compute(&request).unwrap();
    assert_eq!(table.value(0, 0), Value::Float(0.1 + 0.2));
}

#[test]
fn a_result_without_rows_has_its_grand_total() {
    let engine = single(&[], Family::Exact);
    let request = Request {
        rows: vec![0.into()],
        columns: vec![0.into()],
        measures: vec![Measure::CountStar, Measure::Sum(0)],
        ..Request::default()
    };
    let table = engine.compute(&request).unwrap();
    assert_eq!((table.line_count(), table.column_count()), (1, 2));
    assert_eq!(table.value(0, 0), Value::Count(0));
    assert_eq!(table.value(0, 1), Value::Null);
}

#[test]
fn errors_give_no_table() {
    let engine = example();
    let request = |measures: Vec<Measure>| Request {
        rows: vec![REGION.into()],
        columns: vec![],
        measures,
        ..Request::default()
    };
    assert_eq!(
        engine
            .compute(&request(vec![Measure::Sum(CITY)]))
            .unwrap_err(),
        Error::NotNumeric { measure: 0 }
    );
    assert_eq!(
        engine
            .compute(&request(vec![Measure::Count(9)]))
            .unwrap_err(),
        Error::UnknownColumn(9)
    );
    let unknown_level = Request {
        rows: vec![9.into()],
        ..Request::default()
    };
    assert_eq!(
        engine.compute(&unknown_level).unwrap_err(),
        Error::UnknownColumn(9)
    );

    let engine = single(&[Some("1"), Some("abc")], Family::Exact);
    let sum = Request {
        measures: vec![Measure::CountStar, Measure::Sum(0)],
        ..Request::default()
    };
    assert_eq!(
        engine.compute(&sum).unwrap_err(),
        Error::Number {
            column: 0,
            error: NumberError::Invalid { row: 1 }
        }
    );

    let largest = i128::MAX.to_string();
    let engine = single(&[Some(&largest), Some("1")], Family::Exact);
    assert_eq!(
        engine.compute(&sum).unwrap_err(),
        Error::Overflow { measure: 1 }
    );
}

#[test]
fn a_total_of_count_distinct_counts_the_values_of_its_whole_group() {
    let engine = example();
    let request = Request {
        rows: vec![REGION.into()],
        columns: vec![YEAR.into()],
        measures: vec![Measure::CountDistinct(CITY.into())],
        ..Request::default()
    };
    let table = engine.compute(&request).unwrap();
    let expected = "\
region;2025 / villes;2026 / villes;Total / villes
Nord;2;2;2
Sud;1;2;2
Total;3;4;4";
    assert_eq!(flat(&engine, &table, &["region"], &["villes"]), expected);
}

/// Engine over rows of a level, a text, and of a number of family `family`.
fn level_and_numbers(rows: &[(&str, Option<&str>)], family: Family) -> Engine {
    let mut store = Store::new(2);
    for &(level, number) in rows {
        store.push_row(&[Some(level), number]);
    }
    Engine::new(store, vec![Family::Other, family])
}

/// Measures of each level of column 0, then their total, in the layout of the
/// copy of a table.
fn by_level(engine: &Engine, measures: &[Measure]) -> String {
    let request = Request {
        rows: vec![0.into()],
        columns: vec![],
        measures: measures.to_vec(),
        ..Request::default()
    };
    let table = engine.compute(&request).unwrap();
    let lines: Vec<String> = (0..table.line_count())
        .map(|line| {
            let label = table.line_path(engine, line).first().copied().flatten();
            let values = (0..table.column_count()).map(|column| {
                let text = table.value(line, column).text(engine);
                text.unwrap_or_else(|| "NULL".to_owned())
            });
            let mut fields = vec![label.unwrap_or("Total").to_owned()];
            fields.extend(values);
            fields.join(";")
        })
        .collect();
    lines.join("\n")
}

#[test]
fn sums_and_averages_of_distinct_numbers_read_their_values_once_per_group() {
    let engine = level_and_numbers(
        &[
            ("A", Some("1.0")),
            ("A", Some("1.00")),
            ("A", Some("2")),
            ("B", Some("2")),
            ("B", None),
        ],
        Family::Exact,
    );
    let measures = [
        Measure::Sum(1),
        Measure::SumDistinct(1),
        Measure::Avg(1),
        Measure::AvgDistinct(1),
    ];
    let expected = "\
A;4.00;3.00;1.333333;1.500000
B;2.00;2.00;2.000000;2.000000
Total;6.00;3.00;1.500000;1.500000";
    assert_eq!(by_level(&engine, &measures), expected);
}

#[test]
fn averages_of_floats_are_doubles() {
    let engine = level_and_numbers(
        &[("A", Some("1")), ("A", Some("2")), ("B", Some("0.5"))],
        Family::Float,
    );
    let measures = [Measure::Avg(1), Measure::AvgDistinct(1)];
    let expected = "\
A;1.5;1.5
B;0.5;0.5
Total;1.1666666666666667;1.1666666666666667";
    assert_eq!(by_level(&engine, &measures), expected);
}

#[test]
fn extremes_compare_numbers_by_value_and_give_their_cell_as_it_is() {
    let engine = level_and_numbers(
        &[
            ("A", Some("100")),
            ("A", Some("30.5")),
            ("B", Some("9")),
            ("B", None),
            ("C", None),
        ],
        Family::Exact,
    );
    let measures = [Measure::Min(1.into()), Measure::Max(1.into())];
    let expected = "\
A;30.5;100
B;9;9
C;NULL;NULL
Total;9;100";
    assert_eq!(by_level(&engine, &measures), expected);

    let engine = level_and_numbers(
        &[("A", Some("1e300")), ("A", Some("NaN")), ("A", Some("-1"))],
        Family::Float,
    );
    let expected = "\
A;-1;NaN
Total;-1;NaN";
    assert_eq!(by_level(&engine, &measures), expected);
}

#[test]
fn extremes_compare_texts_by_code_points_whatever_the_order_of_the_page() {
    let mut engine = single(
        &[Some("b"), Some("B"), Some("é"), Some("a"), Some("A"), None],
        Family::Other,
    );
    // Order of the grid: NULL, then a, A, b, B, é.
    let values = engine.distinct(0).unwrap();
    let order: Vec<u32> = [None, Some("a"), Some("A"), Some("b"), Some("B"), Some("é")]
        .iter()
        .map(|value| values.iter().position(|cell| cell == value).unwrap() as u32)
        .collect();
    let mut ranks = vec![0; order.len()];
    for (rank, &code) in order.iter().enumerate() {
        ranks[code as usize] = rank as u32;
    }
    engine.set_ranks(0, ranks).unwrap();
    let request = Request {
        measures: vec![Measure::Min(0.into()), Measure::Max(0.into())],
        ..Request::default()
    };
    let table = engine.compute(&request).unwrap();
    let text = |column| table.value(0, column).text(&engine);
    assert_eq!(text(0).as_deref(), Some("A"));
    assert_eq!(text(1).as_deref(), Some("é"));
}

#[test]
fn extremes_read_the_rows_kept_by_the_filter() {
    let mut engine = single(&[Some("3"), Some("1"), Some("2")], Family::Exact);
    engine.set_kept(Some(vec![0, 2])).unwrap();
    let request = Request {
        measures: vec![Measure::Min(0.into()), Measure::Max(0.into())],
        ..Request::default()
    };
    let table = engine.compute(&request).unwrap();
    let text = |column| table.value(0, column).text(&engine);
    assert_eq!(text(0).as_deref(), Some("2"));
    assert_eq!(text(1).as_deref(), Some("3"));
}

#[test]
fn only_columns_of_numbers_can_be_added_up() {
    let engine = single(&[Some("a")], Family::Other);
    for measure in [
        Measure::SumDistinct(0),
        Measure::Avg(0),
        Measure::AvgDistinct(0),
    ] {
        let request = Request {
            measures: vec![Measure::CountStar, measure],
            ..Request::default()
        };
        assert_eq!(
            engine.compute(&request).unwrap_err(),
            Error::NotNumeric { measure: 1 }
        );
    }
}

#[test]
fn numbers_that_cannot_be_read_stop_the_extremes_and_the_averages() {
    let engine = single(&[Some("1"), Some("abc")], Family::Exact);
    for measure in [
        Measure::Min(0.into()),
        Measure::Max(0.into()),
        Measure::Avg(0),
    ] {
        let request = Request {
            measures: vec![measure],
            ..Request::default()
        };
        assert_eq!(
            engine.compute(&request).unwrap_err(),
            Error::Number {
                column: 0,
                error: NumberError::Invalid { row: 1 }
            }
        );
    }
}

#[test]
fn the_rows_of_a_table_can_group_dates_by_quarter() {
    let engine = example();
    let request = Request {
        rows: vec![Level {
            column: DATE,
            precision: Some(Precision::Quarter),
        }],
        measures: vec![Measure::Sum(AMOUNT)],
        ..Request::default()
    };
    let table = engine.compute(&request).unwrap();
    let expected = "\
d;ca
2025-T1;220
2025-T2;210
2026-T1;290
2026-T2;130
Total;850";
    assert_eq!(flat(&engine, &table, &["d"], &["ca"]), expected);
}

#[test]
fn the_columns_of_a_table_can_group_dates_by_iso_year() {
    let engine = example();
    let request = Request {
        rows: vec![REGION.into()],
        columns: vec![Level {
            column: DATE,
            precision: Some(Precision::Year),
        }],
        measures: vec![Measure::Sum(AMOUNT)],
        ..Request::default()
    };
    let table = engine.compute(&request).unwrap();
    let expected = "\
region;2025 / ca;2026 / ca;Total / ca
Nord;260;210;470
Sud;170;210;380
Total;430;420;850";
    assert_eq!(flat(&engine, &table, &["region"], &["ca"]), expected);
}

#[test]
fn a_column_keeps_its_values_next_to_its_periods() {
    let engine = example();
    let by_month = Request {
        rows: vec![Level {
            column: DATE,
            precision: Some(Precision::Month),
        }],
        measures: vec![Measure::CountStar, Measure::CountDistinct(DATE.into())],
        ..Request::default()
    };
    let table = engine.compute(&by_month).unwrap();
    assert_eq!(table.finest_groups(), 10);
    assert_eq!(table.line(&engine, 0).label, Label::Value(Some("2025-01")));
    let by_value = Request {
        rows: vec![DATE.into()],
        ..Request::default()
    };
    let table = engine.compute(&by_value).unwrap();
    assert_eq!(
        table.line(&engine, 0).label,
        Label::Value(Some("2025-01-15"))
    );
}

#[test]
fn a_cell_without_a_date_stops_the_computation() {
    let engine = single(&[Some("2026-01-15"), None, Some("infinity")], Family::Other);
    let request = Request {
        rows: vec![Level {
            column: 0,
            precision: Some(Precision::Month),
        }],
        ..Request::default()
    };
    assert_eq!(
        engine.compute(&request).unwrap_err(),
        Error::InvalidDate { column: 0, row: 2 }
    );
    assert_eq!(
        engine.compute(&request).unwrap_err().to_string(),
        "column 0, row 2: outside of the calendar"
    );
}

#[test]
fn a_date_without_a_day_in_the_calendar_stops_the_computation_under_a_precision() {
    let cells = [Some("2026-01-15"), Some("0000-00-00"), Some("2026-03-00")];
    let mut engine = single(&cells, Family::Other);
    let by_month = Request {
        rows: vec![Level {
            column: 0,
            precision: Some(Precision::Month),
        }],
        ..Request::default()
    };
    assert_eq!(
        engine.compute(&by_month).unwrap_err(),
        Error::InvalidDate { column: 0, row: 1 }
    );

    // Without a precision, it stays a value of its own.
    let by_value = Request {
        rows: vec![0.into()],
        ..Request::default()
    };
    assert_eq!(engine.compute(&by_value).unwrap().line_count(), 4);

    // A row the filter of the grid leaves out stops nothing.
    engine.set_kept(Some(vec![2, 0])).unwrap();
    assert_eq!(
        engine.compute(&by_month).unwrap_err(),
        Error::InvalidDate { column: 0, row: 2 }
    );
    engine.set_kept(Some(vec![0])).unwrap();
    assert_eq!(engine.compute(&by_month).unwrap().line_count(), 2);
}

#[test]
fn a_level_with_a_precision_needs_its_column() {
    let engine = single(&[Some("2026-01-15")], Family::Other);
    let request = Request {
        rows: vec![Level {
            column: 9,
            precision: Some(Precision::Month),
        }],
        ..Request::default()
    };
    assert_eq!(
        engine.compute(&request).unwrap_err(),
        Error::UnknownColumn(9)
    );
}

#[test]
fn a_precision_groups_only_the_rows_kept_by_the_filter() {
    let mut engine = example();
    engine.set_kept(Some(vec![5, 6])).unwrap();
    let request = Request {
        rows: vec![Level {
            column: DATE,
            precision: Some(Precision::Quarter),
        }],
        measures: vec![Measure::Sum(AMOUNT)],
        ..Request::default()
    };
    let table = engine.compute(&request).unwrap();
    let expected = "\
d;ca
2025-T1;80
2025-T2;90
Total;170";
    assert_eq!(flat(&engine, &table, &["d"], &["ca"]), expected);
}

/// Level of the dates of column `column` at a precision.
fn dated(column: usize, precision: Precision) -> Level {
    Level {
        column,
        precision: Some(precision),
    }
}

#[test]
fn distinct_counts_and_extremes_read_the_periods_of_a_precision() {
    let engine = example();
    let request = Request {
        rows: vec![REGION.into()],
        measures: vec![
            Measure::CountDistinct(dated(DATE, Precision::Quarter)),
            Measure::Min(dated(DATE, Precision::Month)),
            Measure::Max(dated(DATE, Precision::Week)),
        ],
        ..Request::default()
    };
    let table = engine.compute(&request).unwrap();
    // A total counts the quarters of its whole group: 4, and not 4 + 4.
    let expected = "\
region;trimestres;premier;dernier
Nord;4;2025-01;2026-S15
Sud;4;2025-03;2026-S22
Total;4;2025-01;2026-S22";
    assert_eq!(
        flat(
            &engine,
            &table,
            &["region"],
            &["trimestres", "premier", "dernier"]
        ),
        expected
    );
}

#[test]
fn an_extreme_at_a_precision_ranks_its_lines_in_the_order_of_time() {
    let engine = keyed(
        &[
            ("a", Some("2026-01-15")),
            ("b", Some("2025-12-31")),
            ("c", None),
        ],
        Family::Date,
    );
    let by_month = Measure::Min(dated(1, Precision::Month));
    assert_eq!(keys_by_value(&engine, by_month), "c b a");
    // 2025-12-31 falls in the ISO year 2026: the tie keeps the order of the labels.
    let by_year = Measure::Max(dated(1, Precision::Year));
    assert_eq!(keys_by_value(&engine, by_year), "c a b");
}

#[test]
fn a_date_outside_of_the_calendar_stops_a_measure_at_a_precision() {
    let mut engine = single(&[Some("2026-01-15"), Some("0000-00-00")], Family::Date);
    for measure in [
        Measure::CountDistinct(dated(0, Precision::Month)),
        Measure::Min(dated(0, Precision::Month)),
        Measure::Max(dated(0, Precision::Year)),
    ] {
        let request = Request {
            measures: vec![measure],
            ..Request::default()
        };
        assert_eq!(
            engine.compute(&request).unwrap_err(),
            Error::InvalidDate { column: 0, row: 1 },
            "{measure:?}"
        );
    }
    // Without a precision, it stays a value of its own.
    let request = Request {
        measures: vec![Measure::Max(0.into())],
        ..Request::default()
    };
    let table = engine.compute(&request).unwrap();
    assert_eq!(
        table.value(0, 0).text(&engine).as_deref(),
        Some("2026-01-15")
    );
    // A row the filter of the grid leaves out stops nothing.
    engine.set_kept(Some(vec![0])).unwrap();
    let request = Request {
        measures: vec![Measure::Min(dated(0, Precision::Quarter))],
        ..Request::default()
    };
    let table = engine.compute(&request).unwrap();
    assert_eq!(table.value(0, 0).text(&engine).as_deref(), Some("2026-T1"));
}

#[test]
fn a_filter_on_an_extreme_at_a_precision_compares_periods() {
    let engine = pairs(
        &[
            ("a", Some("2026-01-15")),
            ("a", Some("2026-03-02")),
            ("b", Some("2025-11-30")),
        ],
        Family::Date,
    );
    let last_month = Measure::Max(dated(1, Precision::Month));
    let keys = |comparison, operand| kept_keys(&engine, last_month, comparison, operand);
    assert_eq!(keys(Comparison::GreaterOrEqual, "2026-01").unwrap(), ["a"]);
    assert_eq!(keys(Comparison::Equal, "2025-11").unwrap(), ["b"]);
    // A date is no month.
    assert_eq!(
        keys(Comparison::Equal, "2026-03-02"),
        Err(Error::FilterOperand { filter: 0 })
    );
    let months = Measure::CountDistinct(dated(1, Precision::Month));
    assert_eq!(
        kept_keys(&engine, months, Comparison::Equal, "2").unwrap(),
        ["a"]
    );

    let engine = pairs(
        &[("a", Some("2026-01-15")), ("b", Some("2026-02-30"))],
        Family::Date,
    );
    assert_eq!(
        kept_keys(&engine, last_month, Comparison::Equal, "2026-01"),
        Err(Error::InvalidDate { column: 1, row: 1 })
    );
}

#[test]
fn an_average_that_does_not_hold_on_128_bits_is_an_error() {
    let digits = format!("0.{}1", "0".repeat(34));
    let engine = single(&[Some(&digits)], Family::Exact);
    let request = Request {
        measures: vec![Measure::CountStar, Measure::Avg(0)],
        ..Request::default()
    };
    assert_eq!(
        engine.compute(&request).unwrap_err(),
        Error::Overflow { measure: 1 }
    );
}

/// Values of a node, named by its labels.
fn value_order(path: &[&str], measure: usize, descending: bool) -> ValueOrder {
    ValueOrder {
        path: path.iter().map(|&label| Some(label.to_owned())).collect(),
        measure,
        descending,
    }
}

/// Order of the lines by the values of a column.
fn lines_by(path: &[&str], measure: usize, descending: bool) -> Order {
    Order {
        rows: AxisOrder {
            values: Some(value_order(path, measure, descending)),
            ..AxisOrder::default()
        },
        ..Order::default()
    }
}

/// Order of the columns by the values of a line.
fn columns_by(path: &[&str], descending: bool) -> Order {
    Order {
        columns: AxisOrder {
            values: Some(value_order(path, 0, descending)),
            ..AxisOrder::default()
        },
        ..Order::default()
    }
}

/// Table of the example dataset in an order, in the layout of its copy.
fn sorted_example(
    rows: &[usize],
    columns: &[usize],
    measures: &[Measure],
    order: &Order,
) -> String {
    arranged_example(rows, columns, measures, order, &Layout::default())
}

/// Table of the example dataset in an order and a layout, as its copy lays it
/// out: one column per level, whatever the form.
fn arranged_example(
    rows: &[usize],
    columns: &[usize],
    measures: &[Measure],
    order: &Order,
    layout: &Layout,
) -> String {
    let engine = example();
    let request = Request {
        rows: rows.iter().map(|&column| column.into()).collect(),
        columns: columns.iter().map(|&column| column.into()).collect(),
        measures: measures.to_vec(),
        ..Request::default()
    };
    let mut table = engine.compute(&request).unwrap();
    table.lay_out(&engine, order, layout).unwrap();
    let names: Vec<&str> = rows
        .iter()
        .map(|&column| ["region", "ville", "d", "annee", "trimestre", "montant"][column])
        .collect();
    flat(&engine, &table, &names, &["ca", "nb"])
}

#[test]
fn a_level_in_descending_order_reverses_its_values() {
    let order = Order {
        rows: AxisOrder {
            descending: vec![true],
            values: None,
        },
        columns: AxisOrder {
            descending: vec![true],
            values: None,
        },
    };
    let expected = "\
region;ville;2026 / ca;2025 / ca;Total / ca
Sud;Total;210;170;380
Sud;Nice;180;170;350
Sud;Toulon;30;;30
Nord;Total;210;260;470
Nord;Arras;60;40;100
Nord;Lille;150;220;370
Total;;420;430;850";
    assert_eq!(
        sorted_example(&[REGION, CITY], &[YEAR], &[Measure::Sum(AMOUNT)], &order),
        expected
    );
}

#[test]
fn the_values_of_a_column_rank_the_lines_within_their_group() {
    let measures = [Measure::Sum(AMOUNT)];
    let by_2025 = |descending| {
        let order = lines_by(&["2025"], 0, descending);
        sorted_example(&[REGION, CITY], &[YEAR], &measures, &order)
    };
    let ascending = "\
region;ville;2025 / ca;2026 / ca;Total / ca
Sud;Total;170;210;380
Sud;Toulon;;30;30
Sud;Nice;170;180;350
Nord;Total;260;210;470
Nord;Arras;40;60;100
Nord;Lille;220;150;370
Total;;430;420;850";
    assert_eq!(by_2025(false), ascending);
    let descending = "\
region;ville;2025 / ca;2026 / ca;Total / ca
Nord;Total;260;210;470
Nord;Lille;220;150;370
Nord;Arras;40;60;100
Sud;Total;170;210;380
Sud;Nice;170;180;350
Sud;Toulon;;30;30
Total;;430;420;850";
    assert_eq!(by_2025(true), descending);
}

#[test]
fn a_tie_keeps_the_order_of_the_labels() {
    let measures = [Measure::Sum(AMOUNT)];
    let regions = |order: &Order| sorted_example(&[REGION], &[YEAR], &measures, order);
    let nord_first = "\
region;2025 / ca;2026 / ca;Total / ca
Nord;260;210;470
Sud;170;210;380
Total;430;420;850";
    assert_eq!(regions(&lines_by(&["2026"], 0, false)), nord_first);
    assert_eq!(regions(&lines_by(&["2026"], 0, true)), nord_first);
    let mut sud_first = lines_by(&["2026"], 0, true);
    sud_first.rows.descending = vec![true];
    let expected = "\
region;2025 / ca;2026 / ca;Total / ca
Sud;170;210;380
Nord;260;210;470
Total;430;420;850";
    assert_eq!(regions(&sud_first), expected);
}

#[test]
fn a_total_ranks_the_lines_and_a_measure_is_named_by_its_index() {
    let measures = [Measure::Sum(AMOUNT), Measure::CountStar];
    let regions = |order: &Order| sorted_example(&[REGION], &[], &measures, order);
    assert_eq!(
        regions(&lines_by(&[], 0, false)),
        "region;ca;nb\nSud;380;5\nNord;470;5\nTotal;850;10"
    );
    assert_eq!(
        regions(&lines_by(&[], 1, true)),
        "region;ca;nb\nNord;470;5\nSud;380;5\nTotal;850;10"
    );
}

#[test]
fn the_values_of_a_line_rank_the_columns_and_the_total_stays_last() {
    let measures = [Measure::Sum(AMOUNT)];
    let header = |order: &Order| {
        let table = sorted_example(&[REGION, CITY], &[YEAR], &measures, order);
        table.lines().next().unwrap().to_owned()
    };
    let by_2026 = "region;ville;2026 / ca;2025 / ca;Total / ca";
    let by_2025 = "region;ville;2025 / ca;2026 / ca;Total / ca";
    assert_eq!(header(&columns_by(&["Nord", "Lille"], false)), by_2026);
    assert_eq!(header(&columns_by(&["Nord", "Lille"], true)), by_2025);
    // The empty cell of 2025 comes first in ascending order.
    assert_eq!(header(&columns_by(&["Sud", "Toulon"], false)), by_2025);
    assert_eq!(header(&columns_by(&["Sud", "Toulon"], true)), by_2026);
    // The line of the grand total.
    assert_eq!(header(&columns_by(&[], false)), by_2026);
}

#[test]
fn the_values_of_a_line_rank_the_columns_within_their_group() {
    let table = sorted_example(
        &[REGION],
        &[YEAR, QUARTER],
        &[Measure::Sum(AMOUNT)],
        &columns_by(&["Nord"], false),
    );
    assert_eq!(
        table.lines().next().unwrap(),
        "region;2026 / T2 / ca;2026 / T1 / ca;2026 / Total / ca;\
         2025 / T2 / ca;2025 / T1 / ca;2025 / Total / ca;Total / ca"
    );
}

#[test]
fn a_node_or_a_measure_that_the_table_does_not_hold_ranks_nothing() {
    let measures = [Measure::Sum(AMOUNT)];
    let unsorted = example_table(&[REGION, CITY], &[YEAR], &measures);
    for order in [
        lines_by(&["2030"], 0, true),
        lines_by(&["2025", "T1"], 0, true),
        lines_by(&["2025"], 1, true),
        columns_by(&["Ouest"], true),
    ] {
        assert_eq!(
            sorted_example(&[REGION, CITY], &[YEAR], &measures, &order),
            unsorted
        );
    }
}

#[test]
fn a_sort_changes_the_layout_alone() {
    let engine = example();
    let request = Request {
        rows: vec![REGION.into(), CITY.into()],
        columns: vec![YEAR.into()],
        measures: vec![Measure::Sum(AMOUNT)],
        ..Request::default()
    };
    let mut table = engine.compute(&request).unwrap();
    let before = flat(&engine, &table, &["region", "ville"], &["ca"]);
    table
        .lay_out(&engine, &lines_by(&["2025"], 0, true), &Layout::default())
        .unwrap();
    assert_eq!(table.line_count(), 7);
    assert_eq!(table.column_count(), 3);
    assert_eq!(table.finest_groups(), 7);
    assert_eq!(table.selected_rows(), 10);
    assert_ne!(flat(&engine, &table, &["region", "ville"], &["ca"]), before);
    table
        .lay_out(&engine, &Order::default(), &Layout::default())
        .unwrap();
    assert_eq!(flat(&engine, &table, &["region", "ville"], &["ca"]), before);
}

/// Engine over two columns: a key, then values of family `family`.
fn keyed(cells: &[(&str, Option<&str>)], family: Family) -> Engine {
    let mut store = Store::new(2);
    for &(key, value) in cells {
        store.push_row(&[Some(key), value]);
    }
    Engine::new(store, vec![Family::Other, family])
}

/// Keys of the lines of a table of one measure, in the order of its values.
fn keys_by_value(engine: &Engine, measure: Measure) -> String {
    let request = Request {
        rows: vec![0.into()],
        measures: vec![measure],
        ..Request::default()
    };
    let mut table = engine.compute(&request).unwrap();
    table
        .lay_out(engine, &lines_by(&[], 0, false), &Layout::default())
        .unwrap();
    (0..table.line_count() - 1)
        .map(|line| table.line_path(engine, line)[0].unwrap_or("NULL"))
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn numbers_rank_by_value_and_null_comes_first() {
    let cells = [("a", Some("10.5")), ("b", Some("9.25")), ("c", None)];
    let exact = keyed(&cells, Family::Exact);
    assert_eq!(keys_by_value(&exact, Measure::Sum(1)), "c b a");
    assert_eq!(keys_by_value(&exact, Measure::Avg(1)), "c b a");
    assert_eq!(keys_by_value(&exact, Measure::Count(1)), "c a b");
    let float = keyed(&cells, Family::Float);
    assert_eq!(keys_by_value(&float, Measure::Sum(1)), "c b a");
}

#[test]
fn a_minimum_or_a_maximum_ranks_by_the_order_of_its_comparison() {
    let texts = keyed(
        &[("a", Some("b")), ("b", Some("B")), ("c", None)],
        Family::Other,
    );
    assert_eq!(keys_by_value(&texts, Measure::Min(1.into())), "c b a");
    assert_eq!(keys_by_value(&texts, Measure::Max(1.into())), "c b a");
    let numbers = keyed(&[("a", Some("10")), ("b", Some("9"))], Family::Exact);
    assert_eq!(keys_by_value(&numbers, Measure::Min(1.into())), "b a");
}

#[test]
fn a_path_reads_null_and_the_labels_of_a_precision() {
    let mut store = Store::new(3);
    for (key, value, amount) in [("a", None, "1"), ("b", Some("x"), "5"), ("c", None, "3")] {
        store.push_row(&[Some(key), value, Some(amount)]);
    }
    let engine = Engine::new(store, vec![Family::Other, Family::Other, Family::Exact]);
    let request = Request {
        rows: vec![0.into()],
        columns: vec![1.into()],
        measures: vec![Measure::Sum(2)],
        ..Request::default()
    };
    let mut table = engine.compute(&request).unwrap();
    let mut order = lines_by(&[], 0, false);
    order.rows.values.as_mut().unwrap().path = vec![None];
    table.lay_out(&engine, &order, &Layout::default()).unwrap();
    assert_eq!(
        (0..3)
            .map(|line| table.line_path(&engine, line)[0].unwrap())
            .collect::<Vec<_>>(),
        ["b", "a", "c"]
    );

    let engine = example();
    let request = Request {
        rows: vec![REGION.into()],
        columns: vec![Level {
            column: DATE,
            precision: Some(Precision::Month),
        }],
        measures: vec![Measure::Sum(AMOUNT)],
        ..Request::default()
    };
    let mut table = engine.compute(&request).unwrap();
    table
        .lay_out(
            &engine,
            &lines_by(&["2026-02"], 0, false),
            &Layout::default(),
        )
        .unwrap();
    assert_eq!(table.line_path(&engine, 0), [Some("Sud")]);
}

#[test]
fn a_column_can_be_a_level_several_times() {
    // On one axis: each value of the second level falls under its own value.
    assert_eq!(
        example_table(&[REGION, REGION], &[], &[Measure::Sum(AMOUNT)]),
        "region;region;ca\nNord;Total;470\nNord;Nord;470\nSud;Total;380\nSud;Sud;380\nTotal;;850"
    );
    // On both axes, at two precisions: the months of a year fill that year alone.
    let engine = example();
    let request = Request {
        rows: vec![Level {
            column: DATE,
            precision: Some(Precision::Year),
        }],
        columns: vec![Level {
            column: DATE,
            precision: Some(Precision::Quarter),
        }],
        measures: vec![Measure::Sum(AMOUNT)],
        ..Request::default()
    };
    assert_eq!(
        flat(&engine, &engine.compute(&request).unwrap(), &["a"], &["m"]),
        concat!(
            "a;2025-T1 / m;2025-T2 / m;2026-T1 / m;2026-T2 / m;Total / m\n",
            "2025;220;210;;;430\n",
            "2026;;;290;130;420\n",
            "Total;220;210;290;130;850"
        )
    );
}

/// `SUM(montant)` of the rows of the levels given, compared to an operand.
fn amount_above(levels: &[usize], operand: &str) -> Filter {
    Filter::Measure {
        measure: Measure::Sum(AMOUNT),
        levels: levels.iter().map(|&column| column.into()).collect(),
        test: Test::Compare(Comparison::Greater, operand.to_owned()),
    }
}

/// Filter `col IN (…)` on the values of a column, as they are.
fn listed(column: usize, values: &[Option<&str>]) -> Filter {
    Filter::Values {
        level: column.into(),
        values: values
            .iter()
            .map(|value| value.map(str::to_owned))
            .collect(),
        excluded: false,
    }
}

/// Filter `col NOT IN (…)` on the values of a column, as they are.
fn left_out(column: usize, values: &[Option<&str>]) -> Filter {
    Filter::Values {
        level: column.into(),
        values: values
            .iter()
            .map(|value| value.map(str::to_owned))
            .collect(),
        excluded: true,
    }
}

/// Table of `SUM(montant)` of the example dataset under some filters, line by
/// line.
fn filtered(rows: &[usize], columns: &[usize], filters: Vec<Filter>) -> String {
    let engine = example();
    let request = Request {
        rows: rows.iter().map(|&column| column.into()).collect(),
        columns: columns.iter().map(|&column| column.into()).collect(),
        measures: vec![Measure::Sum(AMOUNT)],
        filters,
    };
    let table = engine.compute(&request).unwrap();
    let levels = vec!["level"; rows.len()];
    flat(&engine, &table, &levels, &["ca"])
        .lines()
        .skip(1)
        .collect::<Vec<_>>()
        .join("|")
}

#[test]
fn a_filter_on_a_measure_at_the_deepest_level_gives_a_table_that_adds_up() {
    // F-44: Arras (100) and Toulon (30) leave, and the totals count the rest.
    assert_eq!(
        filtered(
            &[REGION, CITY],
            &[],
            vec![amount_above(&[REGION, CITY], "100")]
        ),
        "Nord;Total;370|Nord;Lille;370|Sud;Total;350|Sud;Nice;350|Total;;720"
    );
}

#[test]
fn a_filter_on_a_measure_at_an_outer_level_keeps_the_whole_group() {
    // F-44: Nord (470) stays with all its cities, Sud (380) leaves.
    assert_eq!(
        filtered(&[REGION, CITY], &[], vec![amount_above(&[REGION], "400")]),
        "Nord;Total;470|Nord;Arras;100|Nord;Lille;370|Total;;470"
    );
}

#[test]
fn a_filter_on_a_measure_applies_to_a_level_of_the_columns() {
    // F-44: 2025 (430) stays, 2026 (420) leaves.
    assert_eq!(
        filtered(&[REGION], &[YEAR], vec![amount_above(&[YEAR], "425")]),
        "Nord;260;260|Sud;170;170|Total;430;430"
    );
}

#[test]
fn a_filter_on_a_measure_without_a_level_reads_all_the_rows() {
    assert_eq!(
        filtered(&[REGION], &[], vec![amount_above(&[], "849")]),
        "Nord;470|Sud;380|Total;850"
    );
    assert_eq!(
        filtered(&[REGION], &[], vec![amount_above(&[], "850")]),
        "Total;NULL"
    );
}

#[test]
fn a_filter_on_values_keeps_the_rows_of_its_values() {
    assert_eq!(
        filtered(
            &[CITY],
            &[],
            vec![listed(REGION, &[Some("Sud"), Some("Absent")])]
        ),
        "Nice;350|Toulon;30|Total;380"
    );
    assert_eq!(
        filtered(&[CITY], &[], vec![listed(REGION, &[])]),
        "Total;NULL"
    );
}

#[test]
fn a_filter_on_values_can_keep_null() {
    let engine = single(&[Some("a"), None, Some("b")], Family::Other);
    let rows_of = |values: &[Option<&str>]| {
        let request = Request {
            rows: vec![0.into()],
            filters: vec![listed(0, values)],
            ..Request::default()
        };
        engine.compute(&request).unwrap().selected_rows()
    };
    assert_eq!(rows_of(&[None]), 1);
    assert_eq!(rows_of(&[Some("a")]), 1);
    assert_eq!(rows_of(&[Some("b"), None]), 2);
}

#[test]
fn filters_combine_by_and_and_read_the_same_rows() {
    // The filter on values first: among the cities of Sud, Nice stays.
    let south = listed(REGION, &[Some("Sud")]);
    assert_eq!(
        filtered(
            &[CITY],
            &[],
            vec![amount_above(&[CITY], "100"), south.clone()]
        ),
        "Nice;350|Total;350"
    );
    // Two filters on measures read the same rows: each gives its groups, then
    // both must accept a row. Nord and Lille alone pass both.
    assert_eq!(
        filtered(
            &[REGION, CITY],
            &[],
            vec![amount_above(&[REGION], "400"), amount_above(&[CITY], "100")]
        ),
        "Nord;Total;370|Nord;Lille;370|Total;;370"
    );
}

#[test]
fn a_filter_keeps_the_rows_the_filter_of_the_grid_keeps() {
    let mut engine = example();
    // Without the first row of Lille (100), Lille is at 270.
    engine.set_kept(Some((1..10).collect())).unwrap();
    let request = Request {
        rows: vec![CITY.into()],
        measures: vec![Measure::Sum(AMOUNT)],
        filters: vec![amount_above(&[CITY], "300")],
        ..Request::default()
    };
    assert_eq!(
        flat(&engine, &engine.compute(&request).unwrap(), &["v"], &["ca"]),
        "v;ca\nNice;350\nTotal;350"
    );
    assert_eq!(engine.compute(&request).unwrap().selected_rows(), 4);
}

/// Lengths of the texts of each column of a table, read cell by cell: what
/// `Table::text_lengths` gives by reading the groups. A NULL counts 4
/// characters, a total 5.
fn read_lengths(engine: &Engine, table: &Table) -> Vec<TextLengths> {
    (0..table.column_count())
        .map(|column| {
            let header = table.header(engine, column);
            let label = match header.path.last() {
                _ if header.total => 5,
                Some(&(_, label)) => label.map_or(4, |text| text.chars().count()),
                None => 0,
            };
            let values = (0..table.line_count())
                .map(|line| match table.value(line, column) {
                    Value::Null => 4,
                    value => value.text(engine).map_or(0, |text| text.chars().count()),
                })
                .max()
                .unwrap_or(0);
            TextLengths { values, label }
        })
        .collect()
}

#[test]
fn the_lengths_of_the_texts_of_the_columns_are_those_of_their_cells() {
    let engine = example();
    let request = Request {
        rows: vec![0.into(), 1.into()],
        columns: vec![3.into(), 4.into()],
        measures: vec![
            Measure::Sum(5),
            Measure::Max(2.into()),
            Measure::Avg(5),
            Measure::CountStar,
        ],
        ..Request::default()
    };
    let mut table = engine.compute(&request).unwrap();
    assert_eq!(
        table.text_lengths(&engine, 4, 5),
        read_lengths(&engine, &table)
    );
    // A layout hides lines and columns, and another order moves the columns.
    let hidden = Totals {
        grand_total: false,
        subtotals: vec![false, false],
    };
    for (form, rows, columns) in [
        (Form::Tabular, hidden.clone(), Totals::default()),
        (Form::Outline, Totals::default(), hidden.clone()),
    ] {
        let order = Order {
            columns: AxisOrder {
                descending: vec![true, true],
                values: None,
            },
            ..Order::default()
        };
        let layout = Layout {
            form,
            rows,
            columns,
        };
        table.lay_out(&engine, &order, &layout).unwrap();
        assert_eq!(
            table.text_lengths(&engine, 4, 5),
            read_lengths(&engine, &table)
        );
    }
}

#[test]
fn the_longest_text_of_a_level_bounds_its_cells_but_not_a_null() {
    let engine = pairs(
        &[("a", Some("xy")), ("b", Some("longest value")), ("c", None)],
        Family::Other,
    );
    // The column a never reaches the longest text of the level.
    let by_columns = Request {
        columns: vec![0.into()],
        measures: vec![Measure::Min(1.into())],
        ..Request::default()
    };
    let table = engine.compute(&by_columns).unwrap();
    let lengths = table.text_lengths(&engine, 4, 5);
    assert_eq!(lengths, read_lengths(&engine, &table));
    let values: Vec<usize> = lengths.iter().map(|lengths| lengths.values).collect();
    assert_eq!(values, [2, 13, 4, 13]);
    // A NULL longer than the longest text counts once the column reaches it.
    let by_lines = Request {
        rows: vec![0.into()],
        measures: vec![Measure::Min(1.into())],
        ..Request::default()
    };
    let table = engine.compute(&by_lines).unwrap();
    assert_eq!(table.text_lengths(&engine, 20, 5)[0].values, 20);
}

#[test]
fn floats_and_periods_give_the_lengths_of_their_cells() {
    let floats = pairs(
        &[
            ("a", Some("0.1")),
            ("a", Some("0.2")),
            ("b", Some("-1e300")),
            ("c", None),
        ],
        Family::Float,
    );
    let dates = pairs(
        &[
            ("a", Some("2026-01-15")),
            ("b", Some("2025-12-31")),
            ("b", Some("1999-07-04")),
            ("c", None),
        ],
        Family::Date,
    );
    let month = Level {
        column: 1,
        precision: Some(Precision::Month),
    };
    for (engine, measures) in [
        (&floats, vec![Measure::Sum(1), Measure::Avg(1)]),
        (&dates, vec![Measure::Min(month), Measure::Max(1.into())]),
    ] {
        for (rows, columns) in [(vec![0.into()], vec![]), (vec![], vec![0.into()])] {
            let request = Request {
                rows,
                columns,
                measures: measures.clone(),
                ..Request::default()
            };
            let table = engine.compute(&request).unwrap();
            assert_eq!(
                table.text_lengths(engine, 4, 5),
                read_lengths(engine, &table)
            );
        }
    }
}

#[test]
fn a_null_and_a_total_count_the_lengths_the_page_gives_them() {
    let engine = pairs(
        &[("a", None), ("bb", Some("-12.50")), ("bb", Some("3"))],
        Family::Exact,
    );
    let request = Request {
        rows: vec![1.into()],
        columns: vec![0.into()],
        measures: vec![Measure::Sum(1)],
        ..Request::default()
    };
    let table = engine.compute(&request).unwrap();
    // Columns a (NULL on its line and its total), bb (-12.50, 3.00, -9.50),
    // then the grand total (NULL, -12.50, 3.00, -9.50).
    assert_eq!(
        table.text_lengths(&engine, 9, 7),
        [
            TextLengths {
                values: 9,
                label: 1
            },
            TextLengths {
                values: 6,
                label: 2
            },
            TextLengths {
                values: 9,
                label: 7
            },
        ]
    );
    // Without any level of columns, the header has no label.
    let request = Request {
        rows: vec![0.into()],
        measures: vec![Measure::CountStar],
        ..Request::default()
    };
    let table = engine.compute(&request).unwrap();
    assert_eq!(
        table.text_lengths(&engine, 4, 5),
        [TextLengths {
            values: 1,
            label: 0
        }]
    );
}

/// Engine over two columns: a text, and numbers of a family.
fn pairs(rows: &[(&str, Option<&str>)], family: Family) -> Engine {
    let mut store = Store::new(2);
    for &(key, value) in rows {
        store.push_row(&[Some(key), value]);
    }
    Engine::new(store, vec![Family::Other, family])
}

/// Keys of the rows a filter on a measure of column 1 keeps, grouped by column 0.
fn kept_keys(
    engine: &Engine,
    measure: Measure,
    comparison: Comparison,
    operand: &str,
) -> Result<Vec<String>, Error> {
    let request = Request {
        rows: vec![0.into()],
        filters: vec![Filter::Measure {
            measure,
            levels: vec![0.into()],
            test: Test::Compare(comparison, operand.to_owned()),
        }],
        ..Request::default()
    };
    labels_of(engine, &request)
}

/// Labels of the lines of a table, without its grand total, `NULL` for NULL.
fn labels_of(engine: &Engine, request: &Request) -> Result<Vec<String>, Error> {
    let table = engine.compute(request)?;
    Ok((0..table.line_count() - 1)
        .map(|line| match table.line(engine, line).label {
            Label::Value(value) => value.unwrap_or("NULL").to_owned(),
            Label::Total => "Total".to_owned(),
        })
        .collect())
}

#[test]
fn each_comparison_keeps_its_groups() {
    let engine = pairs(
        &[("a", Some("1")), ("b", Some("2")), ("c", Some("3"))],
        Family::Exact,
    );
    for (comparison, keys) in [
        (Comparison::Equal, vec!["b"]),
        (Comparison::NotEqual, vec!["a", "c"]),
        (Comparison::Less, vec!["a"]),
        (Comparison::LessOrEqual, vec!["a", "b"]),
        (Comparison::Greater, vec!["c"]),
        (Comparison::GreaterOrEqual, vec!["b", "c"]),
    ] {
        assert_eq!(
            kept_keys(&engine, Measure::Sum(1), comparison, "2").unwrap(),
            keys,
            "{comparison:?}"
        );
    }
}

#[test]
fn an_operand_compares_with_a_number_at_another_scale() {
    let engine = pairs(&[("a", Some("1.50")), ("b", Some("2.25"))], Family::Exact);
    let keys = |operand| kept_keys(&engine, Measure::Sum(1), Comparison::Greater, operand);
    assert_eq!(keys("1.5").unwrap(), ["b"]);
    assert_eq!(keys("1.499").unwrap(), ["a", "b"]);
    assert_eq!(keys("2").unwrap(), ["b"]);
    // An average has four more digits than its column.
    let keys = kept_keys(&engine, Measure::Avg(1), Comparison::Equal, "2.25").unwrap();
    assert_eq!(keys, ["b"]);
}

#[test]
fn a_group_whose_measure_is_null_passes_no_comparison() {
    let engine = pairs(&[("a", None), ("b", Some("2"))], Family::Exact);
    for comparison in [
        Comparison::NotEqual,
        Comparison::GreaterOrEqual,
        Comparison::Less,
    ] {
        let keys = kept_keys(&engine, Measure::Sum(1), comparison, "0").unwrap();
        assert!(!keys.contains(&"a".to_owned()), "{comparison:?}");
    }
    assert_eq!(
        kept_keys(&engine, Measure::Min(1.into()), Comparison::NotEqual, "0").unwrap(),
        ["b"]
    );
}

#[test]
fn a_count_compares_with_a_number_whatever_its_column() {
    let engine = pairs(
        &[("a", Some("x")), ("a", Some("y")), ("b", None)],
        Family::Other,
    );
    let keys = |measure, operand| kept_keys(&engine, measure, Comparison::Equal, operand);
    assert_eq!(keys(Measure::CountStar, "1").unwrap(), ["b"]);
    assert_eq!(keys(Measure::Count(1), "0").unwrap(), ["b"]);
    assert_eq!(
        keys(Measure::CountDistinct(1.into()), "2.0").unwrap(),
        ["a"]
    );
    assert_eq!(
        keys(Measure::CountStar, "x"),
        Err(Error::FilterOperand { filter: 0 })
    );
}

#[test]
fn floats_compare_as_floats_and_nan_passes_nothing() {
    let engine = pairs(
        &[("a", Some("1.5")), ("b", Some("NaN")), ("c", Some("1e3"))],
        Family::Float,
    );
    let keys = |measure, comparison, operand| kept_keys(&engine, measure, comparison, operand);
    assert_eq!(
        keys(Measure::Sum(1), Comparison::Greater, "1e2").unwrap(),
        ["c"]
    );
    assert_eq!(
        keys(Measure::Sum(1), Comparison::NotEqual, "0").unwrap(),
        ["a", "c"]
    );
    assert_eq!(
        keys(Measure::Max(1.into()), Comparison::Less, "2").unwrap(),
        ["a"]
    );
    assert_eq!(
        keys(Measure::Sum(1), Comparison::Equal, "x"),
        Err(Error::FilterOperand { filter: 0 })
    );
}

#[test]
fn an_extreme_compares_as_its_column() {
    let engine = pairs(
        &[("a", Some("9")), ("a", Some("10")), ("b", Some("2"))],
        Family::Exact,
    );
    // By value: 10, and not 9, is the largest of a.
    assert_eq!(
        kept_keys(&engine, Measure::Max(1.into()), Comparison::Equal, "10").unwrap(),
        ["a"]
    );
    let engine = pairs(&[("a", Some("Zoé")), ("b", Some("éa"))], Family::Other);
    // By code points: `é` comes after `Z`.
    assert_eq!(
        kept_keys(&engine, Measure::Min(1.into()), Comparison::Greater, "Zz").unwrap(),
        ["b"]
    );
    assert_eq!(
        kept_keys(&engine, Measure::Min(1.into()), Comparison::Equal, "Zoé").unwrap(),
        ["a"]
    );
}

#[test]
fn the_errors_of_a_filter_name_it() {
    let engine = pairs(&[("a", Some("x"))], Family::Other);
    let error = kept_keys(&engine, Measure::Sum(1), Comparison::Equal, "1").unwrap_err();
    assert_eq!(error, Error::FilterNotNumeric { filter: 0 });
    assert_eq!(error.to_string(), "filter 0: column without numbers");

    let huge = "99999999999999999999999999999999999999";
    let engine = pairs(&[("a", Some(huge)), ("a", Some(huge))], Family::Exact);
    let error = kept_keys(&engine, Measure::Sum(1), Comparison::Equal, "1").unwrap_err();
    assert_eq!(error, Error::FilterOverflow { filter: 0 });
}

#[test]
fn a_filter_on_values_leaves_out_a_date_outside_of_the_calendar() {
    let cells = [Some("2026-01-15"), Some("0000-00-00")];
    let engine = single(&cells, Family::Other);
    let by_month = |filters| Request {
        rows: vec![Level {
            column: 0,
            precision: Some(Precision::Month),
        }],
        filters,
        ..Request::default()
    };
    assert_eq!(
        engine.compute(&by_month(vec![])).unwrap_err(),
        Error::InvalidDate { column: 0, row: 1 }
    );
    let only = |value| vec![listed(0, &[Some(value)])];
    assert_eq!(
        engine
            .compute(&by_month(only("2026-01-15")))
            .unwrap()
            .line_count(),
        2
    );
    assert_eq!(
        engine.compute(&by_month(only("0000-00-00"))).unwrap_err(),
        Error::InvalidDate { column: 0, row: 1 }
    );
}

/// Text of an operand.
fn text(value: &str) -> String {
    value.to_owned()
}

/// Values of a column of cells of a family that a test keeps, as its own level.
fn tested(cells: &[Option<&str>], family: Family, test: Test) -> Result<Vec<String>, Error> {
    let request = Request {
        rows: vec![0.into()],
        filters: vec![Filter::Column {
            level: 0.into(),
            test,
        }],
        ..Request::default()
    };
    labels_of(&single(cells, family), &request)
}

#[test]
fn a_filter_on_a_column_compares_numbers_by_value() {
    let cells = [Some("9"), Some("10"), Some("1.50"), None];
    let keys = |comparison, operand: &str| {
        tested(
            &cells,
            Family::Exact,
            Test::Compare(comparison, text(operand)),
        )
        .unwrap()
    };
    assert_eq!(keys(Comparison::Greater, "9"), ["10"]);
    assert_eq!(keys(Comparison::GreaterOrEqual, "9"), ["10", "9"]);
    assert_eq!(keys(Comparison::Less, "9"), ["1.50"]);
    assert_eq!(keys(Comparison::LessOrEqual, "1.5"), ["1.50"]);
    assert_eq!(keys(Comparison::Equal, "1.5"), ["1.50"]);
    // NULL gives neither true nor false, under `<>` too.
    assert_eq!(keys(Comparison::NotEqual, "9"), ["1.50", "10"]);
}

#[test]
fn between_includes_its_bounds_and_null_only_passes_is_null() {
    let cells = [Some("1"), Some("5"), Some("10"), None];
    let keys = |test| tested(&cells, Family::Exact, test).unwrap();
    assert_eq!(keys(Test::Between(text("1"), text("5"))), ["1", "5"]);
    assert_eq!(keys(Test::NotBetween(text("1"), text("5"))), ["10"]);
    // Bounds in the wrong order: nothing is between them.
    assert!(keys(Test::Between(text("10"), text("1"))).is_empty());
    assert_eq!(
        keys(Test::NotBetween(text("10"), text("1"))),
        ["1", "10", "5"]
    );
    assert_eq!(keys(Test::IsNull), ["NULL"]);
    assert_eq!(keys(Test::IsNotNull), ["1", "10", "5"]);
}

#[test]
fn a_float_compares_as_a_float_and_nan_passes_no_comparison() {
    let cells = [Some("1.5"), Some("NaN"), Some("1e3"), None];
    let keys = |test| tested(&cells, Family::Float, test).unwrap();
    assert_eq!(
        keys(Test::Compare(Comparison::Greater, text("1"))),
        ["1.5", "1e3"]
    );
    assert_eq!(keys(Test::NotBetween(text("0"), text("2"))), ["1e3"]);
    assert_eq!(keys(Test::IsNotNull), ["1.5", "1e3", "NaN"]);
}

#[test]
fn a_text_compares_by_code_points() {
    let cells = [Some("Zoé"), Some("éa"), Some("a"), None];
    let keys = |test| tested(&cells, Family::Other, test).unwrap();
    assert_eq!(
        keys(Test::Compare(Comparison::Greater, text("Zz"))),
        ["a", "éa"]
    );
    assert_eq!(keys(Test::Between(text("Z"), text("a"))), ["Zoé", "a"]);
}

#[test]
fn a_cell_that_is_no_number_stops_a_comparison_when_its_row_stays() {
    let huge = "9".repeat(40);
    let cells = [Some("1"), Some("x"), Some(huge.as_str())];
    let mut engine = single(&cells, Family::Exact);
    let request = Request {
        rows: vec![0.into()],
        filters: vec![Filter::Column {
            level: 0.into(),
            test: Test::Compare(Comparison::Greater, text("0")),
        }],
        ..Request::default()
    };
    assert_eq!(
        engine.compute(&request).unwrap_err(),
        Error::Number {
            column: 0,
            error: NumberError::Invalid { row: 1 },
        }
    );
    engine.set_kept(Some(vec![0, 2])).unwrap();
    assert_eq!(
        engine.compute(&request).unwrap_err(),
        Error::Number {
            column: 0,
            error: NumberError::TooLarge { row: 2 },
        }
    );
    engine.set_kept(Some(vec![0])).unwrap();
    assert_eq!(labels_of(&engine, &request).unwrap(), ["1"]);
    assert_eq!(
        tested(
            &cells,
            Family::Exact,
            Test::Compare(Comparison::Equal, text("x"))
        ),
        Err(Error::FilterOperand { filter: 0 })
    );
}

#[test]
fn a_list_can_leave_out_its_values_as_not_in_does() {
    let engine = single(&[Some("a"), Some("b"), None], Family::Other);
    let keys = |values: &[Option<&str>]| {
        labels_of(
            &engine,
            &Request {
                rows: vec![0.into()],
                filters: vec![left_out(0, values)],
                ..Request::default()
            },
        )
        .unwrap()
    };
    assert_eq!(keys(&[Some("a")]), ["b"]);
    assert!(keys(&[None]).is_empty());
    assert!(keys(&[Some("a"), None]).is_empty());
}

#[test]
fn a_date_compares_in_the_order_of_time_and_stops_outside_of_the_calendar() {
    let cells = [
        Some("2026-01-15"),
        Some("2026-03-01"),
        Some("0000-00-00"),
        None,
    ];
    let january = Filter::Column {
        level: 0.into(),
        test: Test::Between(text("2026-01-01"), text("2026-01-31")),
    };
    let engine = single(&cells, Family::Date);
    let keys = |filters| {
        labels_of(
            &engine,
            &Request {
                rows: vec![0.into()],
                filters,
                ..Request::default()
            },
        )
    };
    assert_eq!(
        keys(vec![january.clone()]),
        Err(Error::InvalidDate { column: 0, row: 2 })
    );
    // A list without a precision comes first, wherever it stands: it leaves out
    // the cell that the comparison cannot read.
    let without_zero = left_out(0, &[Some("0000-00-00")]);
    assert_eq!(
        keys(vec![january.clone(), without_zero.clone()]).unwrap(),
        ["2026-01-15"]
    );
    assert_eq!(keys(vec![without_zero, january]).unwrap(), ["2026-01-15"]);
    // `IS NULL` reads no date.
    assert_eq!(
        tested(&cells, Family::Date, Test::IsNull).unwrap(),
        ["NULL"]
    );
    assert_eq!(
        tested(
            &cells,
            Family::Date,
            Test::Compare(Comparison::Less, text("2026-02-30"))
        ),
        Err(Error::FilterOperand { filter: 0 })
    );
}

#[test]
fn an_instant_compares_with_a_whole_instant_whatever_the_zeros_of_its_fraction() {
    let cells = [
        Some("2026-01-15 10:30:00.500"),
        Some("2026-01-15 23:59:59.000"),
    ];
    let keys = |test| tested(&cells, Family::Datetime, test);
    assert_eq!(
        keys(Test::Compare(
            Comparison::Equal,
            text("2026-01-15 10:30:00.5")
        ))
        .unwrap(),
        ["2026-01-15 10:30:00.500"]
    );
    assert_eq!(
        keys(Test::Compare(
            Comparison::Equal,
            text("2026-01-15 23:59:59")
        ))
        .unwrap(),
        ["2026-01-15 23:59:59.000"]
    );
    assert_eq!(
        keys(Test::Between(text("2026-01-15"), text("2026-01-16"))),
        Err(Error::FilterOperand { filter: 0 })
    );
}

#[test]
fn an_hour_outside_of_the_day_stops_a_comparison() {
    let cells = [Some("10:30:00"), Some("25:00:00")];
    let keys = |test| tested(&cells, Family::Time, test);
    assert_eq!(
        keys(Test::Compare(Comparison::Less, text("11:00:00"))),
        Err(Error::InvalidDate { column: 0, row: 1 })
    );
    assert_eq!(
        keys(Test::Compare(Comparison::Less, text("25:00:00"))),
        Err(Error::FilterOperand { filter: 0 })
    );
    assert_eq!(keys(Test::IsNotNull).unwrap(), ["10:30:00", "25:00:00"]);
}

#[test]
fn a_filter_reads_a_precision_of_the_dates() {
    let cells = [
        Some("2026-01-15 10:00:00"),
        Some("2026-03-31 23:59:59"),
        Some("2026-04-01 00:00:00"),
        Some("2027-01-01 00:00:00"),
        None,
    ];
    let engine = single(&cells, Family::Datetime);
    let at = |precision| Level {
        column: 0,
        precision: Some(precision),
    };
    let rows_kept = |filter| {
        let request = Request {
            rows: vec![0.into()],
            filters: vec![filter],
            ..Request::default()
        };
        engine.compute(&request).map(|table| table.selected_rows())
    };
    let month = at(Precision::Month);
    // March keeps all its days, up to its last second.
    assert_eq!(
        rows_kept(Filter::Column {
            level: month,
            test: Test::Between(text("2026-01"), text("2026-03")),
        }),
        Ok(2)
    );
    // The ISO year of 2027-01-01 is 2026.
    assert_eq!(
        rows_kept(Filter::Column {
            level: at(Precision::Year),
            test: Test::Compare(Comparison::Equal, text("2026")),
        }),
        Ok(4)
    );
    assert_eq!(
        rows_kept(Filter::Column {
            level: at(Precision::Quarter),
            test: Test::Compare(Comparison::LessOrEqual, text("2026-T1")),
        }),
        Ok(2)
    );
    assert_eq!(
        rows_kept(Filter::Values {
            level: month,
            values: vec![Some(text("2026-04")), None],
            excluded: false,
        }),
        Ok(2)
    );
    assert_eq!(
        rows_kept(Filter::Values {
            level: month,
            values: vec![Some(text("2026-04"))],
            excluded: true,
        }),
        Ok(3)
    );
    assert_eq!(
        rows_kept(Filter::Column {
            level: month,
            test: Test::IsNull,
        }),
        Ok(1)
    );
    assert_eq!(
        rows_kept(Filter::Column {
            level: month,
            test: Test::Compare(Comparison::Equal, text("2026-1")),
        }),
        Err(Error::FilterOperand { filter: 0 })
    );
}

#[test]
fn a_precision_of_the_time_groups_and_filters_the_instants() {
    let engine = pairs(
        &[
            ("a", Some("2026-01-15 10:59:59.999999")),
            ("b", Some("2026-01-15 10:00:00.000000")),
            ("c", Some("2026-01-15 11:30:15.500000")),
            ("d", Some("2026-01-15 11:30:15.250000")),
        ],
        Family::Datetime,
    );
    let at = |precision| Level {
        column: 1,
        precision: Some(precision),
    };
    let request = Request {
        rows: vec![at(Precision::Hour)],
        measures: vec![Measure::CountStar, Measure::Max(at(Precision::Millisecond))],
        ..Request::default()
    };
    let table = engine.compute(&request).unwrap();
    let lines: Vec<String> = (0..table.line_count())
        .map(|line| {
            let label = table.line_path(&engine, line).first().copied().flatten();
            let values = (0..table.column_count())
                .map(|column| table.value(line, column).text(&engine).unwrap_or_default());
            std::iter::once(label.unwrap_or("Total").to_owned())
                .chain(values)
                .collect::<Vec<_>>()
                .join(";")
        })
        .collect();
    // Truncated: 10:59:59.999999 stays in 10 h, and gives 10:59:59.999.
    assert_eq!(
        lines,
        [
            "2026-01-15 10:00:00;2;2026-01-15 10:59:59.999",
            "2026-01-15 11:00:00;2;2026-01-15 11:30:15.500",
            "Total;4;2026-01-15 11:30:15.500",
        ]
    );
    let kept = |filter| {
        let request = Request {
            rows: vec![0.into()],
            filters: vec![filter],
            ..Request::default()
        };
        engine.compute(&request).map(|table| table.selected_rows())
    };
    assert_eq!(
        kept(Filter::Column {
            level: at(Precision::Minute),
            test: Test::Compare(Comparison::Equal, text("2026-01-15 11:30:00")),
        }),
        Ok(2)
    );
    assert_eq!(
        kept(Filter::Column {
            level: at(Precision::Second),
            test: Test::Between(text("2026-01-15 10:00:00"), text("2026-01-15 10:59:59")),
        }),
        Ok(2)
    );
    // A minute is no hour.
    assert_eq!(
        kept(Filter::Column {
            level: at(Precision::Hour),
            test: Test::Compare(Comparison::Equal, text("2026-01-15 11:30:00")),
        }),
        Err(Error::FilterOperand { filter: 0 })
    );
}

#[test]
fn a_precision_stops_a_filter_on_a_cell_outside_of_the_calendar_unless_a_list_leaves_it_out() {
    let cells = [Some("2026-01-15"), Some("0000-00-00"), None];
    let engine = single(&cells, Family::Date);
    let month = Level {
        column: 0,
        precision: Some(Precision::Month),
    };
    let keys = |filters| {
        labels_of(
            &engine,
            &Request {
                rows: vec![0.into()],
                filters,
                ..Request::default()
            },
        )
    };
    let is_null = Filter::Column {
        level: month,
        test: Test::IsNull,
    };
    assert_eq!(
        keys(vec![is_null.clone()]),
        Err(Error::InvalidDate { column: 0, row: 1 })
    );
    assert_eq!(
        keys(vec![Filter::Values {
            level: month,
            values: vec![Some(text("2026-01"))],
            excluded: false,
        }]),
        Err(Error::InvalidDate { column: 0, row: 1 })
    );
    assert_eq!(
        keys(vec![is_null, left_out(0, &[Some("0000-00-00")])]),
        Ok(vec![])
    );
}

/// Keys of the rows a test of a measure of column 1 keeps, grouped by column 0.
fn kept_by_test(engine: &Engine, measure: Measure, test: Test) -> Result<Vec<String>, Error> {
    labels_of(
        engine,
        &Request {
            rows: vec![0.into()],
            filters: vec![Filter::Measure {
                measure,
                levels: vec![0.into()],
                test,
            }],
            ..Request::default()
        },
    )
}

#[test]
fn a_filter_on_a_measure_takes_each_test() {
    let engine = pairs(
        &[
            ("a", Some("1")),
            ("b", Some("5")),
            ("c", Some("10")),
            ("d", None),
        ],
        Family::Exact,
    );
    let keys = |test| kept_by_test(&engine, Measure::Sum(1), test).unwrap();
    assert_eq!(keys(Test::Between(text("1"), text("5"))), ["a", "b"]);
    assert_eq!(keys(Test::NotBetween(text("1"), text("5"))), ["c"]);
    assert!(keys(Test::Between(text("10"), text("1"))).is_empty());
    assert_eq!(keys(Test::IsNull), ["d"]);
    assert_eq!(keys(Test::IsNotNull), ["a", "b", "c"]);
}

#[test]
fn an_extreme_outside_of_the_calendar_stops_a_comparison() {
    let engine = pairs(
        &[("a", Some("0000-00-00")), ("b", Some("2026-01-15"))],
        Family::Date,
    );
    let keys = |test| kept_by_test(&engine, Measure::Min(1.into()), test);
    assert_eq!(
        keys(Test::Compare(Comparison::Greater, text("2026-01-01"))),
        Err(Error::InvalidDate { column: 1, row: 0 })
    );
    assert_eq!(keys(Test::IsNotNull).unwrap(), ["a", "b"]);
    assert_eq!(
        keys(Test::Compare(Comparison::Greater, text("abc"))),
        Err(Error::FilterOperand { filter: 0 })
    );
}

#[test]
fn the_values_of_a_list_can_be_the_periods_of_a_precision() {
    let cells = [
        Some("2026-03-15"),
        Some("0000-00-00"),
        None,
        Some("2026-01-15"),
    ];
    let engine = single(&cells, Family::Date);
    let month = Level {
        column: 0,
        precision: Some(Precision::Month),
    };
    assert_eq!(
        engine.distinct_at(month).unwrap(),
        [None, Some("2026-01"), Some("2026-03")]
    );
    assert_eq!(engine.distinct_at(0.into()).unwrap(), cells);
    assert_eq!(
        engine.distinct_at(9.into()).unwrap_err(),
        Error::UnknownColumn(9)
    );
}

#[test]
fn a_date_column_is_no_column_of_numbers() {
    let engine = pairs(&[("a", Some("2026-01-15"))], Family::Date);
    let request = Request {
        measures: vec![Measure::Sum(1)],
        ..Request::default()
    };
    assert_eq!(
        engine.compute(&request).unwrap_err(),
        Error::NotNumeric { measure: 0 }
    );
    let extreme = Request {
        measures: vec![Measure::Max(1.into())],
        ..Request::default()
    };
    assert_eq!(
        flat(&engine, &engine.compute(&extreme).unwrap(), &[], &["m"]),
        "m\n2026-01-15"
    );
}

/// Layout in a form, with the totals of the rows and every total of the columns.
fn rows_layout(form: Form, grand_total: bool, subtotals: &[bool]) -> Layout {
    Layout {
        form,
        rows: Totals {
            grand_total,
            subtotals: subtotals.to_vec(),
        },
        columns: Totals::default(),
    }
}

/// Sum of the amounts by region and by town, in a layout.
fn towns_in(layout: &Layout) -> String {
    arranged_example(
        &[REGION, CITY],
        &[],
        &[Measure::Sum(AMOUNT)],
        &Order::default(),
        layout,
    )
}

#[test]
fn compact_and_outline_forms_show_a_group_above_its_members() {
    let expected = "\
region;ville;ca
Nord;Total;470
Nord;Arras;100
Nord;Lille;370
Sud;Total;380
Sud;Nice;350
Sud;Toulon;30
Total;;850";
    assert_eq!(towns_in(&Layout::default()), expected);
    assert_eq!(towns_in(&rows_layout(Form::Outline, true, &[])), expected);
}

#[test]
fn tabular_form_shows_a_subtotal_below_its_members() {
    assert_eq!(
        towns_in(&rows_layout(Form::Tabular, true, &[])),
        "\
region;ville;ca
Nord;Arras;100
Nord;Lille;370
Nord;Total;470
Sud;Nice;350
Sud;Toulon;30
Sud;Total;380
Total;;850"
    );
}

#[test]
fn a_group_without_its_subtotal_keeps_an_empty_line_above_its_members() {
    let expected = "\
region;ville;ca
Nord;Total;
Nord;Arras;100
Nord;Lille;370
Sud;Total;
Sud;Nice;350
Sud;Toulon;30
Total;;850";
    for form in [Form::Compact, Form::Outline] {
        assert_eq!(towns_in(&rows_layout(form, true, &[false])), expected);
    }
}

#[test]
fn tabular_form_leaves_a_group_without_its_subtotal_out() {
    assert_eq!(
        towns_in(&rows_layout(Form::Tabular, true, &[false])),
        "\
region;ville;ca
Nord;Arras;100
Nord;Lille;370
Sud;Nice;350
Sud;Toulon;30
Total;;850"
    );
}

#[test]
fn the_grand_total_of_the_rows_can_leave() {
    for (form, expected) in [
        (
            Form::Compact,
            "region;ville;ca\nNord;Total;\nNord;Arras;100\nNord;Lille;370\nSud;Total;\nSud;Nice;350\nSud;Toulon;30",
        ),
        (
            Form::Tabular,
            "region;ville;ca\nNord;Arras;100\nNord;Lille;370\nSud;Nice;350\nSud;Toulon;30",
        ),
    ] {
        assert_eq!(towns_in(&rows_layout(form, false, &[false])), expected);
    }
}

#[test]
fn each_level_sets_its_subtotal() {
    let by_year = |subtotals: &[bool]| {
        arranged_example(
            &[REGION, CITY, YEAR],
            &[],
            &[Measure::Sum(AMOUNT)],
            &Order::default(),
            &rows_layout(Form::Tabular, true, subtotals),
        )
    };
    assert_eq!(
        by_year(&[false, true]),
        "\
region;ville;annee;ca
Nord;Arras;2025;40
Nord;Arras;2026;60
Nord;Arras;Total;100
Nord;Lille;2025;220
Nord;Lille;2026;150
Nord;Lille;Total;370
Sud;Nice;2025;170
Sud;Nice;2026;180
Sud;Nice;Total;350
Sud;Toulon;2026;30
Sud;Toulon;Total;30
Total;;;850"
    );
    assert_eq!(
        by_year(&[true, false]),
        "\
region;ville;annee;ca
Nord;Arras;2025;40
Nord;Arras;2026;60
Nord;Lille;2025;220
Nord;Lille;2026;150
Nord;Total;;470
Sud;Nice;2025;170
Sud;Nice;2026;180
Sud;Toulon;2026;30
Sud;Total;;380
Total;;;850"
    );
}

#[test]
fn a_level_of_columns_without_its_subtotal_loses_its_column() {
    let quarters = |columns: Totals| {
        arranged_example(
            &[REGION],
            &[YEAR, QUARTER],
            &[Measure::Sum(AMOUNT)],
            &Order::default(),
            &Layout {
                columns,
                ..Layout::default()
            },
        )
    };
    assert_eq!(
        quarters(Totals {
            grand_total: true,
            subtotals: vec![false],
        }),
        "\
region;2025 / T1 / ca;2025 / T2 / ca;2026 / T1 / ca;2026 / T2 / ca;Total / ca
Nord;140;120;150;60;470
Sud;80;90;140;70;380
Total;220;210;290;130;850"
    );
    assert_eq!(
        quarters(Totals {
            grand_total: false,
            subtotals: vec![false],
        }),
        "\
region;2025 / T1 / ca;2025 / T2 / ca;2026 / T1 / ca;2026 / T2 / ca
Nord;140;120;150;60
Sud;80;90;140;70
Total;220;210;290;130"
    );
}

#[test]
fn an_axis_without_levels_keeps_its_only_node() {
    let no_totals = Layout {
        form: Form::Tabular,
        rows: Totals {
            grand_total: false,
            subtotals: Vec::new(),
        },
        columns: Totals {
            grand_total: false,
            subtotals: Vec::new(),
        },
    };
    let table = |rows: &[usize], columns: &[usize]| {
        arranged_example(
            rows,
            columns,
            &[Measure::Sum(AMOUNT)],
            &Order::default(),
            &no_totals,
        )
    };
    assert_eq!(table(&[], &[]), "ca\n850");
    assert_eq!(table(&[], &[YEAR]), "2025 / ca;2026 / ca\n430;420");
    assert_eq!(table(&[REGION], &[]), "region;ca\nNord;470\nSud;380");
}

#[test]
fn a_subtotal_not_shown_still_ranks_its_group() {
    let by_total = lines_by(&[], 0, false);
    let table = arranged_example(
        &[REGION, CITY],
        &[],
        &[Measure::Sum(AMOUNT)],
        &by_total,
        &rows_layout(Form::Tabular, true, &[false]),
    );
    assert_eq!(
        table,
        "region;ville;ca\nSud;Toulon;30\nSud;Nice;350\nNord;Arras;100\nNord;Lille;370\nTotal;;850"
    );
}

#[test]
fn a_line_counts_the_levels_it_shares_with_the_previous_one() {
    let engine = example();
    let request = Request {
        rows: vec![REGION.into(), CITY.into()],
        measures: vec![Measure::CountStar],
        ..Request::default()
    };
    let mut table = engine.compute(&request).unwrap();
    let mut shared = |layout: &Layout, order: &Order| {
        table.lay_out(&engine, order, layout).unwrap();
        (0..table.line_count())
            .map(|line| table.line(&engine, line).shared)
            .collect::<Vec<_>>()
    };
    let tabular = rows_layout(Form::Tabular, true, &[]);
    assert_eq!(shared(&tabular, &Order::default()), [0, 1, 1, 0, 1, 1, 0]);
    let plain = rows_layout(Form::Tabular, true, &[false]);
    assert_eq!(shared(&plain, &Order::default()), [0, 1, 0, 1, 0]);
    assert_eq!(
        shared(&Layout::default(), &Order::default()),
        [0, 1, 1, 0, 1, 1, 0]
    );
    let descending = Order {
        rows: AxisOrder {
            descending: vec![true, true],
            values: None,
        },
        ..Order::default()
    };
    assert_eq!(shared(&plain, &descending), [0, 1, 0, 1, 0]);
}

#[test]
fn a_new_layout_starts_from_the_same_groups() {
    let engine = example();
    let request = Request {
        rows: vec![REGION.into(), CITY.into()],
        measures: vec![Measure::Sum(AMOUNT)],
        ..Request::default()
    };
    let mut table = engine.compute(&request).unwrap();
    let before = flat(&engine, &table, &["region", "ville"], &["ca"]);
    table
        .lay_out(
            &engine,
            &Order::default(),
            &rows_layout(Form::Tabular, false, &[false]),
        )
        .unwrap();
    assert_eq!(table.line_count(), 4);
    assert_eq!(table.finest_groups(), 4);
    table
        .lay_out(&engine, &Order::default(), &Layout::default())
        .unwrap();
    assert_eq!(flat(&engine, &table, &["region", "ville"], &["ca"]), before);
}

/// Column of values named by its labels, as the page names a widened one.
fn named(path: &[&str]) -> Vec<Option<String>> {
    path.iter().map(|&label| Some(label.to_owned())).collect()
}

#[test]
fn a_column_is_found_by_its_values_and_its_measure() {
    let engine = example();
    let request = Request {
        rows: vec![REGION.into()],
        columns: vec![YEAR.into(), QUARTER.into()],
        measures: vec![Measure::Sum(AMOUNT), Measure::CountStar],
        ..Request::default()
    };
    let mut table = engine.compute(&request).unwrap();
    // 2025: T1, T2, Total; 2026: T1, T2, Total; Total: two measures each.
    let at =
        |table: &Table, path: &[&str], measure| table.column_at(&engine, &named(path), measure);
    assert_eq!(at(&table, &["2025", "T1"], 0), Some(0));
    assert_eq!(at(&table, &["2025", "T1"], 1), Some(1));
    assert_eq!(at(&table, &["2025"], 0), Some(4));
    assert_eq!(at(&table, &["2026", "T2"], 1), Some(9));
    assert_eq!(at(&table, &[], 1), Some(13));
    // A node the table does not hold, a measure it does not have.
    assert_eq!(at(&table, &["2027"], 0), None);
    assert_eq!(at(&table, &["2025", "T3"], 0), None);
    assert_eq!(at(&table, &["2025", "T1"], 2), None);
    assert_eq!(at(&table, &["2025", "T1", "x"], 0), None);
    // In descending order, without the subtotals of the years.
    let layout = Layout {
        columns: Totals {
            grand_total: true,
            subtotals: vec![false, true],
        },
        ..Layout::default()
    };
    let order = Order {
        columns: AxisOrder {
            descending: vec![true, false],
            values: None,
        },
        ..Order::default()
    };
    table.lay_out(&engine, &order, &layout).unwrap();
    assert_eq!(at(&table, &["2026", "T1"], 0), Some(0));
    assert_eq!(at(&table, &["2025", "T2"], 1), Some(7));
    assert_eq!(at(&table, &["2025"], 0), None);
    assert_eq!(at(&table, &[], 0), Some(8));

    // Without measures, a column per node.
    let labels = Request {
        columns: vec![YEAR.into()],
        ..Request::default()
    };
    let table = engine.compute(&labels).unwrap();
    assert_eq!(at(&table, &["2026"], 0), Some(1));
    assert_eq!(at(&table, &["2026"], 1), None);
}
