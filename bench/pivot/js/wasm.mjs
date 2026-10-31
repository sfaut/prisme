/**
 * Benchmark of the Pivoteur's engine (point A1 of its specification), WebAssembly side: the Rust
 * engine compiled to `wasm32-unknown-unknown` (`../wasm/src/lib.rs`), run by Node (V8, the
 * engine of Chrome) as the page would run it.
 *
 * Generates the rows as the JavaScript side does (`common.mjs`), and keeps them in JavaScript, as
 * the grid does. Each column is copied into the memory of the module (`transfer.mjs`), which reads
 * it into typed columns; the copy is then freed. The pivot is computed in the module, on one
 * thread; only its results come back.
 *
 * `node wasm.mjs <module.wasm> [rows]`. Progress goes to stderr, the results to stdout, as one
 * JSON object.
 */

import { readFileSync } from 'node:fs'
import { performance } from 'node:perf_hooks'

import { generate, memory, PASSES, rounded } from './common.mjs'
import { copyColumn } from './transfer.mjs'

/**
 * Number of columns of the generated result.
 */
const COLUMNS = 10

/**
 * Names of the profiles, in the order of `PROFILES` in the engine.
 */
const PROFILE_NAMES = ['high', 'low']

const [modulePath, rowsArgument] = process.argv.slice(2)
const count = Number(rowsArgument ?? 10_000_000)
const { instance } = await WebAssembly.instantiate(readFileSync(modulePath), {
  env: { now: () => performance.now() },
})
const engine = instance.exports

/**
 * Size of the memory of the module, in gigabytes: it grows, and never shrinks.
 */
function linearMemory() {
  return engine.memory.buffer.byteLength / 1e9
}

console.error(`wasm: generating ${count} rows`)
let started = performance.now()
const rows = generate(count)
const generation = (performance.now() - started) / 1000
const afterGeneration = memory()

console.error('wasm: copying the columns into the module and reading them')
let copying = 0
let preparation = 0
for (let column = 0; column < COLUMNS; column++) {
  started = performance.now()
  const copy = copyColumn(rows, column, engine)
  copying += performance.now() - started
  started = performance.now()
  engine.read_column(column, copy.bytes, copy.length, copy.offsets, copy.valid, count)
  preparation += performance.now() - started
  engine.release(copy.bytes, copy.capacity)
  engine.release(copy.offsets, (count + 1) * 4)
  engine.release(copy.valid, count)
}
const afterPreparation = memory()
const linearAfterPreparation = linearMemory()

const decoder = new TextDecoder()
const profiles = PROFILE_NAMES.map((name, profile) => {
  console.error(`wasm: profile ${name}, one thread`)
  let best = null
  for (let pass = 0; pass < PASSES; pass++) {
    const total = engine.compute(profile)
    if (best === null || total < best.total) {
      best = { total, phases: [0, 1, 2, 3].map((index) => engine.phase(index)) }
    }
  }
  const length = engine.report() >>> 0
  const address = engine.report_address() >>> 0
  const report = JSON.parse(
    `{${decoder.decode(new Uint8Array(engine.memory.buffer, address, length))}}`,
  )
  const [levels, groups, sets, measures] = best.phases
  return {
    name,
    one_thread: {
      total_s: rounded(best.total, 4),
      levels_s: rounded(levels, 4),
      groups_s: rounded(groups, 4),
      sets_s: rounded(sets, 4),
      measures_s: rounded(measures, 4),
    },
    all_threads: null,
    ...report,
    serialization: null,
  }
})

const peak = process.resourceUsage().maxRSS * 1024
console.log(
  JSON.stringify({
    engine: 'wasm',
    rows: count,
    threads: 1,
    generation_s: rounded(generation, 2),
    copy_s: rounded(copying / 1000, 4),
    preparation: { one_thread_s: rounded(preparation / 1000, 4), all_threads_s: null },
    memory_gb: {
      after_generation: rounded(afterGeneration.rss, 2),
      after_preparation: rounded(afterPreparation.rss, 2),
      peak: rounded(peak / 1e9, 2),
      heap_after_generation: rounded(afterGeneration.heap, 2),
      heap_after_preparation: rounded(afterPreparation.heap, 2),
      module_after_preparation: rounded(linearAfterPreparation, 2),
      module_peak: rounded(linearMemory(), 2),
    },
    profiles,
  }),
)
