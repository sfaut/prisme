//! Benchmark of the Pivoteur's engine (point A1 of its specification),
//! WebAssembly side: the engine of `pivot-engine`, compiled to
//! `wasm32-unknown-unknown` and run by V8, as the page would run it.
//!
//! The rows stay in JavaScript, where the grid holds them. The module reads
//! only its own memory: `../js/wasm.mjs` copies the text cells of each column
//! into it (UTF-8 bytes, offset and validity of each cell), and the module
//! reads them into typed columns. The pivot is computed in the module, on one
//! thread (`wasm32-unknown-unknown` has no other), and only its results go
//! back to JavaScript.
//!
//! The functions are exported without `wasm-bindgen`: they take and return
//! numbers, and addresses in the memory of the module.

use std::alloc::{Layout, alloc, dealloc, handle_alloc_error};
use std::cell::RefCell;
use std::slice;

use pivot_engine::{
    COLUMNS, Column, PROFILES, Pivot, Prepared, Typed, compute as compute_pivot, pivot_json,
    seconds,
};

/// Alignment of the memory reserved for JavaScript, enough for the offsets
/// (`u32`).
const ALIGN: usize = 8;

/// Column copied by JavaScript: UTF-8 text of the cells end to end, offsets
/// and validity of the cells.
struct TextColumn<'a> {
    /// Text of the cells.
    text: &'a str,
    /// Offset of each cell in `text`, and the end of the last one.
    offsets: &'a [u32],
    /// Validity of each cell (0 for NULL).
    valid: &'a [u8],
}

impl Column for TextColumn<'_> {
    fn rows(&self) -> usize {
        self.valid.len()
    }

    fn cells(&self) -> impl Iterator<Item = Option<&str>> {
        self.valid
            .iter()
            .zip(self.offsets.windows(2))
            .map(|(&valid, bounds)| {
                (valid != 0).then(|| &self.text[bounds[0] as usize..bounds[1] as usize])
            })
    }
}

/// State of the module between two calls.
#[derive(Default)]
struct Bench {
    /// Typed columns read so far, in the order of the columns.
    columns: Vec<Typed>,
    /// Typed columns of the result, once all are read.
    prepared: Option<Prepared>,
    /// Last pivot computed.
    pivot: Option<Pivot>,
    /// Last report written, as JSON.
    report: String,
}

thread_local! {
    /// State of the module, on its only thread.
    static BENCH: RefCell<Bench> = RefCell::default();
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

/// Reads column `index` of the result from its copy: `length` bytes of UTF-8
/// text at `bytes`, `rows + 1` offsets at `offsets` (cell `row` runs from
/// `offsets[row]` to `offsets[row + 1]`), one validity byte per row at `valid`
/// (0 for NULL). The columns are read in their order; once the last one is
/// read, the typed columns are assembled. JavaScript frees the copy
/// afterwards.
///
/// # Safety
///
/// The three addresses come from `reserve`, hold what is described above,
/// and stay untouched during the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn read_column(
    index: usize,
    bytes: *const u8,
    length: usize,
    offsets: *const u32,
    valid: *const u8,
    rows: usize,
) {
    // SAFETY: given by the caller.
    let (bytes, offsets, valid) = unsafe {
        (
            slice::from_raw_parts(bytes, length),
            slice::from_raw_parts(offsets, rows + 1),
            slice::from_raw_parts(valid, rows),
        )
    };
    let column = TextColumn {
        text: std::str::from_utf8(bytes).expect("cells in UTF-8"),
        offsets,
        valid,
    };
    let typed = pivot_engine::read_column(index, &column);
    BENCH.with_borrow_mut(|bench| {
        assert_eq!(index, bench.columns.len(), "columns read in their order");
        bench.columns.push(typed);
        if bench.columns.len() == COLUMNS {
            bench.prepared = Some(Prepared::new(std::mem::take(&mut bench.columns)));
        }
    });
}

/// Computes the pivot of profile `profile` (index in `PROFILES`), keeps it,
/// and returns the duration of the computation, in seconds.
#[unsafe(no_mangle)]
pub extern "C" fn compute(profile: usize) -> f64 {
    BENCH.with_borrow_mut(|bench| {
        // The previous pivot is freed before the measure, as on the native side.
        bench.pivot = None;
        let prepared = bench.prepared.as_ref().expect("columns read");
        let measures: Vec<_> = prepared
            .labels()
            .iter()
            .map(|&(measure, _)| measure)
            .collect();
        let row_levels = prepared.row_levels(&PROFILES[profile].1);
        let column_levels = prepared.column_levels();
        let started = seconds();
        let pivot = compute_pivot(&row_levels, &column_levels, &measures, false);
        let total = seconds() - started;
        bench.pivot = Some(pivot);
        total
    })
}

/// Duration of phase `index` of the last pivot, in seconds: levels, groups,
/// grouping sets, measures.
#[unsafe(no_mangle)]
pub extern "C" fn phase(index: usize) -> f64 {
    BENCH.with_borrow(|bench| bench.pivot.as_ref().expect("pivot computed").phases[index])
}

/// Writes the report of the last pivot (JSON fields of `pivot_json`) and
/// returns its length in bytes; `report_address` gives its address.
#[unsafe(no_mangle)]
pub extern "C" fn report() -> usize {
    BENCH.with_borrow_mut(|bench| {
        let prepared = bench.prepared.as_ref().expect("columns read");
        let pivot = bench.pivot.as_ref().expect("pivot computed");
        bench.report = pivot_json(pivot, &prepared.labels(), &prepared.ranked());
        bench.report.len()
    })
}

/// Address of the last report written by `report`.
#[unsafe(no_mangle)]
pub extern "C" fn report_address() -> *const u8 {
    BENCH.with_borrow(|bench| bench.report.as_ptr())
}
