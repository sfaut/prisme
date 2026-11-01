//! Tests of the engine on the example dataset of the specification: the
//! expected outputs are those of its cases, in the layout of its copy (one
//! column per level of the rows, `Total` in the first level a line
//! aggregates, flattened headers such as `2025 / T1 / ca`).

use prisme_store::{NumberError, Store};

use super::*;

/// Columns of the example dataset.
const REGION: usize = 0;
const CITY: usize = 1;
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
    let store = engine.store();
    let mut header: Vec<String> = levels.iter().map(|&level| level.to_owned()).collect();
    for column in 0..table.column_count() {
        let column = table.header(store, column);
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
            .line_path(store, line)
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
                value => value.text().unwrap_or_default(),
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
        rows: rows.to_vec(),
        columns: columns.to_vec(),
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
        rows: vec![CITY],
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
        rows: vec![REGION],
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
        rows: vec![0],
        columns: vec![],
        measures: vec![Measure::CountStar, Measure::Count(0)],
    };
    let table = engine.compute(&request).unwrap();
    assert_eq!(
        flat(&engine, &table, &["x"], &["n", "c"]),
        "x;n;c\nNULL;2;0\n;1;1\nNULL;1;1\na;1;1\nTotal;5;3"
    );
    assert_eq!(table.line(engine.store(), 0).label, Label::Value(None));
    assert_eq!(
        table.line(engine.store(), 2).label,
        Label::Value(Some("NULL"))
    );
    assert_eq!(
        table.line(engine.store(), 4),
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
        rows: vec![0],
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
        rows: vec![0],
        columns: vec![0],
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
        rows: vec![REGION],
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
        rows: vec![9],
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
