//! Streaming into the engine of Prisme itself (`Engine::update`): each slice
//! of rows is appended to the store of the engine, taken in (`Engine::grow`),
//! then the table of the profile is updated from the state of the previous
//! one. This is what the Worker of the page does during a reception.
//!
//! An update reads only the rows of its slice into the finest groups and the
//! states of their measures, then builds the trees, the grouping sets, their
//! merged states and the table again: this part follows the number of
//! groups. One engine per profile, as the page has one table per result.
//!
//! At the end, the engine checks itself: the table updated slice by slice
//! equals, cell by cell, the table it computes from all the rows. Without
//! that pass (a WebAssembly module holds 4 GB at most), the table has as many
//! finest groups as the prototype found.

use prisme_pivot::{Engine, Table};
use prisme_store::Store;

use crate::{COLUMNS, Generator, PROFILES, append, checkpoints, families, request, seconds};

/// Update of the engine by one slice, at one checkpoint.
pub struct Checkpoint {
    /// Rows received, the slice included.
    pub rows: usize,
    /// Duration of `grow`, which extends the caches of the columns to the
    /// slice, in seconds.
    pub grow_s: f64,
    /// Duration of `update`, which adds the slice to the state of the table
    /// and builds the table again, in seconds.
    pub update_s: f64,
    /// Finest groups of the table.
    pub finest_groups: usize,
}

/// Results of the streaming into the engine, for one profile.
pub struct Streamed {
    /// Name of the profile.
    pub name: &'static str,
    /// Updates at the checkpoints of the reception.
    pub checkpoints: Vec<Checkpoint>,
    /// Sum of `grow` over the whole reception, in seconds.
    pub grows_s: f64,
    /// Sum of `update` over the whole reception, in seconds.
    pub updates_s: f64,
    /// Longest `grow` and `update` of a slice together, in seconds.
    pub longest_update_s: f64,
    /// Table computed from all the rows at the end, the caches built, in
    /// seconds; without the pass on all the rows, none.
    pub full_s: Option<f64>,
    /// Does the table updated slice by slice equal the table of all the rows?
    pub checked: bool,
}

/// Do two tables hold the same lines, columns and values?
fn same(a: &Table, b: &Table) -> bool {
    a.finest_groups() == b.finest_groups()
        && a.selected_rows() == b.selected_rows()
        && a.line_count() == b.line_count()
        && a.column_count() == b.column_count()
        && (0..a.line_count()).all(|line| {
            (0..a.column_count()).all(|column| a.value(line, column) == b.value(line, column))
        })
}

/// Streams `total` rows by slices of `slice` rows into one engine per profile.
///
/// # Arguments
///
/// - `one_pass`: compute the table of all the rows at the end, to check the
///   table updated against it;
/// - `finest_groups`: finest groups of each profile, found by the prototype,
///   the check without that pass.
pub fn stream(
    total: usize,
    slice: usize,
    one_pass: bool,
    finest_groups: &[usize],
) -> Vec<Streamed> {
    let points = checkpoints(total, slice);
    PROFILES
        .iter()
        .zip(finest_groups)
        .map(|(&(name, rows), &finest_groups)| {
            let request = request(rows);
            let mut engine = Engine::new(Store::new(COLUMNS), families());
            let mut generator = Generator::new();
            let mut streamed = Streamed {
                name,
                checkpoints: Vec::new(),
                grows_s: 0.0,
                updates_s: 0.0,
                longest_update_s: 0.0,
                full_s: None,
                checked: false,
            };
            let mut received = 0;
            let mut last: Option<Table> = None;
            while received < total {
                let count = slice.min(total - received);
                append(engine.store_mut(), &generator.rows(count));
                received += count;
                // The table before leaves the memory first, as in the Worker,
                // which answers the page only between two computations.
                drop(last.take());
                let started = seconds();
                engine.grow();
                let grown = seconds();
                let table = engine.update(&request).expect("the table of the profile");
                let (grow_s, update_s) = (grown - started, seconds() - grown);
                streamed.grows_s += grow_s;
                streamed.updates_s += update_s;
                streamed.longest_update_s = streamed.longest_update_s.max(grow_s + update_s);
                if points.contains(&received) {
                    streamed.checkpoints.push(Checkpoint {
                        rows: received,
                        grow_s,
                        update_s,
                        finest_groups: table.finest_groups(),
                    });
                }
                last = Some(table);
            }
            let last = last.expect("one slice at least");
            if one_pass {
                let started = seconds();
                let full = engine.compute(&request).expect("the table of the profile");
                streamed.full_s = Some(seconds() - started);
                streamed.checked = same(&last, &full);
            } else {
                streamed.checked = last.finest_groups() == finest_groups;
            }
            streamed
        })
        .collect()
}

/// Memory of the engine streamed at one checkpoint, in bytes: what an
/// allocator that counts its bytes gives (`src/memory.rs`).
pub struct Memory {
    /// Rows received.
    pub rows: usize,
    /// Bytes held before the slice: the store, the caches of the columns and
    /// the state of the last table, the table itself dropped.
    pub held: usize,
    /// Most bytes held while `grow` extends the caches to the slice.
    pub grow_peak: usize,
    /// Most bytes held while `update` computes the table.
    pub update_peak: usize,
    /// Bytes held once the table is computed, the table included.
    pub with_table: usize,
}

/// Memory of the engine of one profile.
pub struct ProfileMemory {
    /// Name of the profile.
    pub name: &'static str,
    /// Memory at each checkpoint of the reception.
    pub checkpoints: Vec<Memory>,
    /// Bytes held by the engine sealed at the end, its last table dropped.
    pub sealed: usize,
}

/// Streams `total` rows by slices of `slice` rows into the engine of each
/// profile, and gives its memory at each checkpoint and once sealed at the
/// end (`Engine::seal`), its last table dropped, with that of the store of
/// all the rows alone.
///
/// # Arguments
///
/// - `live`: bytes held now, and most bytes held since the last call, which
///   starts the count of the most bytes again.
pub fn memory(
    total: usize,
    slice: usize,
    live: &dyn Fn() -> (usize, usize),
) -> (usize, Vec<ProfileMemory>) {
    let points = checkpoints(total, slice);
    let start = live().0;
    let mut store = Store::new(COLUMNS);
    let mut generator = Generator::new();
    let mut received = 0;
    while received < total {
        let count = slice.min(total - received);
        append(&mut store, &generator.rows(count));
        received += count;
    }
    let store_bytes = live().0 - start;
    drop(store);
    let profiles = PROFILES
        .iter()
        .map(|&(name, rows)| {
            let request = request(rows);
            let start = live().0;
            let mut engine = Engine::new(Store::new(COLUMNS), families());
            let mut generator = Generator::new();
            let mut measures = Vec::new();
            let mut received = 0;
            let mut last: Option<Table> = None;
            while received < total {
                let count = slice.min(total - received);
                let rows = generator.rows(count);
                drop(last.take());
                received += count;
                let checkpoint = points.contains(&received);
                append(engine.store_mut(), &rows);
                drop(rows);
                let held = live().0 - start;
                engine.grow();
                let grow_peak = live().1 - start;
                let table = engine.update(&request).expect("the table of the profile");
                let (with_table, update_peak) = live();
                if checkpoint {
                    measures.push(Memory {
                        rows: received,
                        held,
                        grow_peak,
                        update_peak: update_peak - start,
                        with_table: with_table - start,
                    });
                }
                last = Some(table);
            }
            drop(last);
            engine.seal();
            ProfileMemory {
                name,
                checkpoints: measures,
                sealed: live().0 - start,
            }
        })
        .collect();
    (store_bytes, profiles)
}
