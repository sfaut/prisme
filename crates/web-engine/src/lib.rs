//! WebAssembly side of the Pivoteur: the engine of `prisme-pivot`, compiled
//! to `wasm32-unknown-unknown` and run in a Worker of the page
//! (`web/src/components/pivotWorker.ts`), on one thread.
//!
//! The rows stay in the page, where the grid holds them: the Worker copies
//! them once into the memory of the module, block by block and column by
//! column (UTF-8 bytes, offset and validity of each cell, as in
//! `bench/pivot/js/transfer.mjs`), then asks for tables, and for the windows
//! of a table the page shows. A table never leaves the module whole.
//!
//! The functions are exported without `wasm-bindgen`: they take and return
//! numbers, and addresses in the memory of the module. Those that answer
//! with data write JSON (`json.rs`) and return its length in bytes;
//! `output_address` gives its address. A refused call answers
//! `{"error": {"kind", …}}`.

mod json;

use std::alloc::{Layout, alloc, dealloc, handle_alloc_error};
use std::cell::RefCell;
use std::slice;

use prisme_pivot::{Engine, Family, Measure, Request, Table};
use prisme_store::Store;

/// Alignment of the memory reserved for JavaScript, enough for the offsets
/// and the row numbers (`u32`).
const ALIGN: usize = 8;

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
fn layout(size: usize) -> Layout {
    Layout::from_size_align(size.max(1), ALIGN).expect("size below the memory of the module")
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

/// Family of a column from its code: 1 exact numbers, 2 floats, any other
/// code for the other families.
fn family(code: u8) -> Family {
    match code {
        1 => Family::Exact,
        2 => Family::Float,
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

/// Writes the distinct values of column `column`, by code: the page ranks
/// them in the order of the grid (`set_ranks`).
#[unsafe(no_mangle)]
pub extern "C" fn distinct(column: usize) -> usize {
    STATE.with_borrow_mut(|state| {
        answer(state, |json, state| match &state.engine {
            Some(engine) => match engine.distinct(column) {
                Ok(values) => json::distinct(json, &values),
                Err(error) => json::error(json, &error),
            },
            None => json::state_error(json, "no_result", None),
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

/// Reads a request: the numbers of levels of the rows and of the columns,
/// the number of measures, the columns of the levels of the rows, then of
/// the columns, then a kind (0 `COUNT(*)`, 1 `COUNT(col)`, 2 `SUM(col)`) and
/// a column per measure. `None` if the words do not match.
fn request(words: &[u32]) -> Option<Request> {
    let (&rows, &columns, &measures) = (words.first()?, words.get(1)?, words.get(2)?);
    let (rows, columns, measures) = (rows as usize, columns as usize, measures as usize);
    let rest = &words[3..];
    if rest.len() != rows + columns + 2 * measures {
        return None;
    }
    let (row_levels, rest) = rest.split_at(rows);
    let (column_levels, measure_words) = rest.split_at(columns);
    let measures = measure_words
        .chunks(2)
        .map(|pair| match pair[0] {
            0 => Some(Measure::CountStar),
            1 => Some(Measure::Count(pair[1] as usize)),
            2 => Some(Measure::Sum(pair[1] as usize)),
            _ => None,
        })
        .collect::<Option<Vec<_>>>()?;
    Some(Request {
        rows: row_levels.iter().map(|&column| column as usize).collect(),
        columns: column_levels
            .iter()
            .map(|&column| column as usize)
            .collect(),
        measures,
    })
}

/// Computes the table of a request of `length` words (`u32`) at `words` (see
/// `request`), keeps it, and writes its shape (`json::shape`) or an error.
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
        state.table = None;
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

        let length = distinct(0);
        assert_eq!(output(length), r#"["Nord","Sud"]"#);
        let ranks = [1u32, 0];
        // SAFETY: two ranks.
        let length = unsafe { set_ranks(0, ranks.as_ptr(), 2) };
        assert_eq!(output(length), "{}");

        // Rows: column 0; measures: COUNT(*), SUM(column 1).
        let words = [1u32, 0, 2, 0, 0, 0, 2, 1];
        // SAFETY: eight words.
        let length = unsafe { compute(words.as_ptr(), words.len()) };
        assert_eq!(
            output(length),
            r#"{"lines":3,"columns":2,"rowLevels":1,"columnLevels":0,"groups":2,"rows":3}"#
        );
        let length = window(0, 10, 0, 10);
        assert_eq!(
            output(length),
            concat!(
                r#"{"lines":[{"depth":1,"label":"Sud"},{"depth":1,"label":"Nord"},{"depth":0,"label":null}],"#,
                r#""columns":[{"node":0,"path":[],"total":false,"measure":0},{"node":0,"path":[],"total":false,"measure":1}],"#,
                r#""cells":[["1",null],["2","3.5"],["3","3.5"]]}"#
            )
        );

        let kept = [1u32];
        // SAFETY: one row number.
        let length = unsafe { set_kept(kept.as_ptr(), 1, 0) };
        assert_eq!(output(length), "{}");
        // SAFETY: eight words.
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
        assert_eq!(output(length), r#"{"error":{"kind":"no_table"}}"#);
    }

    #[test]
    fn requests_that_do_not_match_their_words_are_refused() {
        assert_eq!(
            request(&[0, 0, 1, 0, 0]).unwrap().measures,
            [Measure::CountStar]
        );
        assert!(request(&[1, 0]).is_none());
        assert!(request(&[1, 0, 0]).is_none());
        assert!(request(&[0, 0, 1, 7, 0]).is_none());
    }
}
