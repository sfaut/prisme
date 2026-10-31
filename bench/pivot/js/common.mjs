/**
 * Shared by the JavaScript and WebAssembly sides of the benchmark of the Pivoteur's engine: the
 * generation of the rows, the same as on the native side (same pseudo-random generator, same
 * seed), and the measures of time and memory.
 */

/**
 * Number of passes of each computation; the fastest one is kept.
 */
export const PASSES = 3

/**
 * Seed of the pseudo-random generator, shared with the native side.
 */
const SEED = 42

/**
 * Days from 1970-01-01 to 2024-01-01, start of the dates of the rows.
 */
const START_DAYS = 19_723

/**
 * Span of the dates of the rows: 1,096 days (2024 to 2026), in seconds.
 */
const SPAN_SECONDS = 1_096 * 86_400

/**
 * Sales channels, values of the `canal` column.
 */
const CHANNELS = ['web', 'magasin', 'telephone', 'partenaire']

/**
 * Rows per message of the server, as the query session sends them.
 */
const BATCH = 500

/**
 * Pseudo-random generator Mulberry32, written the same way on the native side.
 *
 * @param {number} seed Seed.
 * @returns {(bound: number) => number} Next number, between 0 and `bound` - 1.
 */
function generator(seed) {
  let state = seed | 0
  return (bound) => {
    state = (state + 0x6d2b79f5) | 0
    let t = Math.imul(state ^ (state >>> 15), 1 | state)
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t
    return ((t ^ (t >>> 14)) >>> 0) % bound
  }
}

/**
 * Date of the civil calendar of a number of days since 1970-01-01, packed in one number
 * (`(year * 16 + month) * 32 + day`), so that the levels allocate nothing per row.
 *
 * @param {number} days Days since 1970-01-01.
 */
export function civil(days) {
  const shifted = days + 719_468
  const era = Math.floor(shifted / 146_097)
  const dayOfEra = shifted - era * 146_097
  const yearOfEra = Math.floor(
    (dayOfEra -
      Math.floor(dayOfEra / 1_460) +
      Math.floor(dayOfEra / 36_524) -
      Math.floor(dayOfEra / 146_096)) /
      365,
  )
  const dayOfYear =
    dayOfEra - (365 * yearOfEra + Math.floor(yearOfEra / 4) - Math.floor(yearOfEra / 100))
  const shiftedMonth = Math.floor((5 * dayOfYear + 2) / 153)
  const day = dayOfYear - Math.floor((153 * shiftedMonth + 2) / 5) + 1
  const month = shiftedMonth < 10 ? shiftedMonth + 3 : shiftedMonth - 9
  const year = yearOfEra + era * 400 + (month <= 2 ? 1 : 0)
  return (year * 16 + month) * 32 + day
}

/**
 * Number written with at least `width` digits.
 *
 * @param {number} value Number.
 * @param {number} width Number of digits.
 */
function pad(value, width) {
  return String(value).padStart(width, '0')
}

/**
 * Text of a datetime given in seconds since 1970-01-01.
 *
 * @param {number} seconds Seconds since 1970-01-01.
 */
export function datetimeText(seconds) {
  const date = civil(Math.floor(seconds / 86_400))
  const time = seconds - Math.floor(seconds / 86_400) * 86_400
  const year = Math.floor(date / 512)
  const month = (date >> 5) & 15
  const day = date & 31
  const clock = `${pad(Math.floor(time / 3_600), 2)}:${pad(Math.floor(time / 60) % 60, 2)}:${pad(time % 60, 2)}`
  return `${pad(year, 4)}-${pad(month, 2)}-${pad(day, 2)} ${clock}`
}

/**
 * Generates the rows: `pays`, `region`, `categorie`, `produit`, `canal`, `d` (datetime), `qte`
 * (integer, 5 % of NULL), `montant` (decimal with 2 digits), `remise` (float), `client`.
 *
 * @param {number} count Number of rows.
 * @returns {(string | null)[][]} Rows, read from JSON batches as the page reads them.
 */
export function generate(count) {
  const below = generator(SEED)
  const rows = []
  for (let start = 0; start < count; start += BATCH) {
    const batch = []
    for (let row = start; row < Math.min(count, start + BATCH); row++) {
      const country = below(10)
      const region = below(5)
      const category = below(20)
      const product = below(50)
      const channel = below(4)
      const seconds = below(SPAN_SECONDS)
      const quantity = below(20) === 0 ? null : 1 + below(100)
      const cents = 1 + below(9_999_999)
      const discount = below(500_000)
      const client = below(100_000)
      batch.push([
        `P${country}`,
        `P${country}-R${region}`,
        `Cat${pad(category, 2)}`,
        `Cat${pad(category, 2)}-P${pad(product, 2)}`,
        CHANNELS[channel],
        datetimeText(START_DAYS * 86_400 + seconds),
        quantity === null ? null : String(quantity),
        `${Math.floor(cents / 100)}.${pad(cents % 100, 2)}`,
        `0.${pad(discount, 6)}`,
        `C${pad(client, 6)}`,
      ])
    }
    for (const row of JSON.parse(JSON.stringify(batch))) {
      rows.push(row)
    }
  }
  return rows
}

/**
 * Fastest of several passes of a computation.
 *
 * @param {() => { total: number }} run Computation, returning its result and its duration.
 */
export function fastest(run) {
  let best = run()
  for (let pass = 1; pass < PASSES; pass++) {
    const next = run()
    if (next.total < best.total) {
      best = next
    }
  }
  return best
}

/**
 * Memory of the process, in gigabytes: resident set and V8 heap (objects and texts, without the
 * typed arrays), after a full garbage collection when Node allows it (`--expose-gc`).
 */
export function memory() {
  globalThis.gc?.()
  const { rss, heapUsed } = process.memoryUsage()
  return { rss: rss / 1e9, heap: heapUsed / 1e9 }
}

/**
 * Rounds a duration or a size for the JSON of the results.
 *
 * @param {number} value Value.
 * @param {number} digits Digits after the point.
 */
export function rounded(value, digits) {
  return Number(value.toFixed(digits))
}
