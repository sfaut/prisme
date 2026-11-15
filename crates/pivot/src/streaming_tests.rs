//! Tests of an engine whose rows arrive while it computes: generated rows are
//! appended block by block, and every table must be the one an engine created
//! on the same rows computes.

use prisme_store::Store;

use super::*;
use crate::tests::flat;

/// Columns of the generated rows.
const REGION: usize = 0;
const CITY: usize = 1;
const DATE: usize = 2;
const AMOUNT: usize = 3;
const RATE: usize = 4;
const CODE: usize = 5;

/// Family of each column of the generated rows.
const FAMILIES: [Family; 6] = [
    Family::Other,
    Family::Other,
    Family::Date,
    Family::Exact,
    Family::Float,
    Family::Other,
];

/// Generator of numbers, the same from one run to the next (a linear
/// congruential generator, that of Knuth's MMIX).
struct Numbers(u64);

impl Numbers {
    /// Next number, below `bound`.
    fn below(&mut self, bound: u64) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 33) % bound
    }
}

/// Generated rows: few regions, more cities, dates over three years, exact
/// amounts whose scale grows with the rows (integers, then one digit after
/// the point, then three), floats, codes almost all distinct, and NULL in
/// every column.
fn generated(count: usize) -> Vec<[Option<String>; 6]> {
    let mut numbers = Numbers(7);
    (0..count)
        .map(|row| {
            let mut cell = |null: u64, text: String| (numbers.below(100) >= null).then_some(text);
            let region = format!("R{}", row % 5);
            let city = format!("C{}", row * 7 % 43);
            let date = format!(
                "{}-{:02}-{:02}",
                2024 + row % 3,
                1 + row * 5 % 12,
                1 + row * 11 % 28
            );
            let amount = match row {
                0..150 => format!("{}", row as i64 * 37 % 1_000 - 300),
                150..400 => format!("{}.{}", row % 90, row % 10),
                _ => format!("-{}.{:03}", row % 70, row * 13 % 1_000),
            };
            let rate = format!("{}", (row as f64 * 0.37).sin() * 100.0);
            let code = format!("K{}", row / 2);
            [
                cell(3, region),
                cell(5, city),
                cell(4, date),
                cell(10, amount),
                cell(8, rate),
                cell(2, code),
            ]
        })
        .collect()
}

/// Engine created on rows.
fn engine_of(rows: &[[Option<String>; 6]]) -> Engine {
    let mut store = Store::new(FAMILIES.len());
    append(&mut store, rows);
    Engine::new(store, FAMILIES.to_vec())
}

/// Appends rows to a store.
fn append(store: &mut Store, rows: &[[Option<String>; 6]]) {
    for row in rows {
        store.push_row(&row.each_ref().map(Option::as_deref));
    }
}

/// Level of a column at a precision.
fn at(column: usize, precision: Precision) -> Level {
    Level {
        column,
        precision: Some(precision),
    }
}

/// Requests that read every form derived from the columns.
fn requests() -> Vec<Request> {
    let concat = Concat {
        level: CITY.into(),
        distinct: true,
        order: vec![ConcatKey {
            column: AMOUNT,
            descending: true,
        }],
        separator: ",".to_owned(),
        offset: 1,
        limit: Some(5),
        max_bytes: 40,
    };
    vec![
        Request {
            rows: vec![REGION.into(), CITY.into()],
            columns: vec![at(DATE, Precision::Year)],
            measures: vec![
                Measure::CountStar,
                Measure::Count(AMOUNT),
                Measure::Sum(AMOUNT),
                Measure::Avg(AMOUNT),
                Measure::Sum(RATE),
                Measure::Avg(RATE),
                Measure::Min(CITY.into()),
                Measure::Max(at(DATE, Precision::Month)),
                Measure::Min(AMOUNT.into()),
                Measure::Max(RATE.into()),
            ],
            ..Request::default()
        },
        Request {
            rows: vec![at(DATE, Precision::Month)],
            columns: vec![REGION.into()],
            measures: vec![
                Measure::Sum(AMOUNT),
                Measure::CountDistinct(CITY.into()),
                Measure::SumDistinct(AMOUNT),
                Measure::AvgDistinct(RATE),
                Measure::GroupConcat(Box::new(concat)),
            ],
            show_as: vec![ShowAs::RowShare { decimals: 2 }],
            ..Request::default()
        },
        Request {
            rows: vec![CITY.into()],
            measures: vec![Measure::Sum(AMOUNT), Measure::CountStar],
            show_as: vec![ShowAs::ColumnShare { decimals: 1 }],
            filters: vec![
                Filter::Values {
                    level: REGION.into(),
                    values: vec![Some("R1".to_owned()), Some("R3".to_owned())],
                    excluded: true,
                },
                Filter::Column {
                    level: AMOUNT.into(),
                    test: Test::Compare(Comparison::Greater, "-20.5".to_owned()),
                },
            ],
            ..Request::default()
        },
        Request {
            rows: vec![CODE.into()],
            columns: vec![at(DATE, Precision::Quarter)],
            measures: vec![Measure::Count(RATE), Measure::Max(CODE.into())],
            filters: vec![Filter::Measure {
                measure: Measure::CountStar,
                levels: vec![CODE.into()],
                test: Test::Compare(Comparison::GreaterOrEqual, "2".to_owned()),
            }],
            ..Request::default()
        },
    ]
}

/// Table of a request in the layout of its copy, with its numbers of groups
/// and of rows.
fn render(engine: &Engine, request: &Request) -> String {
    let table = engine.compute(request).unwrap();
    let levels = vec!["level"; request.rows.len()];
    let measures = vec!["measure"; request.measures.len()];
    format!(
        "{} groups, {} rows\n{}",
        table.finest_groups(),
        table.selected_rows(),
        flat(engine, &table, &levels, &measures)
    )
}

#[test]
fn an_engine_that_grows_computes_the_tables_of_an_engine_created_on_its_rows() {
    let rows = generated(1_000);
    let mut engine = engine_of(&rows[..0]);
    let mut received = 0;
    for block in [1, 99, 50, 250, 0, 600] {
        let end = received + block;
        append(engine.store_mut(), &rows[received..end]);
        // The rows count once taken in.
        assert_eq!(engine.rows(), received);
        engine.grow();
        received = end;
        assert_eq!(engine.rows(), received);
        let whole = engine_of(&rows[..received]);
        for request in requests() {
            assert_eq!(
                render(&engine, &request),
                render(&whole, &request),
                "{received} rows, {request:?}"
            );
        }
    }
}

#[test]
fn rows_appended_to_some_columns_wait_for_the_others() {
    let rows = generated(10);
    let mut engine = engine_of(&rows[..4]);
    let store = engine.store_mut();
    store.column_mut(REGION).unwrap().push(Some("R9"));
    engine.grow();
    assert_eq!(engine.rows(), 4);
    let request = Request {
        rows: vec![REGION.into()],
        measures: vec![Measure::CountStar],
        ..Request::default()
    };
    assert_eq!(engine.compute(&request).unwrap().selected_rows(), 4);
}

#[test]
fn the_ranks_of_the_page_are_dropped_when_their_column_gains_values() {
    let mut store = Store::new(2);
    for row in [["b", "x"], ["a", "y"]] {
        store.push_row(&row.map(Some));
    }
    let mut engine = Engine::new(store, vec![Family::Other; 2]);
    engine.set_ranks(0, vec![0, 1]).unwrap();
    engine.set_ranks(1, vec![1, 0]).unwrap();
    // A row of values already met keeps the ranks.
    engine.store_mut().push_row(&[Some("a"), Some("x")]);
    assert_eq!(engine.grow(), Vec::<usize>::new());
    assert_eq!(engine.ranks(0).unwrap(), [0, 1]);
    // A new value drops them.
    engine.store_mut().push_row(&[Some("c"), Some("y")]);
    assert_eq!(engine.grow(), [0]);
    assert_eq!(engine.ranks(1).unwrap(), [1, 0]);
    // Without them, the values follow the binary order of their texts.
    assert_eq!(engine.ranks(0).unwrap(), [1, 0, 2]);
}

#[test]
fn the_rows_kept_by_the_grid_extend_to_the_rows_taken_in() {
    let rows = generated(100);
    let mut engine = engine_of(&rows[..60]);
    let kept =
        |range: std::ops::Range<u32>| -> Vec<u32> { range.filter(|row| row % 3 != 0).collect() };
    engine.set_kept(Some(kept(0..60))).unwrap();
    append(engine.store_mut(), &rows[60..]);
    assert_eq!(engine.extend_kept(&[60, 99]), Err(Error::UnknownRow(60)));
    engine.grow();
    engine.extend_kept(&kept(60..100)).unwrap();
    let mut whole = engine_of(&rows);
    whole.set_kept(Some(kept(0..100))).unwrap();
    for request in requests() {
        assert_eq!(render(&engine, &request), render(&whole, &request));
    }
}

#[test]
fn a_cell_that_is_no_number_among_the_rows_taken_in_stops_the_sums() {
    let rows = generated(20);
    let mut engine = engine_of(&rows);
    let request = Request {
        measures: vec![Measure::Sum(AMOUNT)],
        ..Request::default()
    };
    engine.compute(&request).unwrap();
    let mut wrong = rows[0].clone();
    wrong[AMOUNT] = Some("douze".to_owned());
    append(engine.store_mut(), &[wrong]);
    engine.grow();
    assert_eq!(
        engine.compute(&request).unwrap_err(),
        Error::Number {
            column: AMOUNT,
            error: NumberError::Invalid { row: 20 }
        }
    );
}
