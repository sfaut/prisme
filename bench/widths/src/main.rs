//! Benchmark of the default widths of the columns of the Pivoteur: what the
//! reading of the lengths of the texts of every column of values
//! (`Table::text_lengths`) adds to the computation of the table, the first
//! time, and to a new layout of it (`Table::lay_out`), which reads them again
//! from what the first reading kept, on the engine of Prisme
//! (`prisme-pivot`), native, on one thread.
//!
//! The rows and the pivots are those of the benchmark of the engine
//! (`bench/pivot`, `docs/PERFORMANCES.md`, chapter 9): the same generator and
//! seed, columns by year and month of `d`, rows on 4 levels, "high" (`pays`,
//! `region`, `categorie`, `produit`) and "low" (`canal` in place of
//! `produit`), 12 measures, every subtotal and total. Each step keeps the
//! fastest of 3 passes.
//!
//! `cargo run --release --manifest-path bench/widths/Cargo.toml --target-dir
//! target/widths-bench -- [rows]`, from the root of the repository,
//! 1,000,000 rows by default. The results go to stdout, as a Markdown table:
//! each reading with its share of the step it follows.

use std::time::{Duration, Instant};

use prisme_pivot::{Engine, Family, Layout, Level, Measure, Order, Precision, Request};
use prisme_store::Store;

/// Number of passes of each step; the fastest one is kept.
const PASSES: usize = 3;

/// Seed of the pseudo-random generator, that of `bench/pivot`.
const SEED: u32 = 42;

/// Days from 1970-01-01 to 2024-01-01, start of the dates of the rows.
const START_DAYS: i64 = 19_723;

/// Span of the dates of the rows: 1,096 days (2024 to 2026), in seconds.
const SPAN_SECONDS: u32 = 1_096 * 86_400;

/// Sales channels, values of the `canal` column.
const CHANNELS: [&str; 4] = ["web", "magasin", "telephone", "partenaire"];

/// Columns of the rows: `pays`, `region`, `categorie`, `produit`, `canal`, `d`,
/// `qte`, `montant`, `remise`, `client`.
const COLUMNS: usize = 10;

/// Length the page gives to NULL (`NULL`) and to a total (`Total`).
const NULL_LENGTH: usize = 4;
const TOTAL_LENGTH: usize = 5;

/// Pseudo-random generator Mulberry32, as in `bench/pivot`.
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

/// Year, month and day of a number of days since 1970-01-01, in the civil
/// calendar.
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let days = days + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    (year_of_era + era * 400 + i64::from(month <= 2), month, day)
}

/// Generates the rows into a store, as text cells, as `bench/pivot` does.
fn generate(count: usize) -> Store {
    let mut rng = Rng(SEED);
    let mut store = Store::new(COLUMNS);
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
        let cells = [
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
        ];
        store.push_row(&cells.each_ref().map(Option::as_deref));
    }
    store
}

/// Request of a profile: its 4 levels of rows, the year and the month of `d`
/// in columns, the 12 measures of `bench/pivot`.
fn request(rows: [usize; 4]) -> Request {
    let date = |precision| Level {
        column: 5,
        precision: Some(precision),
    };
    Request {
        rows: rows.into_iter().map(Level::from).collect(),
        columns: vec![date(Precision::Year), date(Precision::Month)],
        measures: vec![
            Measure::CountStar,
            Measure::Count(6),
            Measure::Sum(6),
            Measure::Sum(7),
            Measure::Avg(7),
            Measure::Avg(8),
            Measure::Min(7.into()),
            Measure::Max(7.into()),
            Measure::Min(5.into()),
            Measure::Max(5.into()),
            Measure::Min(9.into()),
            Measure::Max(9.into()),
        ],
        ..Request::default()
    }
}

/// Fastest of `PASSES` runs of a step, and the value of the last one.
fn fastest<T>(mut step: impl FnMut() -> T) -> (Duration, T) {
    let mut best = Duration::MAX;
    let mut value = None;
    for _ in 0..PASSES {
        let started = Instant::now();
        value = Some(step());
        best = best.min(started.elapsed());
    }
    (best, value.expect("one pass at least"))
}

/// Duration for the table: in milliseconds, to a tenth below 10 ms, or in
/// microseconds below 1 ms.
fn ms(duration: Duration) -> String {
    let ms = duration.as_secs_f64() * 1_000.0;
    if ms < 1.0 {
        format!("{:.0} µs", ms * 1_000.0)
    } else if ms < 10.0 {
        format!("{ms:.1} ms")
    } else {
        format!("{ms:.0} ms")
    }
}

fn main() {
    let count: usize = std::env::args()
        .nth(1)
        .map_or(1_000_000, |arg| arg.parse().expect("a number of rows"));
    eprintln!("generating {count} rows");
    let store = generate(count);
    let mut families = vec![Family::Other; COLUMNS];
    families[5] = Family::Datetime;
    families[6] = Family::Exact;
    families[7] = Family::Exact;
    families[8] = Family::Float;
    let engine = Engine::new(store, families);
    println!(
        "| {count} rows | Lines | Columns | Groups | Table | First reading | Layout | Next reading |"
    );
    println!("|---|---|---|---|---|---|---|---|");
    for (name, rows) in [("high", [0, 1, 2, 3]), ("low", [0, 1, 2, 4])] {
        eprintln!("profile {name}");
        let request = request(rows);
        // A first computation prepares the levels; the passes measure the
        // computation of the table alone.
        engine.compute(&request).expect("the table of the profile");
        let (table_time, _) = fastest(|| engine.compute(&request).expect("the table"));
        // Each pass of the first reading reads a table of its own.
        let mut tables: Vec<_> = (0..PASSES)
            .map(|_| engine.compute(&request).expect("the table"))
            .collect();
        let (first_time, lengths) = fastest(|| {
            let table = tables.pop().expect("one table per pass");
            table.text_lengths(&engine, NULL_LENGTH, TOTAL_LENGTH)
        });
        let mut table = engine.compute(&request).expect("the table");
        assert_eq!(
            lengths,
            table.text_lengths(&engine, NULL_LENGTH, TOTAL_LENGTH)
        );
        let (layout_time, ()) = fastest(|| {
            table
                .lay_out(&engine, &Order::default(), &Layout::default())
                .expect("the layout");
        });
        let (next_time, _) = fastest(|| table.text_lengths(&engine, NULL_LENGTH, TOTAL_LENGTH));
        let share = |reading: Duration, step: Duration| {
            format!(
                "{} ({:.0} %)",
                ms(reading),
                reading.as_secs_f64() / step.as_secs_f64() * 100.0
            )
        };
        println!(
            "| {name} | {} | {} | {} | {} | {} | {} | {} |",
            table.line_count(),
            table.column_count(),
            table.finest_groups(),
            ms(table_time),
            share(first_time, table_time),
            ms(layout_time),
            share(next_time, layout_time),
        );
    }
}
