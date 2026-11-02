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
                value => value.text(engine.store()).unwrap_or_default(),
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
            label: Label::Total
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
        measures: vec![Measure::CountDistinct(CITY)],
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
    };
    let table = engine.compute(&request).unwrap();
    let store = engine.store();
    let lines: Vec<String> = (0..table.line_count())
        .map(|line| {
            let label = table.line_path(engine, line).first().copied().flatten();
            let values = (0..table.column_count()).map(|column| {
                let text = table.value(line, column).text(store);
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
    let measures = [Measure::Min(1), Measure::Max(1)];
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
        measures: vec![Measure::Min(0), Measure::Max(0)],
        ..Request::default()
    };
    let table = engine.compute(&request).unwrap();
    let text = |column| table.value(0, column).text(engine.store());
    assert_eq!(text(0).as_deref(), Some("A"));
    assert_eq!(text(1).as_deref(), Some("é"));
}

#[test]
fn extremes_read_the_rows_kept_by_the_filter() {
    let mut engine = single(&[Some("3"), Some("1"), Some("2")], Family::Exact);
    engine.set_kept(Some(vec![0, 2])).unwrap();
    let request = Request {
        measures: vec![Measure::Min(0), Measure::Max(0)],
        ..Request::default()
    };
    let table = engine.compute(&request).unwrap();
    let text = |column| table.value(0, column).text(engine.store());
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
    for measure in [Measure::Min(0), Measure::Max(0), Measure::Avg(0)] {
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
        measures: vec![Measure::CountStar, Measure::CountDistinct(DATE)],
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
    let engine = example();
    let request = Request {
        rows: rows.iter().map(|&column| column.into()).collect(),
        columns: columns.iter().map(|&column| column.into()).collect(),
        measures: measures.to_vec(),
    };
    let mut table = engine.compute(&request).unwrap();
    table.sort(&engine, order).unwrap();
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
    };
    let mut table = engine.compute(&request).unwrap();
    let before = flat(&engine, &table, &["region", "ville"], &["ca"]);
    table.sort(&engine, &lines_by(&["2025"], 0, true)).unwrap();
    assert_eq!(table.line_count(), 7);
    assert_eq!(table.column_count(), 3);
    assert_eq!(table.finest_groups(), 7);
    assert_eq!(table.selected_rows(), 10);
    assert_ne!(flat(&engine, &table, &["region", "ville"], &["ca"]), before);
    table.sort(&engine, &Order::default()).unwrap();
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
    table.sort(engine, &lines_by(&[], 0, false)).unwrap();
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
    assert_eq!(keys_by_value(&texts, Measure::Min(1)), "c b a");
    assert_eq!(keys_by_value(&texts, Measure::Max(1)), "c b a");
    let numbers = keyed(&[("a", Some("10")), ("b", Some("9"))], Family::Exact);
    assert_eq!(keys_by_value(&numbers, Measure::Min(1)), "b a");
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
    };
    let mut table = engine.compute(&request).unwrap();
    let mut order = lines_by(&[], 0, false);
    order.rows.values.as_mut().unwrap().path = vec![None];
    table.sort(&engine, &order).unwrap();
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
    };
    let mut table = engine.compute(&request).unwrap();
    table
        .sort(&engine, &lines_by(&["2026-02"], 0, false))
        .unwrap();
    assert_eq!(table.line_path(&engine, 0), [Some("Sud")]);
}
