//! Benchmark of the Pivoteur's engine (point A1 of its specification), native
//! side: the engine of `pivot-engine`, run in the server.
//!
//! Generates the rows of a result as text cells, as the server receives them
//! (`Option<String>`), prepares typed columns once, then computes the pivot
//! several times, on one thread, then with the levels and the measures spread
//! over threads. The JSON of the whole pivot measures what the server would
//! send to the page. The other sides generate the same rows and must give the
//! same checksums; `../../run.mjs` runs them all and compares them.
//!
//! Progress goes to stderr, the results to stdout, as one JSON object.

use std::thread;
use std::time::Instant;

use pivot_engine::{
    COLUMNS, Column, PROFILES, Prepared, checksum, civil_from_days, compute, pivot_json,
    read_column, serialize, timings_json,
};

/// Cell of a result, as the server receives it: text, or `None` for NULL.
type Cell = Option<String>;

/// Number of passes of each computation; the fastest one is kept.
const PASSES: usize = 3;

/// Seed of the pseudo-random generator, shared with the JavaScript side.
const SEED: u32 = 42;

/// Days from 1970-01-01 to 2024-01-01, start of the dates of the rows.
const START_DAYS: i64 = 19_723;

/// Span of the dates of the rows: 1,096 days (2024 to 2026), in seconds.
const SPAN_SECONDS: u32 = 1_096 * 86_400;

/// Sales channels, values of the `canal` column.
const CHANNELS: [&str; 4] = ["web", "magasin", "telephone", "partenaire"];

/// Pseudo-random generator Mulberry32, written the same way on the
/// JavaScript side, so that both generate the same rows.
struct Rng(u32);

impl Rng {
    /// Next number, between 0 and 2^32 - 1.
    fn next(&mut self) -> u32 {
        self.0 = self.0.wrapping_add(0x6d2b_79f5);
        let a = self.0;
        let mut t = (a ^ (a >> 15)).wrapping_mul(1 | a);
        t = t.wrapping_add((t ^ (t >> 7)).wrapping_mul(61 | t)) ^ t;
        t ^ (t >> 14)
    }

    /// Next number, between 0 and `bound` - 1.
    fn below(&mut self, bound: u32) -> u32 {
        self.next() % bound
    }
}

/// Generates the rows: `pays`, `region`, `categorie`, `produit`, `canal`, `d`
/// (datetime), `qte` (integer, 5 % of NULL), `montant` (decimal with 2 digits),
/// `remise` (float), `client`.
fn generate(count: usize) -> Vec<Vec<Cell>> {
    let mut rng = Rng(SEED);
    let mut rows = Vec::with_capacity(count);
    for _ in 0..count {
        let country = rng.below(10);
        let region = rng.below(5);
        let category = rng.below(20);
        let product = rng.below(50);
        let channel = rng.below(4);
        let seconds = i64::from(rng.below(SPAN_SECONDS));
        let quantity = if rng.below(20) == 0 {
            None
        } else {
            Some(1 + rng.below(100))
        };
        let cents = 1 + rng.below(9_999_999);
        let discount = rng.below(500_000);
        let client = rng.below(100_000);
        let (year, month, day) = civil_from_days(START_DAYS + seconds / 86_400);
        let time = seconds % 86_400;
        rows.push(vec![
            Some(format!("P{country}")),
            Some(format!("P{country}-R{region}")),
            Some(format!("Cat{category:02}")),
            Some(format!("Cat{category:02}-P{product:02}")),
            Some(CHANNELS[channel as usize].to_owned()),
            Some(format!(
                "{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02}",
                time / 3_600,
                time / 60 % 60,
                time % 60
            )),
            quantity.map(|quantity| quantity.to_string()),
            Some(format!("{}.{:02}", cents / 100, cents % 100)),
            Some(format!("0.{discount:06}")),
            Some(format!("C{client:06}")),
        ]);
    }
    rows
}

/// Column of a result held as rows, as the server keeps it.
struct RowColumn<'a> {
    /// Rows of the result.
    rows: &'a [Vec<Cell>],
    /// Index of the column.
    column: usize,
}

impl Column for RowColumn<'_> {
    fn rows(&self) -> usize {
        self.rows.len()
    }

    fn cells(&self) -> impl Iterator<Item = Option<&str>> {
        self.rows.iter().map(|row| row[self.column].as_deref())
    }
}

/// Reads the typed columns, one column per thread if `parallel`.
fn prepare(rows: &[Vec<Cell>], parallel: bool) -> Prepared {
    let read = |index| {
        read_column(
            index,
            &RowColumn {
                rows,
                column: index,
            },
        )
    };
    let columns = if parallel {
        thread::scope(|scope| {
            let handles: Vec<_> = (0..COLUMNS)
                .map(|index| scope.spawn(move || read(index)))
                .collect();
            handles
                .into_iter()
                .map(|handle| handle.join().unwrap())
                .collect()
        })
    } else {
        (0..COLUMNS).map(read).collect()
    };
    Prepared::new(columns)
}

/// Memory counters of the process, from Windows.
#[cfg(windows)]
#[repr(C)]
#[derive(Default)]
struct MemoryCounters {
    cb: u32,
    page_fault_count: u32,
    peak_working_set_size: usize,
    working_set_size: usize,
    quota_peak_paged_pool_usage: usize,
    quota_paged_pool_usage: usize,
    quota_peak_non_paged_pool_usage: usize,
    quota_non_paged_pool_usage: usize,
    pagefile_usage: usize,
    peak_pagefile_usage: usize,
}

#[cfg(windows)]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetCurrentProcess() -> isize;
    fn K32GetProcessMemoryInfo(process: isize, counters: *mut MemoryCounters, size: u32) -> i32;
}

/// Private memory of the process and its peak, in gigabytes (0 outside
/// Windows).
fn memory() -> (f64, f64) {
    #[cfg(windows)]
    {
        let mut counters = MemoryCounters {
            cb: size_of::<MemoryCounters>() as u32,
            ..MemoryCounters::default()
        };
        // SAFETY: the counters are a valid PROCESS_MEMORY_COUNTERS, whose size
        // is given; the pseudo-handle of the current process needs no closing.
        let ok =
            unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, counters.cb) };
        if ok != 0 {
            return (
                counters.pagefile_usage as f64 / 1e9,
                counters.peak_pagefile_usage as f64 / 1e9,
            );
        }
    }
    (0.0, 0.0)
}

/// Fastest of several passes of a computation, with its result.
fn fastest<T>(mut run: impl FnMut() -> (T, f64)) -> (T, f64) {
    let mut best = run();
    for _ in 1..PASSES {
        let next = run();
        if next.1 < best.1 {
            best = next;
        }
    }
    best
}

fn main() {
    let count: usize = std::env::args()
        .nth(1)
        .map_or(10_000_000, |arg| arg.parse().expect("number of rows"));
    let threads = thread::available_parallelism().map_or(1, |threads| threads.get());

    eprintln!("native: generating {count} rows");
    let started = Instant::now();
    let rows = generate(count);
    let generation = started.elapsed().as_secs_f64();
    let after_generation = memory().0;

    // The preparation runs once per result: its first pass is measured.
    eprintln!("native: preparing the typed columns");
    let started = Instant::now();
    let prepared = prepare(&rows, false);
    let preparation = started.elapsed().as_secs_f64();
    let after_preparation = memory().0;
    let started = Instant::now();
    let again = prepare(&rows, true);
    let preparation_threads = started.elapsed().as_secs_f64();
    drop(again);

    let ranked = prepared.ranked();
    let labels = prepared.labels();
    let measures: Vec<_> = labels.iter().map(|&(measure, _)| measure).collect();
    let column_levels = prepared.column_levels();

    let mut profiles_json = Vec::new();
    for (name, columns) in PROFILES {
        let row_levels = prepared.row_levels(&columns);
        let mut runs = Vec::new();
        let mut pivots = Vec::new();
        for parallel in [false, true] {
            eprintln!(
                "native: profile {name}, {}",
                if parallel {
                    "all threads"
                } else {
                    "one thread"
                }
            );
            let (pivot, total) = fastest(|| {
                let started = Instant::now();
                let pivot = compute(&row_levels, &column_levels, &measures, parallel);
                let total = started.elapsed().as_secs_f64();
                (pivot, total)
            });
            runs.push(timings_json(&pivot, total));
            pivots.push(pivot);
        }
        let pivot = &pivots[0];
        assert_eq!(
            checksum(&pivot.states),
            checksum(&pivots[1].states),
            "same states on all threads"
        );

        eprintln!("native: profile {name}, serialization");
        let started = Instant::now();
        let json = serialize(pivot, &labels, &ranked);
        let serialization = started.elapsed().as_secs_f64();

        profiles_json.push(format!(
            "{{\"name\":\"{name}\",\"one_thread\":{},\"all_threads\":{},{},\"serialization\":{{\"bytes\":{},\"seconds\":{serialization:.4}}}}}",
            runs[0],
            runs[1],
            pivot_json(pivot, &labels, &ranked),
            json.len(),
        ));
    }

    let (_, peak) = memory();
    println!(
        "{{\"engine\":\"native\",\"rows\":{count},\"threads\":{threads},\"generation_s\":{generation:.2},\"preparation\":{{\"one_thread_s\":{preparation:.4},\"all_threads_s\":{preparation_threads:.4}}},\"memory_gb\":{{\"after_generation\":{after_generation:.2},\"after_preparation\":{after_preparation:.2},\"peak\":{peak:.2}}},\"profiles\":[{}]}}",
        profiles_json.join(",")
    );
}
