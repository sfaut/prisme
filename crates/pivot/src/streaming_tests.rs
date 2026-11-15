//! Tests of an engine whose rows arrive while it computes: generated rows are
//! appended block by block, and every table, computed or updated from the
//! state of the previous one, must be the one an engine created on the same
//! rows computes.

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

/// Table in the layout of its copy, with its numbers of groups and of rows.
fn text(engine: &Engine, table: &Table) -> String {
    let levels = vec!["level"; table.row_levels()];
    let measures = vec!["measure"; 16];
    format!(
        "{} groups, {} rows\n{}",
        table.finest_groups(),
        table.selected_rows(),
        flat(engine, table, &levels, &measures)
    )
}

/// Table of a request, computed from all the rows.
fn render(engine: &Engine, request: &Request) -> String {
    text(engine, &engine.compute(request).unwrap())
}

/// Table of a request, updated from the state of the previous one.
fn update(engine: &mut Engine, request: &Request) -> String {
    let table = engine.update(request).unwrap();
    text(engine, &table)
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
fn an_update_adds_the_rows_taken_in_since_and_gives_the_table_of_all_of_them() {
    let rows = generated(1_000);
    let requests = requests();
    let mut engine = engine_of(&rows[..0]);
    let mut received = 0;
    // The scale of the amounts grows at rows 150 and 400.
    for block in [1, 99, 49, 1, 250, 0, 300, 300] {
        let end = received + block;
        append(engine.store_mut(), &rows[received..end]);
        engine.grow();
        received = end;
        let whole = engine_of(&rows[..received]);
        for request in &requests {
            // Twice: the second update reads no row.
            for _ in 0..2 {
                assert_eq!(
                    update(&mut engine, request),
                    render(&whole, request),
                    "{received} rows, {request:?}"
                );
            }
        }
        // The same table, followed alone from block to block.
        let first = &requests[0];
        assert_eq!(update(&mut engine, first), render(&whole, first));
    }
}

#[test]
fn an_update_follows_the_rows_kept_by_the_grid() {
    let rows = generated(600);
    let kept =
        |range: std::ops::Range<u32>| -> Vec<u32> { range.filter(|row| row % 4 != 1).collect() };
    let request = &requests()[0];
    let mut engine = engine_of(&rows[..200]);
    engine.set_kept(Some(kept(0..200))).unwrap();
    engine.update(request).unwrap();
    append(engine.store_mut(), &rows[200..]);
    engine.grow();
    engine.extend_kept(&kept(200..600)).unwrap();
    let mut whole = engine_of(&rows);
    whole.set_kept(Some(kept(0..600))).unwrap();
    assert_eq!(update(&mut engine, request), render(&whole, request));
    // A new filter of the grid starts from all its rows.
    engine
        .set_kept(Some((0..600).step_by(7).collect()))
        .unwrap();
    whole.set_kept(Some((0..600).step_by(7).collect())).unwrap();
    assert_eq!(update(&mut engine, request), render(&whole, request));
    engine.set_kept(None).unwrap();
    whole.set_kept(None).unwrap();
    assert_eq!(update(&mut engine, request), render(&whole, request));
}

/// Engine whose last update has been given `extra` selected rows that do not
/// exist: a next update that keeps its state counts them, one that starts
/// from all the rows does not.
fn marked(engine: &mut Engine, request: &Request, extra: usize) {
    engine.update(request).unwrap();
    engine.accumulation.as_mut().expect("a kept state").selected += extra;
}

#[test]
fn an_update_keeps_the_state_of_the_same_table_only() {
    let rows = generated(300);
    let requests = requests();
    let mut engine = engine_of(&rows[..100]);
    // The same table goes on from its state.
    marked(&mut engine, &requests[0], 1_000);
    append(engine.store_mut(), &rows[100..120]);
    engine.grow();
    assert_eq!(engine.update(&requests[0]).unwrap().selected_rows(), 1_120);
    // Another table starts from all the rows.
    marked(&mut engine, &requests[0], 1_000);
    let other = engine.compute(&requests[2]).unwrap().selected_rows();
    assert_eq!(engine.update(&requests[2]).unwrap().selected_rows(), other);
    // Amounts of a finer scale start from all the rows: rows 150 and on have
    // a digit after the point.
    let sums = Request {
        measures: vec![Measure::Sum(AMOUNT)],
        ..Request::default()
    };
    marked(&mut engine, &sums, 1_000);
    append(engine.store_mut(), &rows[120..200]);
    engine.grow();
    assert_eq!(engine.update(&sums).unwrap().selected_rows(), 200);
    // Rows of the same scale go on.
    marked(&mut engine, &sums, 1_000);
    append(engine.store_mut(), &rows[200..300]);
    engine.grow();
    assert_eq!(engine.update(&sums).unwrap().selected_rows(), 1_300);
}

#[test]
fn a_table_whose_measures_do_not_add_up_keeps_no_state() {
    let rows = generated(100);
    let requests = requests();
    let mut engine = engine_of(&rows);
    // Distinct values and GROUP_CONCAT, then a filter on a measure.
    for request in [&requests[1], &requests[3]] {
        engine.update(request).unwrap();
        assert!(engine.accumulation.is_none(), "{request:?}");
    }
    engine.update(&requests[0]).unwrap();
    assert!(engine.accumulation.is_some());
    // A refused update keeps none.
    let wrong = Request {
        measures: vec![Measure::Sum(CITY)],
        ..Request::default()
    };
    assert_eq!(
        engine.update(&wrong).unwrap_err(),
        Error::NotNumeric { measure: 0 }
    );
    assert!(engine.accumulation.is_none());
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

#[test]
fn a_ranking_extended_by_parts_ranks_the_codes_as_a_stable_sort_of_all_of_them() {
    // Values by code, with equal values far apart.
    let values = [5, 3, 5, 1, 3, 9, 0, 5, 3, 7, 1, 5];
    let compare = |a: u32, b: u32| values[a as usize].cmp(&values[b as usize]);
    let mut sorted: Vec<u32> = (0..values.len() as u32).collect();
    sorted.sort_by(|&a, &b| compare(a, b));
    for parts in [vec![12], vec![1, 11], vec![3, 0, 5, 4], vec![1; 12]] {
        // The key of a code is the code itself.
        let mut ranking = Ranking::default();
        let mut codes = 0;
        for part in &parts {
            codes += part;
            ranking.extend(codes, |code| code, compare);
        }
        assert_eq!(ranking.order, sorted, "{parts:?}");
        assert_eq!(ranking.keys, sorted, "{parts:?}");
        assert_eq!(ranking.ranks, ranks_of(&sorted), "{parts:?}");
    }
}
