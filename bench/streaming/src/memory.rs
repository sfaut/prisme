//! Memory of the engine of Prisme following the reception (see
//! `engine.rs`), natively: an allocator counts the bytes held and their
//! peak. The structures of the engine take the same bytes in WebAssembly, but
//! for their pointers and lengths (4 bytes there, 8 here); a WebAssembly
//! module holds 4 GB at most.
//!
//! `cargo run --release --manifest-path bench/streaming/Cargo.toml
//! --target-dir target/streaming-bench --bin streaming-memory -- [rows]
//! [slice]`, from the root of the repository: 1,000,000 rows by slices of
//! 100,000 by default. Prints one line per checkpoint, then the engine
//! sealed at the end of the reception, in MB.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

/// Bytes held.
static HELD: AtomicUsize = AtomicUsize::new(0);

/// Most bytes held since the last reading.
static PEAK: AtomicUsize = AtomicUsize::new(0);

/// Allocator of the system, which counts the bytes it holds.
struct Counting;

// SAFETY: every call goes to the allocator of the system, unchanged.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the layout is the caller's.
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            let held = HELD.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
            PEAK.fetch_max(held, Ordering::Relaxed);
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        // SAFETY: the pointer and the layout are the caller's.
        unsafe { System.dealloc(pointer, layout) };
        HELD.fetch_sub(layout.size(), Ordering::Relaxed);
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        // SAFETY: the pointer, the layout and the size are the caller's.
        let moved = unsafe { System.realloc(pointer, layout, size) };
        if !moved.is_null() {
            // The old block and the new one are both held during a move.
            let peak = HELD.load(Ordering::Relaxed) + size;
            PEAK.fetch_max(peak, Ordering::Relaxed);
            if size >= layout.size() {
                HELD.fetch_add(size - layout.size(), Ordering::Relaxed);
            } else {
                HELD.fetch_sub(layout.size() - size, Ordering::Relaxed);
            }
        }
        moved
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// Bytes held now, and most bytes held since the last call.
fn live() -> (usize, usize) {
    let held = HELD.load(Ordering::Relaxed);
    (held, PEAK.swap(held, Ordering::Relaxed))
}

fn main() {
    let mut args = std::env::args().skip(1);
    let mut number = |default: usize| {
        args.next()
            .map_or(default, |arg| arg.parse().expect("a number of rows"))
    };
    let total = number(1_000_000);
    let slice = number(100_000);
    let mb = |bytes: usize| bytes as f64 / 1e6;
    let (store, profiles) = streaming_bench::engine::memory(total, slice, &live);
    println!(
        "{total} rows by slices of {slice}; store of all the rows alone: {:.0} MB",
        mb(store)
    );
    println!(
        "profile | rows | held before the slice | peak of grow | peak of update | with the table"
    );
    for profile in &profiles {
        for memory in &profile.checkpoints {
            println!(
                "{} | {} | {:.0} | {:.0} | {:.0} | {:.0}",
                profile.name,
                memory.rows,
                mb(memory.held),
                mb(memory.grow_peak),
                mb(memory.update_peak),
                mb(memory.with_table)
            );
        }
    }
    for profile in &profiles {
        println!(
            "{}: sealed at the end, its table dropped: {:.0} MB",
            profile.name,
            mb(profile.sealed)
        );
    }
}
