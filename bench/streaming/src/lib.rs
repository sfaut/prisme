//! Benchmark of the streaming of the rows to the Pivoteur: what it costs to
//! keep a pivot table up to date while the rows of a result arrive, slice by
//! slice, on one thread.
//!
//! Two ways are measured on the same rows and the same tables:
//!
//! - **full computation**, the engine of Prisme as it is (`prisme-pivot`): a
//!   new engine on all the rows received so far, whose first table builds
//!   the caches of its columns (dictionaries, numbers, date levels), then
//!   the table; measured at a few checkpoints of the reception;
//! - **incremental aggregates**, a prototype ([`Incremental`]): each slice
//!   extends the dictionaries of the levels, finds the finest group of each
//!   new row through a hash table, and adds the row to the state of that
//!   group and of each of its subtotals and totals, whose groups each finest
//!   group knows from its creation. The cost of an update follows the size
//!   of the slice, not the number of rows already received, nor the number
//!   of groups;
//! - **the engine of Prisme streamed** (`engine.rs`): each slice is appended
//!   to the store of the engine and added to the state of its last table
//!   (`Engine::update`), which then builds its grouping sets and its table
//!   again.
//!
//! The appending of each slice to the store of the rows (`prisme-store`) is
//! measured apart: both ways need it.
//!
//! The rows and the tables are those of `bench/widths` (`docs/PERFORMANCES.md`,
//! chapter 15): the same generator and seed; rows on 4 levels, "high"
//! (`pays`, `region`, `categorie`, `produit`) and "low" (`canal` in place of
//! `produit`); columns by year and month of `d`; 12 measures (`COUNT(*)`,
//! `COUNT`, `SUM`, `AVG`, `MIN`, `MAX` on integers, decimals, a float,
//! datetimes and texts), every subtotal and total. These measures are all
//! decomposable: a state per group holds them, and a new row only adds to it.
//!
//! At the end, the prototype checks itself: the states it kept up to date
//! slice by slice equal those of one pass on all the rows, and it found as
//! many finest groups as the engine of Prisme. The streamed engine checks
//! its table against the one it computes from all the rows.
//!
//! [`run`] gives the results as JSON; `src/main.rs` runs it natively, and
//! `wasm.mjs` compiled to WebAssembly (see `run.mjs`).

use std::collections::HashMap;
use std::fmt::Write as _;
use std::hash::{BuildHasherDefault, Hasher};

use prisme_pivot::{Engine, Family, Level, Measure, Precision, Request};
use prisme_store::Store;

mod engine;

/// Seed of the pseudo-random generator, that of `bench/pivot` and
/// `bench/widths`.
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

/// Indexes of the columns read by the measures.
const DATE: usize = 5;
const QUANTITY: usize = 6;
const AMOUNT: usize = 7;
const DISCOUNT: usize = 8;
const CLIENT: usize = 9;

/// Profiles: name, and columns of the row levels, the outermost first.
pub const PROFILES: [(&str, [usize; 4]); 2] = [("high", [0, 1, 2, 3]), ("low", [0, 1, 2, 4])];

/// Levels of a table: 4 of rows, then 2 of columns (year, month).
const LEVELS: usize = 6;

/// Grouping sets of a table: 0 to 4 levels of rows kept, by 0 to 2 levels of
/// columns kept; the finest set keeps them all.
const SETS: usize = 5 * 3;

/// Seconds elapsed since the first call, to measure durations.
#[cfg(not(target_arch = "wasm32"))]
pub fn seconds() -> f64 {
    use std::sync::OnceLock;
    use std::time::Instant;

    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_secs_f64()
}

/// Seconds given by the clock of the page, imported by the module:
/// `std::time::Instant` has no clock on `wasm32-unknown-unknown`.
#[cfg(target_arch = "wasm32")]
pub fn seconds() -> f64 {
    #[link(wasm_import_module = "env")]
    unsafe extern "C" {
        /// `performance.now()`, in milliseconds.
        safe fn now() -> f64;
    }
    now() / 1000.0
}

/// Hasher of rustc (FxHash), as in `bench/pivot`: the table of the groups
/// needs a fast hash, not one that resists collision attacks.
#[derive(Default)]
struct FxHasher {
    /// Hash computed so far.
    hash: u64,
}

impl FxHasher {
    /// Mixes a word into the hash.
    #[inline]
    fn add(&mut self, word: u64) {
        self.hash = (self.hash.rotate_left(5) ^ word).wrapping_mul(0x51_7c_c1_b7_27_22_0a_95);
    }
}

impl Hasher for FxHasher {
    fn write(&mut self, bytes: &[u8]) {
        for chunk in bytes.chunks(8) {
            let mut word = [0u8; 8];
            word[..chunk.len()].copy_from_slice(chunk);
            self.add(u64::from_le_bytes(word));
        }
    }

    fn write_u8(&mut self, value: u8) {
        self.add(u64::from(value));
    }

    fn write_u32(&mut self, value: u32) {
        self.add(u64::from(value));
    }

    fn write_u64(&mut self, value: u64) {
        self.add(value);
    }

    fn write_i64(&mut self, value: i64) {
        self.add(value as u64);
    }

    fn write_usize(&mut self, value: usize) {
        self.add(value as u64);
    }

    /// The rotation brings the bits mixed best down to where the table picks
    /// its bucket (fix of `rustc-hash` 2).
    fn finish(&self) -> u64 {
        self.hash.rotate_left(26)
    }
}

/// Hash table with the fast hasher.
type FxMap<K, V> = HashMap<K, V, BuildHasherDefault<FxHasher>>;

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

/// Days since 1970-01-01 of a date of the civil calendar (H. Hinnant's
/// algorithm).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let day_of_year = (153 * ((month + 9) % 12) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
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

/// Year of a number of days since 1970-01-01, as the precision "Année" of
/// the Pivoteur reads it: the year of its calendar
/// (`crates/pivot/src/dates.rs`).
fn calendar_year(days: i64) -> i64 {
    civil_from_days(days).0
}

/// Month of a number of days since 1970-01-01, as the precision "Mois" of
/// the Pivoteur reads it: a period of its calendar year, `2026-01` read as
/// 202,601.
fn month_period(days: i64) -> i64 {
    let (year, month, _) = civil_from_days(days);
    year * 100 + month
}

/// Generator of the rows, as text cells, as `bench/widths` writes them; its
/// rows follow one another from one call to the next.
struct Generator(Rng);

impl Generator {
    /// Generator at the first row.
    fn new() -> Self {
        Generator(Rng(SEED))
    }

    /// Next `count` rows, one cell per column (`None` for NULL).
    fn rows(&mut self, count: usize) -> Vec<[Option<String>; COLUMNS]> {
        let rng = &mut self.0;
        (0..count)
            .map(|_| {
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
                [
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
                ]
            })
            .collect()
    }
}

/// Appends rows to a store.
fn append(store: &mut Store, rows: &[[Option<String>; COLUMNS]]) {
    for row in rows {
        store.push_row(&row.each_ref().map(Option::as_deref));
    }
}

/// Store of the first `count` rows, generated again from the first one.
fn store_of(count: usize) -> Store {
    let mut store = Store::new(COLUMNS);
    let mut generator = Generator::new();
    let mut done = 0;
    while done < count {
        let slice = (count - done).min(100_000);
        append(&mut store, &generator.rows(slice));
        done += slice;
    }
    store
}

/// Families of the columns, for the engine of Prisme.
fn families() -> Vec<Family> {
    let mut families = vec![Family::Other; COLUMNS];
    families[DATE] = Family::Datetime;
    families[QUANTITY] = Family::Exact;
    families[AMOUNT] = Family::Exact;
    families[DISCOUNT] = Family::Float;
    families
}

/// Request of a profile for the engine of Prisme: its 4 levels of rows, the
/// year and the month of `d` in columns, the 12 measures.
fn request(rows: [usize; 4]) -> Request {
    let date = |precision| Level {
        column: DATE,
        precision: Some(precision),
    };
    Request {
        rows: rows.into_iter().map(Level::from).collect(),
        columns: vec![date(Precision::Year), date(Precision::Month)],
        measures: vec![
            Measure::CountStar,
            Measure::Count(QUANTITY),
            Measure::Sum(QUANTITY),
            Measure::Sum(AMOUNT),
            Measure::Avg(AMOUNT),
            Measure::Avg(DISCOUNT),
            Measure::Min(AMOUNT.into()),
            Measure::Max(AMOUNT.into()),
            Measure::Min(DATE.into()),
            Measure::Max(DATE.into()),
            Measure::Min(CLIENT.into()),
            Measure::Max(CLIENT.into()),
        ],
        ..Request::default()
    }
}

/// Value of a run of ASCII digits, `None` if one byte is not a digit.
fn digits_value(digits: &[u8]) -> Option<i64> {
    let mut value = 0i64;
    for &byte in digits {
        if !byte.is_ascii_digit() {
            return None;
        }
        value = value * 10 + i64::from(byte - b'0');
    }
    Some(value)
}

/// Reads an integer, with an optional minus sign.
fn parse_integer(text: &str) -> Option<i64> {
    let (sign, digits) = match text.as_bytes().first() {
        Some(b'-') => (-1, &text.as_bytes()[1..]),
        _ => (1, text.as_bytes()),
    };
    if digits.is_empty() {
        return None;
    }
    digits_value(digits).map(|value| sign * value)
}

/// Reads a decimal with 2 digits after the point as hundredths.
fn parse_cents(text: &str) -> Option<i64> {
    let (whole, fraction) = text.split_once('.')?;
    if fraction.len() != 2 {
        return None;
    }
    Some(digits_value(whole.as_bytes())? * 100 + digits_value(fraction.as_bytes())?)
}

/// Reads a `YYYY-MM-DD HH:MM:SS` datetime as seconds since 1970-01-01.
fn parse_datetime(text: &str) -> Option<i64> {
    let text = text.as_bytes();
    if text.len() != 19 {
        return None;
    }
    let days = days_from_civil(
        digits_value(&text[0..4])?,
        digits_value(&text[5..7])?,
        digits_value(&text[8..10])?,
    );
    Some(
        days * 86_400
            + digits_value(&text[11..13])? * 3_600
            + digits_value(&text[14..16])? * 60
            + digits_value(&text[17..19])?,
    )
}

/// Dictionary of a level that grows: the codes are given in the order of the
/// first appearance of the values, so the codes already given never change.
#[derive(Default)]
struct Dictionary {
    /// Code of each value.
    index: FxMap<Box<str>, u32>,
    /// Value of each code; NULL has its own code.
    values: Vec<Option<Box<str>>>,
    /// Code of NULL, once it has appeared.
    null: Option<u32>,
}

impl Dictionary {
    /// Code of a cell, given to it if it is new.
    fn code(&mut self, cell: Option<&str>) -> u32 {
        let Some(text) = cell else {
            return self.null_code();
        };
        if let Some(&code) = self.index.get(text) {
            return code;
        }
        let code = self.values.len() as u32;
        self.values.push(Some(text.into()));
        self.index.insert(text.into(), code);
        code
    }

    /// Code of NULL, given to it on its first appearance.
    fn null_code(&mut self) -> u32 {
        let values = &mut self.values;
        *self.null.get_or_insert_with(|| {
            values.push(None);
            (values.len() - 1) as u32
        })
    }

    /// Text of a code, `None` for NULL.
    fn text(&self, code: u32) -> Option<&str> {
        self.values[code as usize].as_deref()
    }
}

/// No value yet, for a minimum or a maximum of codes.
const NO_CODE: u32 = u32::MAX;

/// States of the 12 measures, one entry per group, in columns.
#[derive(Default)]
struct States {
    /// `COUNT(*)`.
    rows: Vec<u64>,
    /// `COUNT(qte)`, and the count of `SUM(qte)`.
    quantity_count: Vec<u64>,
    /// `SUM(qte)`.
    quantity_sum: Vec<i64>,
    /// Count of `SUM(montant)` and `AVG(montant)`.
    amount_count: Vec<u64>,
    /// `SUM(montant)`, in hundredths, with the sum of `AVG(montant)`.
    amount_sum: Vec<i128>,
    /// Count of `AVG(remise)`.
    discount_count: Vec<u64>,
    /// Sum of `AVG(remise)`.
    discount_sum: Vec<f64>,
    /// `MIN(montant)` and `MAX(montant)`, in hundredths.
    amount_min: Vec<i64>,
    amount_max: Vec<i64>,
    /// `MIN(d)` and `MAX(d)`, in seconds.
    date_min: Vec<i64>,
    date_max: Vec<i64>,
    /// `MIN(client)` and `MAX(client)`: codes of the dictionary of `client`,
    /// compared by their text.
    client_min: Vec<u32>,
    client_max: Vec<u32>,
}

impl States {
    /// Number of groups.
    fn len(&self) -> usize {
        self.rows.len()
    }

    /// Adds `count` groups without rows.
    fn grow(&mut self, count: usize) {
        let len = self.len() + count;
        self.rows.resize(len, 0);
        self.quantity_count.resize(len, 0);
        self.quantity_sum.resize(len, 0);
        self.amount_count.resize(len, 0);
        self.amount_sum.resize(len, 0);
        self.discount_count.resize(len, 0);
        self.discount_sum.resize(len, 0.0);
        self.amount_min.resize(len, i64::MAX);
        self.amount_max.resize(len, i64::MIN);
        self.date_min.resize(len, i64::MAX);
        self.date_max.resize(len, i64::MIN);
        self.client_min.resize(len, NO_CODE);
        self.client_max.resize(len, NO_CODE);
    }

    /// Merges the state of group `from` into group `into` of the same states.
    fn merge_within(&mut self, from: usize, into: usize, clients: &Dictionary) {
        self.rows[into] += self.rows[from];
        self.quantity_count[into] += self.quantity_count[from];
        self.quantity_sum[into] += self.quantity_sum[from];
        self.amount_count[into] += self.amount_count[from];
        self.amount_sum[into] += self.amount_sum[from];
        self.discount_count[into] += self.discount_count[from];
        self.discount_sum[into] += self.discount_sum[from];
        self.amount_min[into] = self.amount_min[into].min(self.amount_min[from]);
        self.amount_max[into] = self.amount_max[into].max(self.amount_max[from]);
        self.date_min[into] = self.date_min[into].min(self.date_min[from]);
        self.date_max[into] = self.date_max[into].max(self.date_max[from]);
        let (low, high) = (self.client_min[from], self.client_max[from]);
        if low != NO_CODE && text_before(clients, low, self.client_min[into]) {
            self.client_min[into] = low;
        }
        if high != NO_CODE && text_after(clients, high, self.client_max[into]) {
            self.client_max[into] = high;
        }
    }

    /// Merges the state of group `from` of `delta` into group `into`.
    fn merge(&mut self, delta: &States, from: usize, into: usize, clients: &Dictionary) {
        self.rows[into] += delta.rows[from];
        self.quantity_count[into] += delta.quantity_count[from];
        self.quantity_sum[into] += delta.quantity_sum[from];
        self.amount_count[into] += delta.amount_count[from];
        self.amount_sum[into] += delta.amount_sum[from];
        self.discount_count[into] += delta.discount_count[from];
        self.discount_sum[into] += delta.discount_sum[from];
        self.amount_min[into] = self.amount_min[into].min(delta.amount_min[from]);
        self.amount_max[into] = self.amount_max[into].max(delta.amount_max[from]);
        self.date_min[into] = self.date_min[into].min(delta.date_min[from]);
        self.date_max[into] = self.date_max[into].max(delta.date_max[from]);
        let (low, high) = (delta.client_min[from], delta.client_max[from]);
        if low != NO_CODE && text_before(clients, low, self.client_min[into]) {
            self.client_min[into] = low;
        }
        if high != NO_CODE && text_after(clients, high, self.client_max[into]) {
            self.client_max[into] = high;
        }
    }
}

/// Is the text of code `a` before that of code `b`, `b` without value?
fn text_before(clients: &Dictionary, a: u32, b: u32) -> bool {
    b == NO_CODE || clients.text(a) < clients.text(b)
}

/// Is the text of code `a` after that of code `b`, `b` without value?
fn text_after(clients: &Dictionary, a: u32, b: u32) -> bool {
    b == NO_CODE || clients.text(a) > clients.text(b)
}

/// Durations of the phases of the updates, in seconds: codes of the levels
/// and values of the measures, finest groups, states of the slice, merge into
/// the states kept.
pub type Phases = [f64; 4];

/// Prototype of incremental aggregates for one table, in the shape of the
/// engine of Prisme: each slice is read column by column, aggregated into
/// states of its own, then merged into the states kept from the slices
/// before, every subtotal and total included.
pub struct Incremental {
    /// Columns of the levels of the rows.
    row_columns: [usize; 4],
    /// Dictionary of each level of the rows.
    dictionaries: [Dictionary; 4],
    /// Dictionary of `client`, read by `MIN` and `MAX`.
    clients: Dictionary,
    /// Code of each year and of each month of `d` (with NULL).
    years: FxMap<Option<i64>, u32>,
    months: FxMap<Option<i64>, u32>,
    /// Number of the finest group of each key: the codes of the levels, 21
    /// bits each.
    finest: FxMap<u128, u32>,
    /// Group of each key of each coarser grouping set: the codes of the
    /// levels the set keeps, `NO_CODE` in place of those it leaves out.
    index: Vec<FxMap<[u32; LEVELS], u32>>,
    /// Groups of each finest group in every grouping set, the finest one
    /// first, found once when the finest group is created; the groups of all
    /// the sets share one numbering.
    parents: Vec<[u32; SETS]>,
    /// Group of each group in the next coarser sets of the chain of merges:
    /// one column level less (0), one row level less (1, only from the sets
    /// that keep both column levels); `NO_CODE` without one.
    up: Vec<[u32; 2]>,
    /// States of the groups, every grouping set included.
    states: States,
    /// Group of the slice of each group touched by the slice being read,
    /// `NO_CODE` for the others.
    local: Vec<u32>,
    /// Durations of the phases of the updates, summed.
    pub phases: Phases,
}

/// Levels kept by each grouping set: (levels of rows, levels of columns),
/// the finest set first.
fn kept_levels() -> [(usize, usize); SETS] {
    let mut sets = [(0, 0); SETS];
    let mut set = 0;
    for rows in (0..=4).rev() {
        for columns in (0..=2).rev() {
            sets[set] = (rows, columns);
            set += 1;
        }
    }
    sets
}

/// Source of each coarser grouping set in the chain of merges, and the slot
/// of `Incremental::up` that leads to it: one column level more if the set
/// leaves one out, else one row level more (as in the engine of Prisme).
fn sources() -> [(usize, usize); SETS] {
    let sets = kept_levels();
    let position = |kept: (usize, usize)| {
        sets.iter()
            .position(|&set| set == kept)
            .expect("a grouping set")
    };
    std::array::from_fn(|set| {
        let (rows, columns) = sets[set];
        match set {
            0 => (0, 0),
            _ if columns < 2 => (position((rows, columns + 1)), 0),
            _ => (position((rows + 1, columns)), 1),
        }
    })
}

/// Key of the finest group of the codes of the levels, 21 bits each.
fn finest_key(codes: &[u32; LEVELS]) -> u128 {
    codes
        .iter()
        .fold(0u128, |key, &code| (key << 21) | u128::from(code))
}

/// Code of a value in a small dictionary of numbers, given to it if it is new.
fn small_code(codes: &mut FxMap<Option<i64>, u32>, value: Option<i64>) -> u32 {
    let next = codes.len() as u32;
    *codes.entry(value).or_insert(next)
}

/// Values of a column of the store for rows `from..to`, read by `parse`.
fn read<T>(
    store: &Store,
    column: usize,
    from: usize,
    to: usize,
    mut parse: impl FnMut(&str) -> Option<T>,
) -> Vec<Option<T>> {
    let column = store.column(column).expect("a column of the rows");
    (from..to)
        .map(|row| column.get(row).and_then(&mut parse))
        .collect()
}

impl Incremental {
    /// Table of a profile, without rows yet.
    pub fn new(row_columns: [usize; 4]) -> Self {
        Incremental {
            row_columns,
            dictionaries: Default::default(),
            clients: Dictionary::default(),
            years: FxMap::default(),
            months: FxMap::default(),
            finest: FxMap::default(),
            index: (0..SETS).map(|_| FxMap::default()).collect(),
            parents: Vec::new(),
            up: Vec::new(),
            states: States::default(),
            local: Vec::new(),
            phases: [0.0; 4],
        }
    }

    /// Adds rows `from..to` of the store to the table.
    pub fn update(&mut self, store: &Store, from: usize, to: usize) {
        // Codes of the levels, values of the measures.
        let started = seconds();
        let mut codes: [Vec<u32>; LEVELS] = Default::default();
        for (level, &column) in self.row_columns.iter().enumerate() {
            let dictionary = &mut self.dictionaries[level];
            let cells = store.column(column).expect("a column of the rows");
            codes[level] = (from..to)
                .map(|row| dictionary.code(cells.get(row)))
                .collect();
        }
        let dates = read(store, DATE, from, to, parse_datetime);
        let (years, months) = (&mut self.years, &mut self.months);
        (codes[4], codes[5]) = dates
            .iter()
            .map(|date| {
                let days = date.map(|seconds| seconds.div_euclid(86_400));
                (
                    small_code(years, days.map(calendar_year)),
                    small_code(months, days.map(month_period)),
                )
            })
            .unzip();
        let quantities = read(store, QUANTITY, from, to, parse_integer);
        let amounts = read(store, AMOUNT, from, to, parse_cents);
        let discounts = read(store, DISCOUNT, from, to, |text| text.parse::<f64>().ok());
        let clients = &mut self.clients;
        let client_codes = read(store, CLIENT, from, to, |text| {
            Some(clients.code(Some(text)))
        });
        let read_done = seconds();

        // Finest group of each row, and its group in the slice.
        let sets = kept_levels();
        let mut touched: Vec<Vec<u32>> = vec![Vec::new(); SETS];
        let mut delta = States::default();
        let mut slice_group = Vec::with_capacity(to - from);
        let keys = (0..to - from)
            .map(|row| -> [u32; LEVELS] { std::array::from_fn(|level| codes[level][row]) });
        for key in keys {
            let finest = match self.finest.get(&finest_key(&key)) {
                Some(&finest) => finest as usize,
                None => self.add_group(key, &sets),
            };
            let group = self.parents[finest][0];
            slice_group.push(self.local_of(group, &mut touched[0], &mut delta) as usize);
        }
        let groups_done = seconds();

        // States of the slice, measure by measure.
        for &group in &slice_group {
            delta.rows[group] += 1;
        }
        for (&group, quantity) in slice_group.iter().zip(&quantities) {
            if let Some(quantity) = *quantity {
                delta.quantity_count[group] += 1;
                delta.quantity_sum[group] += quantity;
            }
        }
        for (&group, amount) in slice_group.iter().zip(&amounts) {
            if let Some(amount) = *amount {
                delta.amount_count[group] += 1;
                delta.amount_sum[group] += i128::from(amount);
                delta.amount_min[group] = delta.amount_min[group].min(amount);
                delta.amount_max[group] = delta.amount_max[group].max(amount);
            }
        }
        for (&group, discount) in slice_group.iter().zip(&discounts) {
            if let Some(discount) = *discount {
                delta.discount_count[group] += 1;
                delta.discount_sum[group] += discount;
            }
        }
        for (&group, date) in slice_group.iter().zip(&dates) {
            if let Some(date) = *date {
                delta.date_min[group] = delta.date_min[group].min(date);
                delta.date_max[group] = delta.date_max[group].max(date);
            }
        }
        for (&group, code) in slice_group.iter().zip(&client_codes) {
            if let Some(code) = *code {
                if text_before(&self.clients, code, delta.client_min[group]) {
                    delta.client_min[group] = code;
                }
                if text_after(&self.clients, code, delta.client_max[group]) {
                    delta.client_max[group] = code;
                }
            }
        }
        let measures_done = seconds();

        // States of the slice in each coarser grouping set, each from its
        // source in the chain of merges, then merge into the states kept.
        for (set, &(source, slot)) in sources().iter().enumerate().skip(1) {
            for position in 0..touched[source].len() {
                let group = touched[source][position];
                let parent = self.up[group as usize][slot];
                let from = self.local[group as usize] as usize;
                let into = self.local_of(parent, &mut touched[set], &mut delta) as usize;
                delta.merge_within(from, into, &self.clients);
            }
        }
        for groups in &touched {
            for &group in groups {
                let local = self.local[group as usize] as usize;
                self.states
                    .merge(&delta, local, group as usize, &self.clients);
                self.local[group as usize] = NO_CODE;
            }
        }
        let merge_done = seconds();
        for (phase, duration) in self.phases.iter_mut().zip([
            read_done - started,
            groups_done - read_done,
            measures_done - groups_done,
            merge_done - measures_done,
        ]) {
            *phase += duration;
        }
    }

    /// Group of the slice of a group, given to it on its first row of the
    /// slice, with a state without rows.
    fn local_of(&mut self, group: u32, touched: &mut Vec<u32>, delta: &mut States) -> u32 {
        let local = &mut self.local[group as usize];
        if *local == NO_CODE {
            *local = delta.len() as u32;
            delta.grow(1);
            touched.push(group);
        }
        *local
    }

    /// Creates the finest group of a key, and finds or creates its group in
    /// every coarser grouping set; gives the number of the finest group.
    fn add_group(&mut self, key: [u32; LEVELS], sets: &[(usize, usize); SETS]) -> usize {
        let finest = self.parents.len();
        let mut parents = [0u32; SETS];
        for (set, &(rows, columns)) in sets.iter().enumerate() {
            let mut kept = [NO_CODE; LEVELS];
            kept[..rows].copy_from_slice(&key[..rows]);
            kept[4..4 + columns].copy_from_slice(&key[4..4 + columns]);
            let next = self.states.len() as u32;
            let mut created = false;
            let group = *self.index[set].entry(kept).or_insert_with(|| {
                created = true;
                next
            });
            if created {
                self.states.grow(1);
                self.up.push([NO_CODE; 2]);
                self.local.push(NO_CODE);
            }
            parents[set] = group;
        }
        for (set, &(source, slot)) in sources().iter().enumerate().skip(1) {
            self.up[parents[source] as usize][slot] = parents[set];
        }
        self.finest.insert(finest_key(&key), finest as u32);
        self.parents.push(parents);
        finest
    }

    /// Number of finest groups.
    pub fn finest_groups(&self) -> usize {
        self.parents.len()
    }

    /// Number of groups, every grouping set included.
    pub fn groups(&self) -> usize {
        self.states.len()
    }

    /// Do two tables hold the same groups, with the same states? The codes of
    /// `client` compare by their text; the float sums, added in another
    /// order, to a relative 10^-9.
    fn same_as(&self, other: &Incremental) -> bool {
        let (mine, theirs) = (&self.states, &other.states);
        let text = |table: &Incremental, code: u32| {
            (code != NO_CODE).then(|| table.clients.text(code).map(str::to_owned))
        };
        let close = |a: f64, b: f64| (a - b).abs() <= 1e-9 * a.abs().max(b.abs()).max(1.0);
        mine.len() == theirs.len()
            && self.parents == other.parents
            && mine.rows == theirs.rows
            && mine.quantity_count == theirs.quantity_count
            && mine.quantity_sum == theirs.quantity_sum
            && mine.amount_count == theirs.amount_count
            && mine.amount_sum == theirs.amount_sum
            && mine.discount_count == theirs.discount_count
            && mine
                .discount_sum
                .iter()
                .zip(&theirs.discount_sum)
                .all(|(&a, &b)| close(a, b))
            && mine.amount_min == theirs.amount_min
            && mine.amount_max == theirs.amount_max
            && mine.date_min == theirs.date_min
            && mine.date_max == theirs.date_max
            && (0..mine.len()).all(|group| {
                text(self, mine.client_min[group]) == text(other, theirs.client_min[group])
                    && text(self, mine.client_max[group]) == text(other, theirs.client_max[group])
            })
    }
}

/// Full computation by the engine of Prisme at one checkpoint of the
/// reception, for one profile.
struct Full {
    /// Rows received.
    rows: usize,
    /// First table of a new engine: the caches of the columns, then the
    /// table, in seconds.
    first_s: f64,
    /// Next table of the same engine, its caches built, in seconds.
    table_s: f64,
    /// Finest groups of the table.
    finest_groups: usize,
}

/// Update of the prototype by one slice, at one checkpoint.
struct Update {
    /// Rows received, the slice included.
    rows: usize,
    /// Duration of the update by this slice, in seconds.
    update_s: f64,
    /// Duration of one pass of the prototype on all the rows received, in
    /// seconds: its full computation; `None` when not measured.
    one_pass_s: Option<f64>,
    /// Groups of every grouping set.
    groups: usize,
}

/// Results of one profile.
#[derive(Default)]
struct ProfileResults {
    /// Full computations, at the checkpoints.
    full: Vec<Full>,
    /// Updates of the prototype, at the checkpoints.
    updates: Vec<Update>,
    /// Sum of the updates of the whole reception, in seconds.
    updates_s: f64,
    /// Longest update, in seconds.
    longest_update_s: f64,
    /// Did the prototype check itself (see the module comment)?
    checked: bool,
    /// Finest groups found by the prototype at the end.
    finest_groups: usize,
    /// Durations of the phases of all the updates.
    phases: Phases,
}

/// Checkpoints of a reception of `total` rows: a tenth, a half, and the end.
fn checkpoints(total: usize, slice: usize) -> Vec<usize> {
    let mut points: Vec<usize> = [total / 10, total / 2, total]
        .into_iter()
        .map(|point| point.div_ceil(slice) * slice)
        .map(|point| point.min(total).max(slice.min(total)))
        .collect();
    points.dedup();
    points
}

/// Computes the table of each profile from the rows received so far by a new
/// engine, twice: its first table builds the caches of its columns, the
/// second one keeps them.
fn full_tables(store: Store, received: usize, results: &mut [ProfileResults]) {
    let engine = Engine::new(store, families());
    for (profile, &(_, rows)) in results.iter_mut().zip(PROFILES.iter()) {
        let request = request(rows);
        let started = seconds();
        engine.compute(&request).expect("the table of the profile");
        let first_s = seconds() - started;
        let started = seconds();
        let table = engine.compute(&request).expect("the table of the profile");
        let table_s = seconds() - started;
        profile.full.push(Full {
            rows: received,
            first_s,
            table_s,
            finest_groups: table.finest_groups(),
        });
    }
}

/// Runs the benchmark on `total` rows received by slices of `slice` rows, and
/// gives its results as a JSON object.
///
/// Without `one_pass`, the prototype and the streamed engine make no pass on
/// all the rows: their check is then only the number of finest groups, and
/// the memory holds one table less per profile (a WebAssembly module holds
/// 4 GB at most).
pub fn run(total: usize, slice: usize, one_pass: bool) -> String {
    let points = checkpoints(total, slice);
    let mut store = Store::new(COLUMNS);
    let mut generator = Generator::new();
    let mut tables: Vec<Incremental> = PROFILES
        .iter()
        .map(|&(_, rows)| Incremental::new(rows))
        .collect();
    let mut results: Vec<ProfileResults> =
        PROFILES.iter().map(|_| ProfileResults::default()).collect();
    let mut appending_s = 0.0;
    let mut longest_append_s: f64 = 0.0;
    let mut received = 0;
    while received < total {
        let count = slice.min(total - received);
        let rows = generator.rows(count);
        let started = seconds();
        append(&mut store, &rows);
        let elapsed = seconds() - started;
        appending_s += elapsed;
        longest_append_s = longest_append_s.max(elapsed);
        drop(rows);
        let from = received;
        received += count;
        let checkpoint = points.contains(&received);
        for (table, profile) in tables.iter_mut().zip(&mut results) {
            let started = seconds();
            table.update(&store, from, received);
            let elapsed = seconds() - started;
            profile.updates_s += elapsed;
            profile.longest_update_s = profile.longest_update_s.max(elapsed);
            if checkpoint {
                let mut one_pass_s = None;
                if one_pass {
                    let started = seconds();
                    let mut one_pass = Incremental::new(table.row_columns);
                    one_pass.update(&store, 0, received);
                    one_pass_s = Some(seconds() - started);
                    if received == total {
                        profile.checked = one_pass.same_as(table);
                    }
                } else {
                    profile.checked = true;
                }
                profile.updates.push(Update {
                    rows: received,
                    update_s: elapsed,
                    one_pass_s,
                    groups: table.groups(),
                });
            }
        }
        // The engine takes the same rows generated again; at the end, the
        // store of the reception, once the prototype has left the memory.
        if checkpoint && received < total {
            full_tables(store_of(received), received, &mut results);
        }
    }
    for (table, profile) in tables.iter().zip(&mut results) {
        profile.finest_groups = table.finest_groups();
        profile.phases = table.phases;
    }
    drop(tables);
    full_tables(store, total, &mut results);
    for profile in &mut results {
        let engine_groups = profile.full.last().map(|full| full.finest_groups);
        profile.checked &= engine_groups == Some(profile.finest_groups);
    }
    let finest_groups: Vec<usize> = results
        .iter()
        .map(|profile| profile.finest_groups)
        .collect();
    let streamed = engine::stream(total, slice, one_pass, &finest_groups);
    let mut out = json(total, slice, appending_s, longest_append_s, &results);
    out.pop();
    out.push_str(",\"engine\":[");
    for (index, profile) in streamed.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        let _ = write!(
            out,
            "{{\"name\":\"{}\",\"checked\":{},\"grows_s\":{:.4},\"updates_s\":{:.4},\"longest_update_s\":{:.4},\"full_s\":{},\"updates\":[",
            profile.name,
            profile.checked,
            profile.grows_s,
            profile.updates_s,
            profile.longest_update_s,
            profile
                .full_s
                .map_or("null".to_owned(), |full_s| format!("{full_s:.4}"))
        );
        for (position, checkpoint) in profile.checkpoints.iter().enumerate() {
            if position > 0 {
                out.push(',');
            }
            let _ = write!(
                out,
                "{{\"rows\":{},\"grow_s\":{:.4},\"update_s\":{:.4},\"finest_groups\":{}}}",
                checkpoint.rows, checkpoint.grow_s, checkpoint.update_s, checkpoint.finest_groups
            );
        }
        out.push_str("]}");
    }
    out.push_str("]}");
    out
}

/// Results as a JSON object.
fn json(
    total: usize,
    slice: usize,
    appending_s: f64,
    longest_append_s: f64,
    results: &[ProfileResults],
) -> String {
    let mut out = String::new();
    let _ = write!(
        out,
        "{{\"rows\":{total},\"slice\":{slice},\"append_s\":{appending_s:.4},\"longest_append_s\":{longest_append_s:.4},\"profiles\":["
    );
    for (index, ((name, _), profile)) in PROFILES.iter().zip(results).enumerate() {
        if index > 0 {
            out.push(',');
        }
        let _ = write!(
            out,
            "{{\"name\":\"{name}\",\"checked\":{},\"finest_groups\":{},\"updates_s\":{:.4},\"longest_update_s\":{:.4},\"phases_s\":[{:.4},{:.4},{:.4},{:.4}],\"full\":[",
            profile.checked,
            profile.finest_groups,
            profile.updates_s,
            profile.longest_update_s,
            profile.phases[0],
            profile.phases[1],
            profile.phases[2],
            profile.phases[3]
        );
        for (position, full) in profile.full.iter().enumerate() {
            if position > 0 {
                out.push(',');
            }
            let _ = write!(
                out,
                "{{\"rows\":{},\"first_s\":{:.4},\"table_s\":{:.4},\"finest_groups\":{}}}",
                full.rows, full.first_s, full.table_s, full.finest_groups
            );
        }
        out.push_str("],\"updates\":[");
        for (position, update) in profile.updates.iter().enumerate() {
            if position > 0 {
                out.push(',');
            }
            let _ = write!(
                out,
                "{{\"rows\":{},\"update_s\":{:.4},\"one_pass_s\":{},\"groups\":{}}}",
                update.rows,
                update.update_s,
                update
                    .one_pass_s
                    .map_or("null".to_owned(), |seconds| format!("{seconds:.4}")),
                update.groups
            );
        }
        out.push_str("]}");
    }
    out.push_str("]}");
    out
}

/// Report of the last run, read by JavaScript in the memory of the module.
#[cfg(target_arch = "wasm32")]
static mut REPORT: String = String::new();

/// Runs the benchmark (see [`run`]) in WebAssembly, and gives the length of
/// its report, whose address `report_address` gives.
#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn bench(total: u32, slice: u32, one_pass: u32) -> u32 {
    let report = run(total as usize, slice as usize, one_pass != 0);
    let length = report.len() as u32;
    // SAFETY: the module runs on one thread, and JavaScript reads the report
    // before calling it again.
    unsafe { REPORT = report };
    length
}

/// Address of the report of the last run in the memory of the module.
#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn report_address() -> u32 {
    // SAFETY: see `bench`.
    unsafe {
        (&raw const REPORT)
            .as_ref()
            .map_or(0, |report| report.as_ptr() as u32)
    }
}
