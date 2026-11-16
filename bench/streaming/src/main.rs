//! Native side of the benchmark of the streaming of the rows to the Pivoteur
//! (see `lib.rs`): runs it on one thread and prints its results as JSON.
//!
//! `cargo run --release --manifest-path bench/streaming/Cargo.toml
//! --target-dir target/streaming-bench -- [rows] [slice]`, from the root of
//! the repository: 1,000,000 rows by slices of 100,000 by default.
//! `bench/streaming/run.mjs` runs it with the other sides.

fn main() {
    let mut args = std::env::args().skip(1);
    let mut number = |default: usize| {
        args.next()
            .map_or(default, |arg| arg.parse().expect("a number of rows"))
    };
    let total = number(1_000_000);
    let slice = number(100_000);
    eprintln!("native: {total} rows by slices of {slice}");
    println!("{}", streaming_bench::run(total, slice, true));
}
