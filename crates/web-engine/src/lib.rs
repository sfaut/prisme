//! WebAssembly side of the Pivoteur: the engine of `prisme-pivot`, compiled
//! to `wasm32-unknown-unknown` and run in a Worker of the page
//! (`web/src/components/pivotWorker.ts`), on one thread.
//!
//! The rows stay in the page, where the grid holds them: the Worker copies
//! them once into the memory of the module, block by block and column by
//! column (UTF-8 bytes, offset and validity of each cell, as in
//! `bench/pivot/js/transfer.mjs`), then asks for tables, for the windows
//! of a table the page shows, and for the positions of the columns it names
//! by their values. A table never leaves the module whole.
//!
//! The functions are exported without `wasm-bindgen`: they take and return
//! numbers, and addresses in the memory of the module. Those that answer
//! with data write JSON (`json.rs`) and return its length in bytes;
//! `output_address` gives its address. A refused call answers
//! `{"error": {"kind", …}}`.

mod json;

use std::alloc::{Layout as MemoryLayout, alloc, dealloc, handle_alloc_error};
use std::cell::RefCell;
use std::slice;

use prisme_pivot::{
    AxisOrder, Comparison, Engine, Family, Filter, Form, Layout, Level, Measure, Order, Precision,
    Request, Table, Test, Totals, ValueOrder,
};
use prisme_store::Store;

/// Alignment of the memory reserved for JavaScript, enough for the offsets
/// and the row numbers (`u32`).
const ALIGN: usize = 8;

/// Word that stands for NULL in a path of values (see `order`).
const NULL_STEP: u32 = u32::MAX;

/// Copy of a result under way: its rows so far, the family of each column.
struct Loading {
    /// Rows received so far.
    store: Store,
    /// Family of each column.
    families: Vec<Family>,
}

/// State of the module between two calls.
#[derive(Default)]
struct State {
    /// Copy under way, until `finish`.
    loading: Option<Loading>,
    /// Engine of the result, once copied.
    engine: Option<Engine>,
    /// Last table computed.
    table: Option<Table>,
    /// Last answer written, as JSON.
    output: String,
}

thread_local! {
    /// State of the module, on its only thread.
    static STATE: RefCell<State> = RefCell::default();
}

/// Layout of `size` bytes of memory reserved for JavaScript.
fn layout(size: usize) -> MemoryLayout {
    MemoryLayout::from_size_align(size.max(1), ALIGN).expect("size below the memory of the module")
}

/// Reserves `size` bytes of memory for JavaScript, and returns their address.
#[unsafe(no_mangle)]
pub extern "C" fn reserve(size: usize) -> *mut u8 {
    let layout = layout(size);
    // SAFETY: the layout has a size above zero.
    let pointer = unsafe { alloc(layout) };
    if pointer.is_null() {
        handle_alloc_error(layout);
    }
    pointer
}

/// Frees memory reserved for JavaScript.
///
/// # Safety
///
/// `pointer` comes from `reserve(size)`, and is freed only once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn release(pointer: *mut u8, size: usize) {
    // SAFETY: given by the caller.
    unsafe { dealloc(pointer, layout(size)) }
}

/// Address of the last answer written.
#[unsafe(no_mangle)]
pub extern "C" fn output_address() -> *const u8 {
    STATE.with_borrow(|state| state.output.as_ptr())
}

/// Writes an answer and returns its length in bytes.
fn answer(state: &mut State, write: impl FnOnce(&mut String, &State)) -> usize {
    let mut output = std::mem::take(&mut state.output);
    output.clear();
    write(&mut output, state);
    state.output = output;
    state.output.len()
}

/// Family of a column from its code: 1 exact numbers, 2 floats, 3 dates,
/// 4 instants, 5 hours, any other code for the other families.
fn family(code: u8) -> Family {
    match code {
        1 => Family::Exact,
        2 => Family::Float,
        3 => Family::Date,
        4 => Family::Datetime,
        5 => Family::Time,
        _ => Family::Other,
    }
}

/// Starts the copy of a result of `columns` columns, the family of each
/// given by a byte at `families` (see `family`). The previous result and its
/// table are dropped.
///
/// # Safety
///
/// `families` holds `columns` bytes, untouched during the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn begin(families: *const u8, columns: usize) {
    // SAFETY: given by the caller.
    let families = unsafe { slice::from_raw_parts(families, columns) };
    STATE.with_borrow_mut(|state| {
        state.table = None;
        state.engine = None;
        state.loading = Some(Loading {
            store: Store::new(columns),
            families: families.iter().map(|&code| family(code)).collect(),
        });
    });
}

/// Appends a block of `rows` cells to column `column` of the copy under way:
/// `length` bytes of UTF-8 text at `bytes`, `rows + 1` offsets at `offsets`
/// (cell `row` runs from `offsets[row]` to `offsets[row + 1]`), one validity
/// byte per row at `valid` (0 for NULL). Returns 0, or the length of the
/// error written.
///
/// # Safety
///
/// The three addresses hold what is described above, untouched during the
/// call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn append(
    column: usize,
    bytes: *const u8,
    length: usize,
    offsets: *const u32,
    valid: *const u8,
    rows: usize,
) -> usize {
    // SAFETY: given by the caller.
    let (bytes, offsets, valid) = unsafe {
        (
            slice::from_raw_parts(bytes, length),
            slice::from_raw_parts(offsets, rows + 1),
            slice::from_raw_parts(valid, rows),
        )
    };
    STATE.with_borrow_mut(|state| {
        let target = state
            .loading
            .as_mut()
            .and_then(|loading| loading.store.column_mut(column));
        match target.map(|target| target.append(bytes, offsets, valid)) {
            Some(Ok(())) => 0,
            Some(Err(error)) => answer(state, |json, _| {
                json::state_error(json, "block", Some(&error))
            }),
            None => answer(state, |json, _| {
                json::state_error(json, "not_loading", None)
            }),
        }
    })
}

/// Ends the copy under way: the engine of the result starts from its rows.
/// Returns 0, or the length of the error written if no copy is under way or
/// its columns do not have the same number of rows.
#[unsafe(no_mangle)]
pub extern "C" fn finish() -> usize {
    STATE.with_borrow_mut(|state| match state.loading.take() {
        Some(loading) => {
            let rows = loading.store.row_count();
            let complete = (0..loading.store.column_count()).all(|column| {
                loading
                    .store
                    .column(column)
                    .is_some_and(|cells| cells.len() == rows)
            });
            if complete {
                state.engine = Some(Engine::new(loading.store, loading.families));
                0
            } else {
                answer(state, |json, _| json::state_error(json, "incomplete", None))
            }
        }
        None => answer(state, |json, _| {
            json::state_error(json, "not_loading", None)
        }),
    })
}

/// Writes the distinct values of a column at a precision (see `level`). As they
/// are, by code: the page ranks them in the order of the grid (`set_ranks`). At a
/// precision, the labels of the periods in the order of time.
#[unsafe(no_mangle)]
pub extern "C" fn distinct(column: u32, precision: u32) -> usize {
    STATE.with_borrow_mut(|state| {
        answer(state, |json, state| {
            match (&state.engine, level(column, precision)) {
                (Some(engine), Some(level)) => match engine.distinct_at(level) {
                    Ok(values) => json::distinct(json, &values),
                    Err(error) => json::error(json, &error),
                },
                (None, _) => json::state_error(json, "no_result", None),
                (_, None) => json::state_error(json, "request", None),
            }
        })
    })
}

/// Sets the rank of each distinct value of column `column`: `count` ranks
/// (`u32`) at `ranks`, by code. Writes `{}` or an error.
///
/// # Safety
///
/// `ranks` holds `count` numbers, untouched during the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn set_ranks(column: usize, ranks: *const u32, count: usize) -> usize {
    // SAFETY: given by the caller.
    let ranks = unsafe { slice::from_raw_parts(ranks, count) }.to_vec();
    STATE.with_borrow_mut(|state| {
        let result = state
            .engine
            .as_mut()
            .map(|engine| engine.set_ranks(column, ranks));
        answer(state, |json, _| match result {
            Some(Ok(())) => json.push_str("{}"),
            Some(Err(error)) => json::error(json, &error),
            None => json::state_error(json, "no_result", None),
        })
    })
}

/// Keeps only the rows the filter of the grid keeps: `count` row numbers
/// (`u32`) at `rows`, in their order; all the rows if `all` is not 0. Writes
/// `{}` or an error.
///
/// # Safety
///
/// Unless `all` is not 0, `rows` holds `count` numbers, untouched during the
/// call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn set_kept(rows: *const u32, count: usize, all: u32) -> usize {
    let kept = if all != 0 {
        None
    } else {
        // SAFETY: given by the caller.
        Some(unsafe { slice::from_raw_parts(rows, count) }.to_vec())
    };
    STATE.with_borrow_mut(|state| {
        let result = state.engine.as_mut().map(|engine| engine.set_kept(kept));
        answer(state, |json, _| match result {
            Some(Ok(())) => json.push_str("{}"),
            Some(Err(error)) => json::error(json, &error),
            None => json::state_error(json, "no_result", None),
        })
    })
}

/// Reads a level: a column and a precision (0 its values as they are, 1 year,
/// 2 quarter, 3 month, 4 week, 5 date). `None` for another precision.
fn level(column: u32, precision: u32) -> Option<Level> {
    let precision = match precision {
        0 => None,
        1 => Some(Precision::Year),
        2 => Some(Precision::Quarter),
        3 => Some(Precision::Month),
        4 => Some(Precision::Week),
        5 => Some(Precision::Date),
        _ => return None,
    };
    Some(Level {
        column: column as usize,
        precision,
    })
}

/// Reads levels, each by two words (see `level`).
fn levels(words: &[u32]) -> Option<Vec<Level>> {
    words
        .chunks(2)
        .map(|pair| level(pair[0], pair[1]))
        .collect()
}

/// Reads a measure: a kind (0 `COUNT(*)`, 1 `COUNT(col)`, 2 `SUM(col)`,
/// 3 `COUNT(DISTINCT col)`, 4 `SUM(DISTINCT col)`, 5 `AVG(col)`,
/// 6 `AVG(DISTINCT col)`, 7 `MIN(col)`, 8 `MAX(col)`) and a column. `None` for
/// another kind.
fn measure(kind: u32, column: u32) -> Option<Measure> {
    let column = column as usize;
    match kind {
        0 => Some(Measure::CountStar),
        1 => Some(Measure::Count(column)),
        2 => Some(Measure::Sum(column)),
        3 => Some(Measure::CountDistinct(column)),
        4 => Some(Measure::SumDistinct(column)),
        5 => Some(Measure::Avg(column)),
        6 => Some(Measure::AvgDistinct(column)),
        7 => Some(Measure::Min(column)),
        8 => Some(Measure::Max(column)),
        _ => None,
    }
}

/// Reads a comparison: 0 `=`, 1 `<>`, 2 `<`, 3 `<=`, 4 `>`, 5 `>=`. `None` for
/// another code.
fn comparison(code: u32) -> Option<Comparison> {
    match code {
        0 => Some(Comparison::Equal),
        1 => Some(Comparison::NotEqual),
        2 => Some(Comparison::Less),
        3 => Some(Comparison::LessOrEqual),
        4 => Some(Comparison::Greater),
        5 => Some(Comparison::GreaterOrEqual),
        _ => None,
    }
}

/// Reads an operand (see `path_step`), which NULL cannot be.
fn operand(words: &mut impl Iterator<Item = u32>) -> Option<String> {
    path_step(words)?
}

/// Reads a test: a comparison (see `comparison`) and its operand; 6 `BETWEEN` or
/// 7 `NOT BETWEEN` and its two bounds; 8 `IS NULL`; 9 `IS NOT NULL`. `None` for
/// another code.
fn test(words: &mut impl Iterator<Item = u32>) -> Option<Test> {
    Some(match words.next()? {
        6 => Test::Between(operand(words)?, operand(words)?),
        7 => Test::NotBetween(operand(words)?, operand(words)?),
        8 => Test::IsNull,
        9 => Test::IsNotNull,
        code => Test::Compare(comparison(code)?, operand(words)?),
    })
}

/// Reads filters, up to the end of the words. A filter on a list of values: 0, the
/// level (see `level`), a word not 0 for `NOT IN`, the number of values, and each
/// value (see `path_step`). A filter on a measure: 1, the measure (see `measure`),
/// the number of its levels and each level, and the test (see `test`). A filter
/// on a column: 2, the level and the test.
fn filters(words: &[u32]) -> Option<Vec<Filter>> {
    let mut words = words.iter().copied();
    let mut filters = Vec::new();
    while let Some(kind) = words.next() {
        filters.push(match kind {
            0 => {
                let level = level(words.next()?, words.next()?)?;
                let excluded = words.next()? != 0;
                let count = words.next()?;
                let values = (0..count)
                    .map(|_| path_step(&mut words))
                    .collect::<Option<Vec<_>>>()?;
                Filter::Values {
                    level,
                    values,
                    excluded,
                }
            }
            1 => {
                let measure = measure(words.next()?, words.next()?)?;
                let count = words.next()?;
                let levels = (0..count)
                    .map(|_| level(words.next()?, words.next()?))
                    .collect::<Option<Vec<_>>>()?;
                let test = test(&mut words)?;
                Filter::Measure {
                    measure,
                    levels,
                    test,
                }
            }
            2 => Filter::Column {
                level: level(words.next()?, words.next()?)?,
                test: test(&mut words)?,
            },
            _ => return None,
        });
    }
    Some(filters)
}

/// Reads a request: the numbers of levels of the rows and of the columns,
/// the number of measures, the levels of the rows, then of the columns (see
/// `levels`), then the measures, each by two words (see `measure`), then the
/// filters (see `filters`). `None` if the words do not match.
fn request(words: &[u32]) -> Option<Request> {
    let (&rows, &columns, &measures) = (words.first()?, words.get(1)?, words.get(2)?);
    let (rows, columns, measures) = (rows as usize, columns as usize, measures as usize);
    let rest = &words[3..];
    let fixed = 2 * (rows + columns + measures);
    if rest.len() < fixed {
        return None;
    }
    let (rest, filter_words) = rest.split_at(fixed);
    let (row_levels, rest) = rest.split_at(2 * rows);
    let (column_levels, measure_words) = rest.split_at(2 * columns);
    let measures = measure_words
        .chunks(2)
        .map(|pair| measure(pair[0], pair[1]))
        .collect::<Option<Vec<_>>>()?;
    Some(Request {
        rows: levels(row_levels)?,
        columns: levels(column_levels)?,
        measures,
        filters: filters(filter_words)?,
    })
}

/// Computes the table of a request of `length` words (`u32`) at `words` (see
/// `request`), keeps it in place of the previous one, and writes its shape
/// (`json::shape`) or an error. A refused request keeps the previous table:
/// the page goes on showing it, with the error.
///
/// # Safety
///
/// `words` holds `length` numbers, untouched during the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn compute(words: *const u32, length: usize) -> usize {
    // SAFETY: given by the caller.
    let words = unsafe { slice::from_raw_parts(words, length) };
    let request = request(words);
    STATE.with_borrow_mut(|state| {
        let result = match (&state.engine, request) {
            (Some(engine), Some(request)) => Some(engine.compute(&request)),
            _ => None,
        };
        match result {
            Some(Ok(table)) => {
                state.table = Some(table);
                answer(state, |json, state| {
                    json::shape(json, state.table.as_ref().expect("table just computed"))
                })
            }
            Some(Err(error)) => answer(state, |json, _| json::error(json, &error)),
            None if state.engine.is_none() => {
                answer(state, |json, _| json::state_error(json, "no_result", None))
            }
            None => answer(state, |json, _| json::state_error(json, "request", None)),
        }
    })
}

/// Reads the value of a step of a path: `NULL_STEP` for NULL, otherwise its
/// length in bytes, then one word per byte of its UTF-8 text.
fn path_step(words: &mut impl Iterator<Item = u32>) -> Option<Option<String>> {
    match words.next()? {
        NULL_STEP => Some(None),
        length => {
            let bytes = (0..length)
                .map(|_| words.next().map(|word| word as u8))
                .collect::<Option<Vec<u8>>>()?;
            String::from_utf8(bytes).ok().map(Some)
        }
    }
}

/// Reads the order of an axis: the number of its levels, a word per level (not
/// 0 for descending), then 0 without values that rank it; otherwise 1, the
/// measure, a word for the direction, the number of steps of the path, and each
/// step (see `path_step`).
fn axis_order(words: &mut impl Iterator<Item = u32>) -> Option<AxisOrder> {
    let levels = words.next()?;
    let descending = (0..levels)
        .map(|_| words.next().map(|word| word != 0))
        .collect::<Option<Vec<bool>>>()?;
    let values = match words.next()? {
        0 => None,
        _ => {
            let measure = words.next()? as usize;
            let descending = words.next()? != 0;
            let steps = words.next()?;
            let path = (0..steps)
                .map(|_| path_step(words))
                .collect::<Option<Vec<_>>>()?;
            Some(ValueOrder {
                path,
                measure,
                descending,
            })
        }
    };
    Some(AxisOrder { descending, values })
}

/// Reads the totals of an axis: a word for its grand total (0 to leave it out),
/// the number of its levels, and a word per level for its subtotal (0 to leave
/// it out).
fn totals(words: &mut impl Iterator<Item = u32>) -> Option<Totals> {
    let grand_total = words.next()? != 0;
    let levels = words.next()?;
    let subtotals = (0..levels)
        .map(|_| words.next().map(|word| word != 0))
        .collect::<Option<Vec<bool>>>()?;
    Some(Totals {
        grand_total,
        subtotals,
    })
}

/// Reads what lays a table out: the order of the rows, then that of the columns
/// (see `axis_order`), the form (0 compact, 1 outline, 2 tabular), then the
/// totals of the rows and those of the columns (see `totals`). `None` if the
/// words do not match.
fn arrangement(words: &[u32]) -> Option<(Order, Layout)> {
    let mut words = words.iter().copied();
    let rows = axis_order(&mut words)?;
    let columns = axis_order(&mut words)?;
    let form = match words.next()? {
        0 => Form::Compact,
        1 => Form::Outline,
        2 => Form::Tabular,
        _ => return None,
    };
    let layout = Layout {
        form,
        rows: totals(&mut words)?,
        columns: totals(&mut words)?,
    };
    words
        .next()
        .is_none()
        .then_some((Order { rows, columns }, layout))
}

/// Lays the last table computed out again, by `length` words (`u32`) at `words`
/// (see `arrangement`), without computing anything. Writes its shape
/// (`json::shape`), which the totals change, or an error.
///
/// # Safety
///
/// `words` holds `length` numbers, untouched during the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lay_out(words: *const u32, length: usize) -> usize {
    // SAFETY: given by the caller.
    let words = unsafe { slice::from_raw_parts(words, length) };
    let arrangement = arrangement(words);
    STATE.with_borrow_mut(|state| {
        let result = match (&state.engine, &mut state.table, arrangement) {
            (Some(engine), Some(table), Some((order, layout))) => {
                Some(table.lay_out(engine, &order, &layout))
            }
            _ => None,
        };
        answer(state, |json, state| match result {
            Some(Ok(())) => json::shape(json, state.table.as_ref().expect("table just laid out")),
            Some(Err(error)) => json::error(json, &error),
            None if state.table.is_none() => json::state_error(json, "no_table", None),
            None => json::state_error(json, "request", None),
        })
    })
}

/// Reads a column of values named by its values: its measure, the number of
/// steps of the path of its node, and each step (see `path_step`).
fn named_column(words: &mut impl Iterator<Item = u32>) -> Option<(Vec<Option<String>>, usize)> {
    let measure = words.next()? as usize;
    let steps = words.next()?;
    let path = (0..steps)
        .map(|_| path_step(words))
        .collect::<Option<Vec<_>>>()?;
    Some((path, measure))
}

/// Reads columns of values named by their values: their number, then each
/// one (see `named_column`). `None` if the words do not match.
fn named_columns(words: &[u32]) -> Option<Vec<(Vec<Option<String>>, usize)>> {
    let mut words = words.iter().copied();
    let count = words.next()?;
    let columns = (0..count)
        .map(|_| named_column(&mut words))
        .collect::<Option<Vec<_>>>()?;
    words.next().is_none().then_some(columns)
}

/// Writes the position of columns of values in the last table laid out,
/// named by `length` words (`u32`) at `words` (see `named_columns`):
/// `null` for a column the table does not show (`json::positions`), or an
/// error. The page places by them the columns it has widened.
///
/// # Safety
///
/// `words` holds `length` numbers, untouched during the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn columns_at(words: *const u32, length: usize) -> usize {
    // SAFETY: given by the caller.
    let words = unsafe { slice::from_raw_parts(words, length) };
    let columns = named_columns(words);
    STATE.with_borrow_mut(|state| {
        let positions = match (&state.engine, &state.table, columns) {
            (Some(engine), Some(table), Some(columns)) => Some(
                columns
                    .iter()
                    .map(|(path, measure)| table.column_at(engine, path, *measure))
                    .collect::<Vec<_>>(),
            ),
            _ => None,
        };
        answer(state, |json, state| match positions {
            Some(positions) => json::positions(json, &positions),
            None if state.table.is_none() => json::state_error(json, "no_table", None),
            None => json::state_error(json, "request", None),
        })
    })
}

/// Writes a window of the last table computed: lines `first_line` to
/// `end_line` (excluded), columns of values `first_column` to `end_column`
/// (excluded), both bounded by the table (`json::window`).
#[unsafe(no_mangle)]
pub extern "C" fn window(
    first_line: usize,
    end_line: usize,
    first_column: usize,
    end_column: usize,
) -> usize {
    STATE.with_borrow_mut(|state| {
        answer(state, |json, state| match (&state.engine, &state.table) {
            (Some(engine), Some(table)) => json::window(
                json,
                engine,
                table,
                first_line..end_line,
                first_column..end_column,
            ),
            _ => json::state_error(json, "no_table", None),
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Words of the layout of a table just computed: compact form, every total
    /// of both axes.
    const FIRST_LAYOUT: [u32; 5] = [0, 1, 0, 1, 0];

    /// Last answer written.
    fn output(length: usize) -> String {
        STATE.with_borrow(|state| {
            assert_eq!(state.output.len(), length);
            state.output.clone()
        })
    }

    /// Copies the cells of a column, in one block.
    fn append_column(column: usize, cells: &[Option<&str>]) -> usize {
        let mut bytes = Vec::new();
        let mut offsets = vec![0u32];
        let mut valid = Vec::new();
        for cell in cells {
            bytes.extend_from_slice(cell.unwrap_or("").as_bytes());
            offsets.push(bytes.len() as u32);
            valid.push(u8::from(cell.is_some()));
        }
        // SAFETY: the three slices hold what `append` reads.
        unsafe {
            append(
                column,
                bytes.as_ptr(),
                bytes.len(),
                offsets.as_ptr(),
                valid.as_ptr(),
                cells.len(),
            )
        }
    }

    #[test]
    fn a_result_is_copied_then_computed_then_read_by_windows() {
        let families = [0u8, 1];
        // SAFETY: two families.
        unsafe { begin(families.as_ptr(), 2) };
        assert_eq!(
            append_column(0, &[Some("Nord"), Some("Sud"), Some("Nord")]),
            0
        );
        assert_eq!(append_column(1, &[Some("1.5"), None, Some("2")]), 0);
        assert_eq!(finish(), 0);

        let length = distinct(0, 0);
        assert_eq!(output(length), r#"["Nord","Sud"]"#);
        let length = distinct(0, 9);
        assert_eq!(output(length), r#"{"error":{"kind":"request"}}"#);
        let ranks = [1u32, 0];
        // SAFETY: two ranks.
        let length = unsafe { set_ranks(0, ranks.as_ptr(), 2) };
        assert_eq!(output(length), "{}");

        // Rows: column 0; measures: COUNT(*), SUM(column 1).
        let words = [1u32, 0, 2, 0, 0, 0, 0, 2, 1];
        // SAFETY: nine words.
        let length = unsafe { compute(words.as_ptr(), words.len()) };
        assert_eq!(
            output(length),
            r#"{"lines":3,"columns":2,"rowLevels":1,"columnLevels":0,"groups":2,"rows":3}"#
        );
        let length = window(0, 10, 0, 10);
        assert_eq!(
            output(length),
            concat!(
                r#"{"lines":[{"depth":1,"label":"Sud","path":["Sud"],"shared":0},{"depth":1,"label":"Nord","path":["Nord"],"shared":0},{"depth":0,"label":null,"path":[],"shared":0}],"#,
                r#""columns":[{"node":0,"path":[],"total":false,"measure":0},{"node":0,"path":[],"total":false,"measure":1}],"#,
                r#""cells":[["1",null],["2","3.5"],["3","3.5"]]}"#
            )
        );

        let kept = [1u32];
        // SAFETY: one row number.
        let length = unsafe { set_kept(kept.as_ptr(), 1, 0) };
        assert_eq!(output(length), "{}");
        // SAFETY: nine words.
        let length = unsafe { compute(words.as_ptr(), words.len()) };
        assert!(output(length).ends_with(r#""groups":1,"rows":1}"#));

        let sum_of_text = [0u32, 0, 1, 2, 0];
        // SAFETY: five words.
        let length = unsafe { compute(sum_of_text.as_ptr(), sum_of_text.len()) };
        assert_eq!(
            output(length),
            r#"{"error":{"kind":"not_numeric","measure":0}}"#
        );
        let length = window(0, 1, 0, 1);
        assert_eq!(
            output(length),
            concat!(
                r#"{"lines":[{"depth":1,"label":"Sud","path":["Sud"],"shared":0}],"#,
                r#""columns":[{"node":0,"path":[],"total":false,"measure":0}],"#,
                r#""cells":[["1"]]}"#
            )
        );
    }

    #[test]
    fn a_sort_lays_the_table_out_again_without_a_new_computation() {
        let families = [0u8, 1];
        // SAFETY: two families.
        unsafe { begin(families.as_ptr(), 2) };
        assert_eq!(append_column(0, &[Some("a"), Some("b"), None]), 0);
        assert_eq!(append_column(1, &[Some("3"), Some("1"), Some("2")]), 0);
        assert_eq!(finish(), 0);

        let no_table = [0u32, 0, 0, 0, 0, 1, 0, 1, 0];
        // SAFETY: nine words.
        let length = unsafe { lay_out(no_table.as_ptr(), no_table.len()) };
        assert_eq!(output(length), r#"{"error":{"kind":"no_table"}}"#);

        // Rows: column 0; measure: SUM(column 1).
        let words = [1u32, 0, 1, 0, 0, 2, 1];
        // SAFETY: seven words.
        unsafe { compute(words.as_ptr(), words.len()) };
        // Lines by the grand total, measure 0, ascending; columns as they are.
        let by_value = [[1u32, 0, 1, 0, 0, 0, 0, 0].as_slice(), &FIRST_LAYOUT].concat();
        // SAFETY: thirteen words.
        let length = unsafe { lay_out(by_value.as_ptr(), by_value.len()) };
        assert_eq!(
            output(length),
            r#"{"lines":4,"columns":1,"rowLevels":1,"columnLevels":0,"groups":3,"rows":3}"#
        );
        let labels = || {
            let length = window(0, 4, 0, 0);
            let text = output(length);
            text.split(r#""label":"#)
                .skip(1)
                .map(|part| part.split(',').next().unwrap().to_owned())
                .collect::<Vec<_>>()
        };
        assert_eq!(labels(), [r#""b""#, "null", r#""a""#, "null"]);

        // The level of the rows in descending order: NULL comes last.
        let descending = [[1u32, 1, 0, 0, 0].as_slice(), &FIRST_LAYOUT].concat();
        // SAFETY: ten words.
        unsafe { lay_out(descending.as_ptr(), descending.len()) };
        assert_eq!(labels(), [r#""b""#, r#""a""#, "null", "null"]);

        // Tabular form, without the grand total of the rows: the shape follows.
        let tabular = [0u32, 0, 0, 0, 2, 0, 0, 1, 0];
        // SAFETY: nine words.
        let length = unsafe { lay_out(tabular.as_ptr(), tabular.len()) };
        assert_eq!(
            output(length),
            r#"{"lines":3,"columns":1,"rowLevels":1,"columnLevels":0,"groups":3,"rows":3}"#
        );

        let wrong = [1u32, 0, 0, 0, 0, 7];
        // SAFETY: six words.
        let length = unsafe { lay_out(wrong.as_ptr(), wrong.len()) };
        assert_eq!(output(length), r#"{"error":{"kind":"request"}}"#);
    }

    #[test]
    fn the_columns_named_by_their_values_give_their_position() {
        let families = [0u8, 1];
        // SAFETY: two families.
        unsafe { begin(families.as_ptr(), 2) };
        assert_eq!(append_column(0, &[Some("a"), Some("b"), None]), 0);
        assert_eq!(append_column(1, &[Some("3"), Some("1"), Some("2")]), 0);
        assert_eq!(finish(), 0);
        let named = [1u32, 0, 1, 1, 97];
        // SAFETY: five words.
        let length = unsafe { columns_at(named.as_ptr(), named.len()) };
        assert_eq!(output(length), r#"{"error":{"kind":"no_table"}}"#);

        // Columns: column 0; measure: SUM(column 1). Columns NULL, a, b, total.
        let words = [0u32, 1, 1, 0, 0, 2, 1];
        // SAFETY: seven words.
        unsafe { compute(words.as_ptr(), words.len()) };
        // b, NULL, the grand total, a column of a measure the table lacks, c.
        let named = [
            5u32, 0, 1, 1, 98, 0, 1, NULL_STEP, 0, 0, 1, 1, 1, 97, 0, 1, 1, 99,
        ];
        // SAFETY: eighteen words.
        let length = unsafe { columns_at(named.as_ptr(), named.len()) };
        assert_eq!(output(length), "[2,0,3,null,null]");
        let none = [0u32];
        // SAFETY: one word.
        let length = unsafe { columns_at(none.as_ptr(), none.len()) };
        assert_eq!(output(length), "[]");

        let refused = |words: &[u32]| named_columns(words).is_none();
        assert!(refused(&[]));
        assert!(refused(&[1, 0]));
        assert!(refused(&[1, 0, 1, 2, 97]));
        assert!(refused(&[0, 0]));
        // SAFETY: two words.
        let length = unsafe { columns_at([2u32, 0].as_ptr(), 2) };
        assert_eq!(output(length), r#"{"error":{"kind":"request"}}"#);
    }

    #[test]
    fn an_order_reads_the_path_of_a_node() {
        let text = "é";
        let mut words = vec![0u32, 1, 2, 1, 2, NULL_STEP, text.len() as u32];
        words.extend(text.bytes().map(u32::from));
        words.extend([0, 0]);
        words.extend(FIRST_LAYOUT);
        let (order, layout) = arrangement(&words).unwrap();
        assert_eq!(layout, Layout::default());
        assert_eq!(order.columns, AxisOrder::default());
        assert_eq!(
            order.rows,
            AxisOrder {
                descending: vec![],
                values: Some(ValueOrder {
                    path: vec![None, Some(text.to_owned())],
                    measure: 2,
                    descending: true,
                }),
            }
        );
    }

    #[test]
    fn a_layout_reads_its_form_and_the_totals_of_each_axis() {
        let (_, layout) = arrangement(&[0, 0, 0, 0, 1, 0, 2, 1, 0, 1, 1, 0]).unwrap();
        assert_eq!(
            layout,
            Layout {
                form: Form::Outline,
                rows: Totals {
                    grand_total: false,
                    subtotals: vec![true, false],
                },
                columns: Totals {
                    grand_total: true,
                    subtotals: vec![false],
                },
            }
        );
    }

    #[test]
    fn arrangements_that_do_not_match_their_words_are_refused() {
        let refused = |words: &[u32]| arrangement(words).is_none();
        assert!(refused(&[0, 0]));
        assert!(refused(&[0, 0, 0, 0]));
        assert!(refused(&[2, 1, 0, 0, 0]));
        assert!(refused(&[0, 1, 0, 0, 1, 5, 97, 0, 0]));
        // A text that is not UTF-8.
        assert!(refused(&[0, 1, 0, 0, 1, 1, 0xFF, 0, 0]));
        // An unknown form, totals cut short, a word too many.
        assert!(refused(&[0, 0, 0, 0, 3, 1, 0, 1, 0]));
        assert!(refused(&[0, 0, 0, 0, 0, 1, 2, 1, 1, 0]));
        assert!(refused(&[0, 0, 0, 0, 0, 1, 0, 1, 0, 9]));
    }

    #[test]
    fn a_level_reads_a_precision_of_its_dates() {
        let request = request(&[1, 1, 0, 3, 4, 7, 0]).unwrap();
        assert_eq!(
            request.rows,
            [Level {
                column: 3,
                precision: Some(Precision::Week),
            }]
        );
        assert_eq!(request.columns, [Level::from(7)]);
    }

    #[test]
    fn a_request_reads_its_filters() {
        let text = "é";
        let bytes = || text.bytes().map(u32::from);
        // Measure: COUNT(*). Filters: column 2 NOT IN (NULL, 'é'), SUM(column 4)
        // >= 'é' at the level of column 3, by month, then column 3 by year BETWEEN
        // 'é' AND 'é', and column 1 IS NOT NULL.
        let mut words = vec![
            0u32,
            0,
            1,
            0,
            0,
            0,
            2,
            0,
            1,
            2,
            NULL_STEP,
            text.len() as u32,
        ];
        words.extend(bytes());
        words.extend([1, 2, 4, 1, 3, 3, 5, text.len() as u32]);
        words.extend(bytes());
        words.extend([2, 3, 1, 6, text.len() as u32]);
        words.extend(bytes());
        words.push(text.len() as u32);
        words.extend(bytes());
        words.extend([2, 1, 0, 9]);
        let value = || text.to_owned();
        assert_eq!(
            request(&words).unwrap().filters,
            [
                Filter::Values {
                    level: 2.into(),
                    values: vec![None, Some(value())],
                    excluded: true,
                },
                Filter::Measure {
                    measure: Measure::Sum(4),
                    levels: vec![Level {
                        column: 3,
                        precision: Some(Precision::Month),
                    }],
                    test: Test::Compare(Comparison::GreaterOrEqual, value()),
                },
                Filter::Column {
                    level: Level {
                        column: 3,
                        precision: Some(Precision::Year),
                    },
                    test: Test::Between(value(), value()),
                },
                Filter::Column {
                    level: 1.into(),
                    test: Test::IsNotNull,
                },
            ]
        );
    }

    #[test]
    fn a_request_reads_each_test() {
        let tested = |words: &[u32]| {
            let mut all = vec![0u32, 0, 1, 0, 0, 2, 0, 0];
            all.extend_from_slice(words);
            match request(&all).unwrap().filters.remove(0) {
                Filter::Column { test, .. } => test,
                other => panic!("{other:?}"),
            }
        };
        let a = || "a".to_owned();
        assert_eq!(tested(&[0, 1, 97]), Test::Compare(Comparison::Equal, a()));
        assert_eq!(tested(&[6, 1, 97, 1, 97]), Test::Between(a(), a()));
        assert_eq!(tested(&[7, 1, 97, 1, 97]), Test::NotBetween(a(), a()));
        assert_eq!(tested(&[8]), Test::IsNull);
        assert_eq!(tested(&[9]), Test::IsNotNull);
    }

    #[test]
    fn filters_that_do_not_match_their_words_are_refused() {
        // An unknown kind, a missing value, an unknown test, a NULL operand, a
        // missing bound, an unknown precision.
        assert!(request(&[0, 0, 1, 0, 0, 7]).is_none());
        assert!(request(&[0, 0, 1, 0, 0, 0, 2, 0, 0, 1]).is_none());
        assert!(request(&[0, 0, 1, 0, 0, 1, 0, 0, 0, 10, 0]).is_none());
        assert!(request(&[0, 0, 1, 0, 0, 1, 0, 0, 0, 0, NULL_STEP]).is_none());
        assert!(request(&[0, 0, 1, 0, 0, 2, 0, 0, 6, 1, 97]).is_none());
        assert!(request(&[0, 0, 1, 0, 0, 2, 0, 9, 8]).is_none());
    }

    #[test]
    fn requests_that_do_not_match_their_words_are_refused() {
        assert_eq!(
            request(&[0, 0, 1, 0, 0]).unwrap().measures,
            [Measure::CountStar]
        );
        assert_eq!(
            request(&[0, 0, 1, 3, 4]).unwrap().measures,
            [Measure::CountDistinct(4)]
        );
        assert_eq!(
            request(&[0, 0, 5, 4, 1, 5, 2, 6, 3, 7, 4, 8, 5])
                .unwrap()
                .measures,
            [
                Measure::SumDistinct(1),
                Measure::Avg(2),
                Measure::AvgDistinct(3),
                Measure::Min(4),
                Measure::Max(5),
            ]
        );
        assert!(request(&[1, 0]).is_none());
        assert!(request(&[1, 0, 0]).is_none());
        assert!(request(&[0, 0, 1, 9, 0]).is_none());
        assert!(request(&[1, 0, 0, 0]).is_none());
        assert!(request(&[1, 0, 0, 0, 6]).is_none());
    }
}
