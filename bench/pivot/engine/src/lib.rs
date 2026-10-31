//! Engine of the benchmark of the Pivoteur (point A1 of its specification),
//! shared by the native side (`../native`, run in the server) and the
//! WebAssembly side (`../wasm`, run in the page).
//!
//! Reads the text cells of a result into typed columns, once, then computes
//! the pivot: 4 row levels, 2 column levels (`YEAR(d)`, `MONTH(d)`),
//! 12 measures, every subtotal and total. The code has the shape of a generic
//! engine: each level and each measure is an expression evaluated over whole
//! columns, the groups are found through a hash table, and the totals merge
//! the states of the finer groups. The JavaScript side (`../js/bench.mjs`)
//! has the same shape, phase by phase, and must give the same checksums.
//!
//! Columns of the generated result: `pays`, `region`, `categorie`, `produit`,
//! `canal`, `d` (datetime), `qte` (integer), `montant` (decimal with
//! 2 digits), `remise` (float), `client`.

use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::hash::{BuildHasherDefault, Hasher};
use std::thread;

/// Number of columns of the generated result.
pub const COLUMNS: usize = 10;

/// Profiles of the benchmark: name, and columns of the row levels, the
/// outermost first.
pub const PROFILES: [(&str, [usize; 4]); 2] = [
    ("high", [0, 1, 2, 3]), // pays, region, categorie, produit
    ("low", [0, 1, 2, 4]),  // pays, region, categorie, canal
];

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

/// Hasher of rustc (FxHash): the table of the groups needs a fast hash; the
/// default one (SipHash) resists collision attacks, which the pivot does not
/// need.
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

    /// The rotation brings the bits mixed best, the high ones, down to where
    /// the table picks its bucket: without it, texts that differ only by
    /// their last characters (`C000123`, `C000124`) collide (fix of
    /// `rustc-hash` 2).
    fn finish(&self) -> u64 {
        self.hash.rotate_left(26)
    }
}

/// Hash table with the fast hasher.
type FxMap<K, V> = HashMap<K, V, BuildHasherDefault<FxHasher>>;

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

/// Date of the civil calendar of a number of days since 1970-01-01, as
/// (year, month, day).
pub fn civil_from_days(days: i64) -> (i64, i64, i64) {
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
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// `YEAR()` of a number of days.
fn year_of(days: i64) -> i64 {
    civil_from_days(days).0
}

/// `MONTH()` of a number of days.
fn month_of(days: i64) -> i64 {
    civil_from_days(days).1
}

/// Text cells of one column of a result, whatever their layout in memory.
pub trait Column {
    /// Number of rows.
    fn rows(&self) -> usize;

    /// Cells, in the order of the rows: text, or `None` for NULL.
    fn cells(&self) -> impl Iterator<Item = Option<&str>>;
}

/// Text column encoded as a dictionary: one code per row, one value per code.
pub struct Dictionary {
    /// Code of each row.
    pub codes: Vec<u32>,
    /// Value of each code, in the order of their first appearance; NULL has
    /// its own code.
    pub values: Vec<String>,
}

/// Column of exact numbers (integers, decimals scaled to integers, seconds),
/// with the validity of each row (0 for NULL or an invalid value).
pub struct Exact {
    /// Value of each row.
    pub values: Vec<i64>,
    /// Validity of each row.
    pub valid: Vec<u8>,
}

/// Column of floats, with the validity of each row.
pub struct Approx {
    /// Value of each row.
    pub values: Vec<f64>,
    /// Validity of each row.
    pub valid: Vec<u8>,
}

/// Encodes a text column as a dictionary.
fn dictionary(column: &impl Column) -> Dictionary {
    let mut index: FxMap<&str, u32> = FxMap::default();
    let mut values: Vec<String> = Vec::new();
    let mut null_code = None;
    let mut codes = Vec::with_capacity(column.rows());
    for cell in column.cells() {
        let code = match cell {
            Some(text) => *index.entry(text).or_insert_with(|| {
                values.push(text.to_owned());
                (values.len() - 1) as u32
            }),
            None => *null_code.get_or_insert_with(|| {
                values.push(String::new());
                (values.len() - 1) as u32
            }),
        };
        codes.push(code);
    }
    Dictionary { codes, values }
}

/// Reads a column of exact numbers; an invalid value becomes NULL.
fn exact(column: &impl Column, parse: impl Fn(&[u8]) -> Option<i64>) -> Exact {
    let mut values = Vec::with_capacity(column.rows());
    let mut valid = Vec::with_capacity(column.rows());
    for cell in column.cells() {
        match cell.and_then(|text| parse(text.as_bytes())) {
            Some(value) => {
                values.push(value);
                valid.push(1);
            }
            None => {
                values.push(0);
                valid.push(0);
            }
        }
    }
    Exact { values, valid }
}

/// Reads a column of floats; an invalid value becomes NULL.
fn approx(column: &impl Column) -> Approx {
    let mut values = Vec::with_capacity(column.rows());
    let mut valid = Vec::with_capacity(column.rows());
    for cell in column.cells() {
        match cell.and_then(|text| text.parse::<f64>().ok()) {
            Some(value) => {
                values.push(value);
                valid.push(1);
            }
            None => {
                values.push(0.0);
                valid.push(0);
            }
        }
    }
    Approx { values, valid }
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
fn parse_integer(text: &[u8]) -> Option<i64> {
    let (sign, digits) = match text.first() {
        Some(b'-') => (-1, &text[1..]),
        _ => (1, text),
    };
    if digits.is_empty() {
        return None;
    }
    digits_value(digits).map(|value| sign * value)
}

/// Reads a decimal as an integer scaled by 10^`scale` (`12.5` with a scale
/// of 2 gives 1250); extra digits are rounded half away from zero.
fn parse_decimal(text: &[u8], scale: usize) -> Option<i64> {
    let (sign, unsigned) = match text.first() {
        Some(b'-') => (-1, &text[1..]),
        _ => (1, text),
    };
    let (whole, fraction) = match unsigned.iter().position(|&byte| byte == b'.') {
        Some(point) => (&unsigned[..point], &unsigned[point + 1..]),
        None => (unsigned, &unsigned[..0]),
    };
    if whole.is_empty() && fraction.is_empty() {
        return None;
    }
    let mut value = digits_value(whole)?;
    for position in 0..scale {
        let digit = match fraction.get(position) {
            Some(&byte) if byte.is_ascii_digit() => i64::from(byte - b'0'),
            Some(_) => return None,
            None => 0,
        };
        value = value * 10 + digit;
    }
    if fraction.len() > scale {
        let rest = digits_value(&fraction[scale..])?;
        if fraction[scale] >= b'5' && rest > 0 {
            value += 1;
        }
    }
    Some(sign * value)
}

/// Reads a `YYYY-MM-DD` date or a `YYYY-MM-DD HH:MM:SS` datetime, as seconds
/// since 1970-01-01.
fn parse_datetime(text: &[u8]) -> Option<i64> {
    if text.len() != 10 && text.len() != 19 {
        return None;
    }
    let year = digits_value(&text[0..4])?;
    let month = digits_value(&text[5..7])?;
    let day = digits_value(&text[8..10])?;
    let mut seconds = days_from_civil(year, month, day) * 86_400;
    if text.len() == 19 {
        seconds += digits_value(&text[11..13])? * 3_600
            + digits_value(&text[14..16])? * 60
            + digits_value(&text[17..19])?;
    }
    Some(seconds)
}

/// Rank of each value of a dictionary in the binary order of the texts.
fn ranks(values: &[String]) -> Vec<u32> {
    let mut order: Vec<u32> = (0..values.len() as u32).collect();
    order.sort_unstable_by(|&a, &b| {
        values[a as usize]
            .as_bytes()
            .cmp(values[b as usize].as_bytes())
    });
    let mut rank = vec![0u32; values.len()];
    for (position, &code) in order.iter().enumerate() {
        rank[code as usize] = position as u32;
    }
    rank
}

/// Typed column, read from the text cells of a column of the result.
pub enum Typed {
    /// Text, encoded as a dictionary.
    Dictionary(Dictionary),
    /// Exact numbers.
    Exact(Exact),
    /// Floats.
    Approx(Approx),
}

impl Typed {
    /// Dictionary of a text column.
    fn into_dictionary(self) -> Dictionary {
        match self {
            Typed::Dictionary(dictionary) => dictionary,
            _ => panic!("text column expected"),
        }
    }

    /// Values of a column of exact numbers.
    fn into_exact(self) -> Exact {
        match self {
            Typed::Exact(column) => column,
            _ => panic!("column of exact numbers expected"),
        }
    }

    /// Values of a column of floats.
    fn into_approx(self) -> Approx {
        match self {
            Typed::Approx(column) => column,
            _ => panic!("column of floats expected"),
        }
    }
}

/// Reads column `index` of the generated result as its type requires.
pub fn read_column(index: usize, column: &impl Column) -> Typed {
    match index {
        5 => Typed::Exact(exact(column, parse_datetime)),
        6 => Typed::Exact(exact(column, parse_integer)),
        7 => Typed::Exact(exact(column, |text| parse_decimal(text, 2))),
        8 => Typed::Approx(approx(column)),
        _ => Typed::Dictionary(dictionary(column)),
    }
}

/// Typed columns of the result, read once from the text cells.
pub struct Prepared {
    /// `pays`, `region`, `categorie`, `produit`, `canal`.
    pub levels: Vec<Dictionary>,
    /// `client`.
    pub client: Dictionary,
    /// `d`, in seconds.
    pub date: Exact,
    /// `qte`.
    pub quantity: Exact,
    /// `montant`, in hundredths.
    pub amount: Exact,
    /// `remise`.
    pub discount: Approx,
    /// Rank of the `client` of each row, in the binary order of the texts.
    pub client_rank: Exact,
}

impl Prepared {
    /// Assembles the typed columns of the result, given in the order of its
    /// columns, and ranks the values of `client`.
    pub fn new(columns: Vec<Typed>) -> Self {
        assert_eq!(columns.len(), COLUMNS, "one typed column per column");
        let mut columns = columns.into_iter();
        let mut next = move || columns.next().expect("one typed column per column");
        let levels = (0..5).map(|_| next().into_dictionary()).collect();
        let date = next().into_exact();
        let quantity = next().into_exact();
        let amount = next().into_exact();
        let discount = next().into_approx();
        let client = next().into_dictionary();
        let rank = ranks(&client.values);
        let client_rank = Exact {
            values: client
                .codes
                .iter()
                .map(|&code| i64::from(rank[code as usize]))
                .collect(),
            valid: vec![1; client.codes.len()],
        };
        Prepared {
            levels,
            client,
            date,
            quantity,
            amount,
            discount,
            client_rank,
        }
    }

    /// Values of `client` in the binary order of the texts: the text of each
    /// rank.
    pub fn ranked(&self) -> Vec<&str> {
        let rank = ranks(&self.client.values);
        let mut ranked = vec![""; rank.len()];
        for (code, &position) in rank.iter().enumerate() {
            ranked[position as usize] = &self.client.values[code];
        }
        ranked
    }

    /// Measures of the benchmark, with the type of their values.
    pub fn labels(&self) -> Vec<(Measure<'_>, Format)> {
        vec![
            (Measure::CountStar, Format::Integer),
            (Measure::Count(&self.quantity), Format::Integer),
            (Measure::Sum(&self.quantity), Format::Integer),
            (Measure::Sum(&self.amount), Format::Decimal(2)),
            (Measure::Avg(&self.amount), Format::Decimal(2)),
            (Measure::AvgFloat(&self.discount), Format::Float),
            (Measure::Min(&self.amount), Format::Decimal(2)),
            (Measure::Max(&self.amount), Format::Decimal(2)),
            (Measure::Min(&self.date), Format::Datetime),
            (Measure::Max(&self.date), Format::Datetime),
            (Measure::Min(&self.client_rank), Format::Text),
            (Measure::Max(&self.client_rank), Format::Text),
        ]
    }

    /// Levels of the rows of a profile.
    pub fn row_levels(&self, columns: &[usize]) -> Vec<Level<'_>> {
        columns
            .iter()
            .map(|&column| Level::Column(&self.levels[column]))
            .collect()
    }

    /// Levels of the columns: `YEAR(d)`, `MONTH(d)`.
    pub fn column_levels(&self) -> [Level<'_>; 2] {
        [Level::Year(&self.date), Level::Month(&self.date)]
    }
}

/// Expression of a level: a text column, or a part of a date.
pub enum Level<'a> {
    /// Text column, already encoded.
    Column(&'a Dictionary),
    /// `YEAR(d)`.
    Year(&'a Exact),
    /// `MONTH(d)`.
    Month(&'a Exact),
}

/// Codes of a level, one per row, and its number of distinct codes.
fn evaluate<'a>(level: &Level<'a>) -> (Cow<'a, [u32]>, u64) {
    let (column, part): (&Exact, fn(i64) -> i64) = match *level {
        Level::Column(dictionary) => {
            return (
                Cow::Borrowed(&dictionary.codes),
                dictionary.values.len() as u64,
            );
        }
        Level::Year(column) => (column, year_of),
        Level::Month(column) => (column, month_of),
    };
    let mut index: FxMap<i64, u32> = FxMap::default();
    let mut count = 0u32;
    let mut null_code = None;
    let mut codes = Vec::with_capacity(column.values.len());
    for (row, &seconds) in column.values.iter().enumerate() {
        let code = if column.valid[row] != 0 {
            *index
                .entry(part(seconds.div_euclid(86_400)))
                .or_insert_with(|| {
                    count += 1;
                    count - 1
                })
        } else {
            *null_code.get_or_insert_with(|| {
                count += 1;
                count - 1
            })
        };
        codes.push(code);
    }
    (Cow::Owned(codes), u64::from(count))
}

/// Expression of a measure.
#[derive(Clone, Copy)]
pub enum Measure<'a> {
    /// `COUNT(*)`.
    CountStar,
    /// `COUNT(x)`.
    Count(&'a Exact),
    /// `SUM(x)`, exact.
    Sum(&'a Exact),
    /// `AVG(x)`, exact.
    Avg(&'a Exact),
    /// `AVG(x)` of a float.
    AvgFloat(&'a Approx),
    /// `MIN(x)`.
    Min(&'a Exact),
    /// `MAX(x)`.
    Max(&'a Exact),
}

/// States of a measure, one per group.
pub enum State {
    /// Count.
    Count(Vec<u64>),
    /// Exact sum and count of the values.
    Sum(Vec<i64>, Vec<u64>),
    /// Float sum and count of the values.
    FloatSum(Vec<f64>, Vec<u64>),
    /// Minimum, `i64::MAX` without value.
    Min(Vec<i64>),
    /// Maximum, `i64::MIN` without value.
    Max(Vec<i64>),
}

/// Grouping set computed from a finer one: each of its groups merges the
/// states of the groups of its source.
struct Grouping {
    /// Index of the first group of the source.
    source: usize,
    /// Group of this set of each group of the source, in order.
    parent_of: Vec<u32>,
}

/// Computed pivot.
pub struct Pivot {
    /// Number of groups, all grouping sets included.
    pub groups: usize,
    /// First group and number of groups of each grouping set, by number of
    /// row levels then of column levels kept.
    pub sets: Vec<Vec<(usize, usize)>>,
    /// States of each measure.
    pub states: Vec<State>,
    /// Durations in seconds: levels, groups, grouping sets, measures.
    pub phases: [f64; 4],
}

/// Merges the states of each grouping set into those of the coarser one, by
/// addition.
fn merge_add<T: Copy + std::ops::AddAssign>(values: &mut [T], groupings: &[Grouping]) {
    for grouping in groupings {
        for (offset, &parent) in grouping.parent_of.iter().enumerate() {
            let value = values[grouping.source + offset];
            values[parent as usize] += value;
        }
    }
}

/// Merges the states of each grouping set into those of the coarser one,
/// keeping the smallest (`keep_less`) or the largest value.
fn merge_extreme(values: &mut [i64], groupings: &[Grouping], keep_less: bool) {
    for grouping in groupings {
        for (offset, &parent) in grouping.parent_of.iter().enumerate() {
            let value = values[grouping.source + offset];
            let parent = &mut values[parent as usize];
            let better = if keep_less {
                value < *parent
            } else {
                value > *parent
            };
            if better {
                *parent = value;
            }
        }
    }
}

/// Computes the states of a measure: rows into the finest groups, then the
/// merge into the subtotals and totals.
fn accumulate(measure: &Measure, group_of: &[u32], groups: usize, groupings: &[Grouping]) -> State {
    match *measure {
        Measure::CountStar => {
            let mut counts = vec![0u64; groups];
            for &group in group_of {
                counts[group as usize] += 1;
            }
            merge_add(&mut counts, groupings);
            State::Count(counts)
        }
        Measure::Count(column) => {
            let mut counts = vec![0u64; groups];
            for (row, &group) in group_of.iter().enumerate() {
                if column.valid[row] != 0 {
                    counts[group as usize] += 1;
                }
            }
            merge_add(&mut counts, groupings);
            State::Count(counts)
        }
        Measure::Sum(column) | Measure::Avg(column) => {
            let mut sums = vec![0i64; groups];
            let mut counts = vec![0u64; groups];
            for (row, &group) in group_of.iter().enumerate() {
                if column.valid[row] != 0 {
                    sums[group as usize] += column.values[row];
                    counts[group as usize] += 1;
                }
            }
            merge_add(&mut sums, groupings);
            merge_add(&mut counts, groupings);
            State::Sum(sums, counts)
        }
        Measure::AvgFloat(column) => {
            let mut sums = vec![0f64; groups];
            let mut counts = vec![0u64; groups];
            for (row, &group) in group_of.iter().enumerate() {
                if column.valid[row] != 0 {
                    sums[group as usize] += column.values[row];
                    counts[group as usize] += 1;
                }
            }
            merge_add(&mut sums, groupings);
            merge_add(&mut counts, groupings);
            State::FloatSum(sums, counts)
        }
        Measure::Min(column) => {
            let mut minimums = vec![i64::MAX; groups];
            for (row, &group) in group_of.iter().enumerate() {
                let value = column.values[row];
                if column.valid[row] != 0 && value < minimums[group as usize] {
                    minimums[group as usize] = value;
                }
            }
            merge_extreme(&mut minimums, groupings, true);
            State::Min(minimums)
        }
        Measure::Max(column) => {
            let mut maximums = vec![i64::MIN; groups];
            for (row, &group) in group_of.iter().enumerate() {
                let value = column.values[row];
                if column.valid[row] != 0 && value > maximums[group as usize] {
                    maximums[group as usize] = value;
                }
            }
            merge_extreme(&mut maximums, groupings, false);
            State::Max(maximums)
        }
    }
}

/// Product of numbers of distinct codes.
fn product(cards: &[u64]) -> u64 {
    cards.iter().product()
}

/// Computes the pivot, every subtotal and total included, with the levels and
/// the measures spread over threads if `parallel`.
///
/// Phases: the levels give a code per row; the codes of a row, read in a
/// mixed radix, give the key of its finest group, numbered through a hash
/// table; each grouping set is built from a finer one (rows, then columns);
/// each measure fills its states, then merges them set by set.
pub fn compute(
    row_levels: &[Level],
    column_levels: &[Level],
    measures: &[Measure],
    parallel: bool,
) -> Pivot {
    let started = seconds();
    let levels: Vec<&Level> = row_levels.iter().chain(column_levels).collect();
    let evaluated: Vec<(Cow<[u32]>, u64)> = if parallel {
        thread::scope(|scope| {
            let handles: Vec<_> = levels
                .iter()
                .map(|&level| scope.spawn(move || evaluate(level)))
                .collect();
            handles
                .into_iter()
                .map(|handle| handle.join().unwrap())
                .collect()
        })
    } else {
        levels.iter().map(|&level| evaluate(level)).collect()
    };
    let cards: Vec<u64> = evaluated.iter().map(|(_, card)| *card).collect();
    let codes: Vec<&[u32]> = evaluated.iter().map(|(codes, _)| codes.as_ref()).collect();
    let levels_done = seconds() - started;

    let started = seconds();
    let rows = codes[0].len();
    let mut index: FxMap<u64, u32> = FxMap::default();
    let mut keys: Vec<u64> = Vec::new();
    let mut group_of = Vec::with_capacity(rows);
    for row in 0..rows {
        let mut key = 0u64;
        for (level, level_codes) in codes.iter().enumerate() {
            key = key * cards[level] + u64::from(level_codes[row]);
        }
        let next = keys.len() as u32;
        let group = *index.entry(key).or_insert_with(|| {
            keys.push(key);
            next
        });
        group_of.push(group);
    }
    drop(index);
    let groups_done = seconds() - started;

    let started = seconds();
    let (row_count, column_count) = (row_levels.len(), column_levels.len());
    let (row_cards, column_cards) = cards.split_at(row_count);
    let column_product = product(column_cards);
    let mut sets = vec![vec![(0usize, 0usize); column_count + 1]; row_count + 1];
    sets[row_count][column_count] = (0, keys.len());
    let mut groupings = Vec::new();
    for kept_rows in (0..=row_count).rev() {
        for kept_columns in (0..=column_count).rev() {
            if kept_rows == row_count && kept_columns == column_count {
                continue;
            }
            let (source_rows, source_columns) = if kept_columns < column_count {
                (kept_rows, kept_columns + 1)
            } else {
                (kept_rows + 1, kept_columns)
            };
            let (source, source_len) = sets[source_rows][source_columns];
            let row_divisor = product(&row_cards[kept_rows..]);
            let column_divisor = product(&column_cards[kept_columns..]);
            let column_prefix = product(&column_cards[..kept_columns]);
            let start = keys.len();
            let mut index: FxMap<u64, u32> = FxMap::default();
            let mut parent_of = Vec::with_capacity(source_len);
            for group in source..source + source_len {
                let key = keys[group];
                let parent_key = key / column_product / row_divisor * column_prefix
                    + key % column_product / column_divisor;
                let next = keys.len() as u32;
                let parent = *index.entry(parent_key).or_insert_with(|| {
                    keys.push(key);
                    next
                });
                parent_of.push(parent);
            }
            sets[kept_rows][kept_columns] = (start, keys.len() - start);
            groupings.push(Grouping { source, parent_of });
        }
    }
    let groups = keys.len();
    let sets_done = seconds() - started;

    let started = seconds();
    let states: Vec<State> = if parallel {
        let (group_of, groupings) = (&group_of, &groupings);
        thread::scope(|scope| {
            let handles: Vec<_> = measures
                .iter()
                .map(|measure| {
                    scope.spawn(move || accumulate(measure, group_of, groups, groupings))
                })
                .collect();
            handles
                .into_iter()
                .map(|handle| handle.join().unwrap())
                .collect()
        })
    } else {
        measures
            .iter()
            .map(|measure| accumulate(measure, &group_of, groups, &groupings))
            .collect()
    };
    let measures_done = seconds() - started;

    Pivot {
        groups,
        sets,
        states,
        phases: [levels_done, groups_done, sets_done, measures_done],
    }
}

/// Checksum of the states: a 32-bit FNV-1a hash of the exact values, in the
/// order of the measures then of the groups, and the bits of the sum of the
/// float sums.
pub fn checksum(states: &[State]) -> (u32, u64) {
    let mut hash: u32 = 2_166_136_261;
    let mut float_total = 0f64;
    let mut mix = |value: i64| {
        hash = (hash ^ value as u32).wrapping_mul(16_777_619);
        hash = (hash ^ (value >> 32) as u32).wrapping_mul(16_777_619);
    };
    for state in states {
        match state {
            State::Count(counts) => counts.iter().for_each(|&count| mix(count as i64)),
            State::Sum(sums, counts) => {
                sums.iter().for_each(|&sum| mix(sum));
                counts.iter().for_each(|&count| mix(count as i64));
            }
            State::FloatSum(sums, counts) => {
                sums.iter().for_each(|&sum| float_total += sum);
                counts.iter().for_each(|&count| mix(count as i64));
            }
            State::Min(values) => values
                .iter()
                .for_each(|&value| mix(if value == i64::MAX { 0 } else { value })),
            State::Max(values) => values
                .iter()
                .for_each(|&value| mix(if value == i64::MIN { 0 } else { value })),
        }
    }
    (hash, float_total.to_bits())
}

/// Type of the values of a measure, for their text.
#[derive(Clone, Copy)]
pub enum Format {
    /// Integer.
    Integer,
    /// Decimal scaled to an integer, with its number of digits after the point.
    Decimal(u32),
    /// Float.
    Float,
    /// Datetime, in seconds since 1970-01-01.
    Datetime,
    /// Text, by its rank in the binary order.
    Text,
}

/// Writes a decimal scaled to an integer.
fn write_decimal(json: &mut String, value: i128, scale: u32) {
    let divisor = 10i128.pow(scale);
    let sign = if value < 0 { "-" } else { "" };
    let value = value.abs();
    let _ = write!(
        json,
        "{sign}{}.{:0width$}",
        value / divisor,
        value % divisor,
        width = scale as usize
    );
}

/// Writes an exact value as JSON, as MySQL gives it: a decimal as a string
/// with its digits, a datetime as `YYYY-MM-DD HH:MM:SS`, a text by its rank
/// in `ranked`.
fn write_exact(json: &mut String, value: i64, format: Format, ranked: &[&str]) {
    match format {
        Format::Integer | Format::Float => {
            let _ = write!(json, "{value}");
        }
        Format::Decimal(scale) => {
            json.push('"');
            write_decimal(json, i128::from(value), scale);
            json.push('"');
        }
        Format::Datetime => {
            let (year, month, day) = civil_from_days(value.div_euclid(86_400));
            let time = value.rem_euclid(86_400);
            let _ = write!(
                json,
                "\"{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02}\"",
                time / 3_600,
                time / 60 % 60,
                time % 60
            );
        }
        Format::Text => {
            json.push('"');
            json.push_str(ranked[value as usize]);
            json.push('"');
        }
    }
}

/// Writes the values of the measures of `group` as JSON, separated by
/// commas, in the order of the measures; the average of a decimal keeps the
/// scale of its column + 4, as in MySQL, rounded half away from zero.
fn write_group(
    json: &mut String,
    pivot: &Pivot,
    group: usize,
    labels: &[(Measure, Format)],
    ranked: &[&str],
) {
    for (index, (state, &(measure, format))) in pivot.states.iter().zip(labels).enumerate() {
        if index > 0 {
            json.push(',');
        }
        match state {
            State::Count(counts) => {
                let _ = write!(json, "{}", counts[group]);
            }
            State::Sum(_, counts) | State::FloatSum(_, counts) if counts[group] == 0 => {
                json.push_str("null")
            }
            State::Sum(sums, counts) => match (measure, format) {
                (Measure::Avg(_), Format::Decimal(scale)) => {
                    let scaled = i128::from(sums[group]) * 10_000;
                    let count = i128::from(counts[group]);
                    json.push('"');
                    write_decimal(json, (2 * scaled + count) / (2 * count), scale + 4);
                    json.push('"');
                }
                _ => write_exact(json, sums[group], format, ranked),
            },
            State::FloatSum(sums, counts) => {
                let _ = write!(json, "{}", sums[group] / counts[group] as f64);
            }
            State::Min(values) | State::Max(values) => match values[group] {
                i64::MAX | i64::MIN => json.push_str("null"),
                value => write_exact(json, value, format, ranked),
            },
        }
    }
}

/// Serializes every group of the pivot as JSON, as the server would send it
/// to the page: `[set, group, values...]` per group.
pub fn serialize(pivot: &Pivot, labels: &[(Measure, Format)], ranked: &[&str]) -> String {
    let mut json = String::from("[");
    for (kept_rows, row) in pivot.sets.iter().enumerate() {
        for (kept_columns, &(start, len)) in row.iter().enumerate() {
            for group in start..start + len {
                if json.len() > 1 {
                    json.push(',');
                }
                let _ = write!(json, "[{},{group},", kept_rows * row.len() + kept_columns);
                write_group(&mut json, pivot, group, labels, ranked);
                json.push(']');
            }
        }
    }
    json.push(']');
    json
}

/// JSON fields of a computed pivot that every side of the benchmark gives, to
/// compare them: number of groups, sizes of the grouping sets, checksums,
/// grand total.
pub fn pivot_json(pivot: &Pivot, labels: &[(Measure, Format)], ranked: &[&str]) -> String {
    let (hash, float_bits) = checksum(&pivot.states);
    let set_sizes: Vec<String> = pivot
        .sets
        .iter()
        .flatten()
        .map(|&(_, len)| len.to_string())
        .collect();
    let mut grand_total = String::new();
    write_group(&mut grand_total, pivot, pivot.sets[0][0].0, labels, ranked);
    format!(
        "\"groups\":{},\"set_sizes\":[{}],\"checksum\":\"{hash:08x}\",\"float_bits\":\"{float_bits:016x}\",\"grand_total\":[{grand_total}]",
        pivot.groups,
        set_sizes.join(",")
    )
}

/// JSON of the durations of a computation.
pub fn timings_json(pivot: &Pivot, total: f64) -> String {
    let [levels, groups, sets, measures] = pivot.phases;
    format!(
        "{{\"total_s\":{total:.4},\"levels_s\":{levels:.4},\"groups_s\":{groups:.4},\"sets_s\":{sets:.4},\"measures_s\":{measures:.4}}}"
    )
}
