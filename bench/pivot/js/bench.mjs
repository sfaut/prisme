/**
 * Benchmark of the Pivoteur's engine (point A1 of its specification), JavaScript side, run by
 * Node (V8, the engine of Chrome): what the TypeScript of the interface becomes once compiled.
 *
 * Generates the rows of a result as the page receives them (batches parsed from JSON, of 500
 * rows here, cells `string | null`), prepares typed columns once, then computes the pivot several
 * times: 4 row levels, 2 column levels (`YEAR(d)`, `MONTH(d)`), 12 measures, every subtotal and
 * total. The rows are those of the other sides (`common.mjs`), which must give the same
 * checksums; `../run.mjs` runs them all and compares them. The code follows the same shape as the
 * Rust engine (`../engine/src/lib.rs`), phase by phase.
 *
 * Progress goes to stderr, the results to stdout, as one JSON object.
 */

import { performance } from 'node:perf_hooks'

import { civil, datetimeText, fastest, generate, memory, rounded } from './common.mjs'

/**
 * Days since 1970-01-01 of a date of the civil calendar (H. Hinnant's algorithm).
 *
 * @param {number} year Year.
 * @param {number} month Month, from 1 to 12.
 * @param {number} day Day of the month.
 */
function daysFromCivil(year, month, day) {
  const shiftedYear = month <= 2 ? year - 1 : year
  const era = Math.floor(shiftedYear / 400)
  const yearOfEra = shiftedYear - era * 400
  const dayOfYear = Math.floor((153 * ((month + 9) % 12) + 2) / 5) + day - 1
  const dayOfEra =
    yearOfEra * 365 + Math.floor(yearOfEra / 4) - Math.floor(yearOfEra / 100) + dayOfYear
  return era * 146_097 + dayOfEra - 719_468
}

/**
 * `YEAR()` of a number of days.
 *
 * @param {number} days Days since 1970-01-01.
 */
function yearOf(days) {
  return Math.floor(civil(days) / 512)
}

/**
 * `MONTH()` of a number of days.
 *
 * @param {number} days Days since 1970-01-01.
 */
function monthOf(days) {
  return (civil(days) >> 5) & 15
}

/**
 * Encodes a text column as a dictionary: one code per row, one value per code, in the order of
 * their first appearance; NULL has its own code.
 *
 * @param {(string | null)[][]} rows Rows of the result.
 * @param {number} column Index of the column.
 */
function dictionary(rows, column) {
  const index = new Map()
  const values = []
  let nullCode = -1
  const codes = new Uint32Array(rows.length)
  for (let row = 0; row < rows.length; row++) {
    const text = rows[row][column]
    let code
    if (text === null) {
      if (nullCode < 0) {
        nullCode = values.length
        values.push('')
      }
      code = nullCode
    } else {
      code = index.get(text)
      if (code === undefined) {
        code = values.length
        index.set(text, code)
        values.push(text)
      }
    }
    codes[row] = code
  }
  return { codes, values }
}

/**
 * Reads a column of numbers (exact ones, scaled to integers, or floats), with the validity of
 * each row; an invalid value becomes NULL.
 *
 * @param {(string | null)[][]} rows Rows of the result.
 * @param {number} column Index of the column.
 * @param {(text: string) => number | null} parse Reading of a value.
 */
function numbers(rows, column, parse) {
  const values = new Float64Array(rows.length)
  const valid = new Uint8Array(rows.length)
  for (let row = 0; row < rows.length; row++) {
    const text = rows[row][column]
    const value = text === null ? null : parse(text)
    if (value !== null) {
      values[row] = value
      valid[row] = 1
    }
  }
  return { values, valid }
}

/**
 * Value of a run of ASCII digits, `null` if one character is not a digit.
 *
 * @param {string} text Text.
 * @param {number} start Index of the first digit.
 * @param {number} end Index after the last digit.
 */
function digitsValue(text, start, end) {
  let value = 0
  for (let index = start; index < end; index++) {
    const digit = text.charCodeAt(index) - 48
    if (digit < 0 || digit > 9) {
      return null
    }
    value = value * 10 + digit
  }
  return value
}

/**
 * Reads an integer, with an optional minus sign.
 *
 * @param {string} text Text.
 */
function parseInteger(text) {
  const start = text.charCodeAt(0) === 45 ? 1 : 0
  if (start === text.length) {
    return null
  }
  const value = digitsValue(text, start, text.length)
  return value === null ? null : start === 1 ? -value : value
}

/**
 * Reads a decimal as an integer scaled by 10^`scale` (`12.5` with a scale of 2 gives 1250);
 * extra digits are rounded half away from zero.
 *
 * @param {string} text Text.
 * @param {number} scale Number of digits after the point.
 */
function parseDecimal(text, scale) {
  const start = text.charCodeAt(0) === 45 ? 1 : 0
  const point = text.indexOf('.', start)
  const wholeEnd = point < 0 ? text.length : point
  const fractionStart = point < 0 ? text.length : point + 1
  const fractionLength = text.length - fractionStart
  if (wholeEnd === start && fractionLength === 0) {
    return null
  }
  let value = digitsValue(text, start, wholeEnd)
  if (value === null) {
    return null
  }
  for (let position = 0; position < scale; position++) {
    let digit = 0
    if (position < fractionLength) {
      digit = text.charCodeAt(fractionStart + position) - 48
      if (digit < 0 || digit > 9) {
        return null
      }
    }
    value = value * 10 + digit
  }
  if (fractionLength > scale) {
    const rest = digitsValue(text, fractionStart + scale, text.length)
    if (rest === null) {
      return null
    }
    if (text.charCodeAt(fractionStart + scale) >= 53 && rest > 0) {
      value += 1
    }
  }
  return start === 1 ? -value : value
}

/**
 * Reads a `YYYY-MM-DD` date or a `YYYY-MM-DD HH:MM:SS` datetime, as seconds since 1970-01-01.
 *
 * @param {string} text Text.
 */
function parseDatetime(text) {
  if (text.length !== 10 && text.length !== 19) {
    return null
  }
  const year = digitsValue(text, 0, 4)
  const month = digitsValue(text, 5, 7)
  const day = digitsValue(text, 8, 10)
  if (year === null || month === null || day === null) {
    return null
  }
  let seconds = daysFromCivil(year, month, day) * 86_400
  if (text.length === 19) {
    const hours = digitsValue(text, 11, 13)
    const minutes = digitsValue(text, 14, 16)
    const rest = digitsValue(text, 17, 19)
    if (hours === null || minutes === null || rest === null) {
      return null
    }
    seconds += hours * 3_600 + minutes * 60 + rest
  }
  return seconds
}

/**
 * Reads a float.
 *
 * @param {string} text Text.
 */
function parseFloatValue(text) {
  const value = Number(text)
  return text === '' || Number.isNaN(value) ? null : value
}

/**
 * Rank of each value of a dictionary in the binary order of the texts (the order of the UTF-16
 * code units, the same as the bytes for ASCII).
 *
 * @param {string[]} values Values of the dictionary.
 */
function ranks(values) {
  const order = values.map((_, code) => code)
  order.sort((a, b) => (values[a] < values[b] ? -1 : values[a] > values[b] ? 1 : 0))
  const rank = new Uint32Array(values.length)
  order.forEach((code, position) => {
    rank[code] = position
  })
  return rank
}

/**
 * Reads the typed columns of the result, once.
 *
 * @param {(string | null)[][]} rows Rows of the result.
 */
function prepare(rows) {
  const levels = [0, 1, 2, 3, 4].map((column) => dictionary(rows, column))
  const client = dictionary(rows, 9)
  const rank = ranks(client.values)
  const clientRank = {
    values: Float64Array.from(client.codes, (code) => rank[code]),
    valid: new Uint8Array(rows.length).fill(1),
  }
  return {
    levels,
    client,
    date: numbers(rows, 5, parseDatetime),
    quantity: numbers(rows, 6, parseInteger),
    amount: numbers(rows, 7, (text) => parseDecimal(text, 2)),
    discount: numbers(rows, 8, parseFloatValue),
    clientRank,
  }
}

/**
 * Codes of a level, one per row, and its number of distinct codes.
 *
 * @param {{ kind: 'column', dictionary: { codes: Uint32Array, values: string[] } }
 *   | { kind: 'year' | 'month', column: { values: Float64Array, valid: Uint8Array } }} level
 *     Expression of the level: a text column, or a part of a date.
 */
function evaluate(level) {
  if (level.kind === 'column') {
    return { codes: level.dictionary.codes, card: level.dictionary.values.length }
  }
  const part = level.kind === 'year' ? yearOf : monthOf
  const { values, valid } = level.column
  const index = new Map()
  let count = 0
  let nullCode = -1
  const codes = new Uint32Array(values.length)
  for (let row = 0; row < values.length; row++) {
    let code
    if (valid[row] !== 0) {
      const key = part(Math.floor(values[row] / 86_400))
      code = index.get(key)
      if (code === undefined) {
        code = count++
        index.set(key, code)
      }
    } else {
      if (nullCode < 0) {
        nullCode = count++
      }
      code = nullCode
    }
    codes[row] = code
  }
  return { codes, card: count }
}

/**
 * Merges the states of each grouping set into those of the coarser one, by addition.
 *
 * @param {Float64Array} values States of all the groups.
 * @param {{ source: number, parentOf: Uint32Array }[]} groupings Grouping sets, in the order of
 *   their computation.
 */
function mergeAdd(values, groupings) {
  for (const { source, parentOf } of groupings) {
    for (let offset = 0; offset < parentOf.length; offset++) {
      values[parentOf[offset]] += values[source + offset]
    }
  }
}

/**
 * Merges the states of each grouping set into those of the coarser one, keeping the smallest
 * (`keepLess`) or the largest value.
 *
 * @param {Float64Array} values States of all the groups.
 * @param {{ source: number, parentOf: Uint32Array }[]} groupings Grouping sets, in the order of
 *   their computation.
 * @param {boolean} keepLess Keep the smallest value?
 */
function mergeExtreme(values, groupings, keepLess) {
  for (const { source, parentOf } of groupings) {
    for (let offset = 0; offset < parentOf.length; offset++) {
      const value = values[source + offset]
      const parent = parentOf[offset]
      if (keepLess ? value < values[parent] : value > values[parent]) {
        values[parent] = value
      }
    }
  }
}

/**
 * Computes the states of a measure: rows into the finest groups, then the merge into the
 * subtotals and totals.
 *
 * @param {{ kind: string, column?: { values: Float64Array, valid: Uint8Array } }} measure
 *   Expression of the measure.
 * @param {Uint32Array} groupOf Finest group of each row.
 * @param {number} groups Number of groups, all grouping sets included.
 * @param {{ source: number, parentOf: Uint32Array }[]} groupings Grouping sets, in the order of
 *   their computation.
 */
function accumulate(measure, groupOf, groups, groupings) {
  const rows = groupOf.length
  switch (measure.kind) {
    case 'countStar': {
      const counts = new Float64Array(groups)
      for (let row = 0; row < rows; row++) {
        counts[groupOf[row]]++
      }
      mergeAdd(counts, groupings)
      return { kind: 'count', counts }
    }
    case 'count': {
      const { valid } = measure.column
      const counts = new Float64Array(groups)
      for (let row = 0; row < rows; row++) {
        if (valid[row] !== 0) {
          counts[groupOf[row]]++
        }
      }
      mergeAdd(counts, groupings)
      return { kind: 'count', counts }
    }
    case 'sum':
    case 'avg':
    case 'avgFloat': {
      const { values, valid } = measure.column
      const sums = new Float64Array(groups)
      const counts = new Float64Array(groups)
      for (let row = 0; row < rows; row++) {
        if (valid[row] !== 0) {
          sums[groupOf[row]] += values[row]
          counts[groupOf[row]]++
        }
      }
      mergeAdd(sums, groupings)
      mergeAdd(counts, groupings)
      return { kind: measure.kind === 'avgFloat' ? 'floatSum' : 'sum', sums, counts }
    }
    case 'min':
    case 'max': {
      const { values, valid } = measure.column
      const keepLess = measure.kind === 'min'
      const extremes = new Float64Array(groups).fill(keepLess ? Infinity : -Infinity)
      for (let row = 0; row < rows; row++) {
        const value = values[row]
        const group = groupOf[row]
        if (valid[row] !== 0 && (keepLess ? value < extremes[group] : value > extremes[group])) {
          extremes[group] = value
        }
      }
      mergeExtreme(extremes, groupings, keepLess)
      return { kind: measure.kind, values: extremes }
    }
  }
  throw new Error(`unknown measure ${measure.kind}`)
}

/**
 * Product of numbers of distinct codes.
 *
 * @param {number[]} cards Numbers of distinct codes.
 */
function product(cards) {
  return cards.reduce((total, card) => total * card, 1)
}

/**
 * Computes the pivot, every subtotal and total included.
 *
 * Phases: the levels give a code per row; the codes of a row, read in a mixed radix, give the
 * key of its finest group, numbered through a hash table; each grouping set is built from a
 * finer one (rows, then columns); each measure fills its states, then merges them set by set.
 *
 * @param {object[]} rowLevels Levels of the rows, the outermost first.
 * @param {object[]} columnLevels Levels of the columns, the outermost first.
 * @param {object[]} measures Measures.
 */
function compute(rowLevels, columnLevels, measures) {
  let started = performance.now()
  const evaluated = [...rowLevels, ...columnLevels].map(evaluate)
  const cards = evaluated.map(({ card }) => card)
  const codes = evaluated.map(({ codes }) => codes)
  const levelsDone = (performance.now() - started) / 1000

  started = performance.now()
  const rows = codes[0].length
  const levelCount = codes.length
  const index = new Map()
  const keys = []
  const groupOf = new Uint32Array(rows)
  for (let row = 0; row < rows; row++) {
    let key = 0
    for (let level = 0; level < levelCount; level++) {
      key = key * cards[level] + codes[level][row]
    }
    let group = index.get(key)
    if (group === undefined) {
      group = keys.length
      index.set(key, group)
      keys.push(key)
    }
    groupOf[row] = group
  }
  const groupsDone = (performance.now() - started) / 1000

  started = performance.now()
  const rowCount = rowLevels.length
  const columnCount = columnLevels.length
  const rowCards = cards.slice(0, rowCount)
  const columnCards = cards.slice(rowCount)
  const columnProduct = product(columnCards)
  const sets = Array.from({ length: rowCount + 1 }, () =>
    Array.from({ length: columnCount + 1 }, () => [0, 0]),
  )
  sets[rowCount][columnCount] = [0, keys.length]
  const groupings = []
  for (let keptRows = rowCount; keptRows >= 0; keptRows--) {
    for (let keptColumns = columnCount; keptColumns >= 0; keptColumns--) {
      if (keptRows === rowCount && keptColumns === columnCount) {
        continue
      }
      const [sourceRows, sourceColumns] =
        keptColumns < columnCount ? [keptRows, keptColumns + 1] : [keptRows + 1, keptColumns]
      const [source, sourceLength] = sets[sourceRows][sourceColumns]
      const rowDivisor = product(rowCards.slice(keptRows))
      const columnDivisor = product(columnCards.slice(keptColumns))
      const columnPrefix = product(columnCards.slice(0, keptColumns))
      const start = keys.length
      const parents = new Map()
      const parentOf = new Uint32Array(sourceLength)
      for (let offset = 0; offset < sourceLength; offset++) {
        const key = keys[source + offset]
        const parentKey =
          Math.floor(Math.floor(key / columnProduct) / rowDivisor) * columnPrefix +
          Math.floor((key % columnProduct) / columnDivisor)
        let parent = parents.get(parentKey)
        if (parent === undefined) {
          parent = keys.length
          parents.set(parentKey, parent)
          keys.push(key)
        }
        parentOf[offset] = parent
      }
      sets[keptRows][keptColumns] = [start, keys.length - start]
      groupings.push({ source, parentOf })
    }
  }
  const groups = keys.length
  const setsDone = (performance.now() - started) / 1000

  started = performance.now()
  const states = measures.map((measure) => accumulate(measure, groupOf, groups, groupings))
  const measuresDone = (performance.now() - started) / 1000

  return { groups, sets, states, phases: [levelsDone, groupsDone, setsDone, measuresDone] }
}

/**
 * Checksum of the states: a 32-bit FNV-1a hash of the exact values, in the order of the
 * measures then of the groups, and the bits of the sum of the float sums.
 *
 * @param {object[]} states States of the measures.
 */
function checksum(states) {
  let hash = 2_166_136_261
  let floatTotal = 0
  const mix = (value) => {
    hash = Math.imul(hash ^ (value >>> 0), 16_777_619) >>> 0
    hash = Math.imul(hash ^ (Math.floor(value / 4_294_967_296) >>> 0), 16_777_619) >>> 0
  }
  for (const state of states) {
    switch (state.kind) {
      case 'count':
        state.counts.forEach(mix)
        break
      case 'sum':
        state.sums.forEach(mix)
        state.counts.forEach(mix)
        break
      case 'floatSum':
        state.sums.forEach((sum) => {
          floatTotal += sum
        })
        state.counts.forEach(mix)
        break
      default:
        state.values.forEach((value) => mix(Number.isFinite(value) ? value : 0))
    }
  }
  const bits = new DataView(new ArrayBuffer(8))
  bits.setFloat64(0, floatTotal)
  return {
    hash: hash.toString(16).padStart(8, '0'),
    floatBits: bits.getBigUint64(0).toString(16).padStart(16, '0'),
  }
}

/**
 * Text of a decimal scaled to an integer.
 *
 * @param {bigint} value Value multiplied by 10^`scale`.
 * @param {number} scale Number of digits after the point.
 */
function decimalText(value, scale) {
  const sign = value < 0n ? '-' : ''
  const digits = (value < 0n ? -value : value).toString().padStart(scale + 1, '0')
  return `${sign}${digits.slice(0, -scale)}.${digits.slice(-scale)}`
}

/**
 * Values of the measures of a group, in the order of the measures, as the Rust engine writes them
 * in JSON; the average of a decimal keeps the scale of its column + 4, as in MySQL, rounded half
 * away from zero.
 *
 * @param {object} pivot Computed pivot.
 * @param {number} group Group.
 * @param {[object, { type: string, scale?: number }][]} labels Measures, with the type of their
 *   values.
 * @param {string[]} ranked Value of each rank of `client`.
 */
function groupValues(pivot, group, labels, ranked) {
  const exact = (value, format) => {
    switch (format.type) {
      case 'decimal':
        return decimalText(BigInt(value), format.scale)
      case 'datetime':
        return datetimeText(value)
      case 'text':
        return ranked[value]
      default:
        return value
    }
  }
  return pivot.states.map((state, index) => {
    const [measure, format] = labels[index]
    switch (state.kind) {
      case 'count':
        return state.counts[group]
      case 'sum': {
        if (state.counts[group] === 0) {
          return null
        }
        if (measure.kind === 'avg') {
          const scaled = BigInt(state.sums[group]) * 10_000n
          const count = BigInt(state.counts[group])
          return decimalText((2n * scaled + count) / (2n * count), format.scale + 4)
        }
        return exact(state.sums[group], format)
      }
      case 'floatSum':
        return state.counts[group] === 0 ? null : state.sums[group] / state.counts[group]
      default:
        return Number.isFinite(state.values[group]) ? exact(state.values[group], format) : null
    }
  })
}

const count = Number(process.argv[2] ?? 10_000_000)

console.error(`javascript: generating ${count} rows`)
let started = performance.now()
const rows = generate(count)
const generation = (performance.now() - started) / 1000
const afterGeneration = memory()

console.error('javascript: preparing the typed columns')
started = performance.now()
const prepared = prepare(rows)
const preparation = (performance.now() - started) / 1000
const afterPreparation = memory()

const rank = ranks(prepared.client.values)
const ranked = new Array(rank.length)
rank.forEach((position, code) => {
  ranked[position] = prepared.client.values[code]
})
const { quantity, amount, discount, date, clientRank } = prepared
const integer = { type: 'integer' }
const cents = { type: 'decimal', scale: 2 }
const labels = [
  [{ kind: 'countStar' }, integer],
  [{ kind: 'count', column: quantity }, integer],
  [{ kind: 'sum', column: quantity }, integer],
  [{ kind: 'sum', column: amount }, cents],
  [{ kind: 'avg', column: amount }, cents],
  [{ kind: 'avgFloat', column: discount }, { type: 'float' }],
  [{ kind: 'min', column: amount }, cents],
  [{ kind: 'max', column: amount }, cents],
  [{ kind: 'min', column: date }, { type: 'datetime' }],
  [{ kind: 'max', column: date }, { type: 'datetime' }],
  [{ kind: 'min', column: clientRank }, { type: 'text' }],
  [{ kind: 'max', column: clientRank }, { type: 'text' }],
]
const measures = labels.map(([measure]) => measure)
const columnLevels = [
  { kind: 'year', column: date },
  { kind: 'month', column: date },
]
const profiles = [
  ['high', [0, 1, 2, 3]], // pays, region, categorie, produit
  ['low', [0, 1, 2, 4]], // pays, region, categorie, canal
]

const profileResults = profiles.map(([name, columns]) => {
  console.error(`javascript: profile ${name}, one thread`)
  const rowLevels = columns.map((column) => ({
    kind: 'column',
    dictionary: prepared.levels[column],
  }))
  const { pivot, total } = fastest(() => {
    const begun = performance.now()
    const pivot = compute(rowLevels, columnLevels, measures)
    return { pivot, total: (performance.now() - begun) / 1000 }
  })
  const [levels, groups, sets, measuresTime] = pivot.phases
  const { hash, floatBits } = checksum(pivot.states)
  return {
    name,
    one_thread: {
      total_s: rounded(total, 4),
      levels_s: rounded(levels, 4),
      groups_s: rounded(groups, 4),
      sets_s: rounded(sets, 4),
      measures_s: rounded(measuresTime, 4),
    },
    all_threads: null,
    groups: pivot.groups,
    set_sizes: pivot.sets.flat().map(([, length]) => length),
    checksum: hash,
    float_bits: floatBits,
    grand_total: groupValues(pivot, pivot.sets[0][0][0], labels, ranked),
    serialization: null,
  }
})

const peak = process.resourceUsage().maxRSS * 1024
console.log(
  JSON.stringify({
    engine: 'javascript',
    rows: count,
    threads: 1,
    generation_s: rounded(generation, 2),
    preparation: { one_thread_s: rounded(preparation, 4), all_threads_s: null },
    memory_gb: {
      after_generation: rounded(afterGeneration.rss, 2),
      after_preparation: rounded(afterPreparation.rss, 2),
      peak: rounded(peak / 1e9, 2),
      heap_after_generation: rounded(afterGeneration.heap, 2),
      heap_after_preparation: rounded(afterPreparation.heap, 2),
    },
    profiles: profileResults,
  }),
)
