/**
 * Benchmark of the streaming of the rows to the Pivoteur, WebAssembly side: the library of
 * `src/lib.rs`, compiled to `wasm32-unknown-unknown`, run by Node (V8, the engine of Chrome) as the
 * Worker of the page would run it, on one thread. The rows are generated in the module, the way
 * the store of the Worker would receive them.
 *
 * Above 1,000,000 rows, the prototype makes no pass on all the rows: a module holds 4 GB at most,
 * and this second table, which only checks the first one, does not fit beside it at 10,000,000
 * rows. The native side makes it whatever the number of rows.
 *
 * `node wasm.mjs <module.wasm> [rows] [slice]`. The results go to stdout, as one JSON object.
 */

import { readFileSync } from 'node:fs'
import { performance } from 'node:perf_hooks'

const [modulePath, rowsArgument, sliceArgument] = process.argv.slice(2)
const rows = Number(rowsArgument ?? 1_000_000)
const slice = Number(sliceArgument ?? 100_000)
const { instance } = await WebAssembly.instantiate(readFileSync(modulePath), {
  env: { now: () => performance.now() },
})
const module = instance.exports

console.error(`wasm: ${rows} rows by slices of ${slice}`)
const length = module.bench(rows, slice, rows <= 1_000_000 ? 1 : 0) >>> 0
const address = module.report_address() >>> 0
const report = JSON.parse(
  new TextDecoder().decode(new Uint8Array(module.memory.buffer, address, length)),
)
// The memory of a module grows, and never shrinks: its size is its peak.
report.module_peak_gb = Math.round(module.memory.buffer.byteLength / 1e7) / 100
console.log(JSON.stringify(report))
