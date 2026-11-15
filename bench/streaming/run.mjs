/**
 * Benchmark of the streaming of the rows to the Pivoteur (see `src/lib.rs`): runs the native side
 * and the WebAssembly side of the engine (full computation against incremental aggregates, then
 * the engine of Prisme streamed), then the cost of sending the rows to a Worker on the main thread (`post.mjs`), and prints a summary.
 *
 * From the root of the repository: `node bench/streaming/run.mjs [rows] [slice] [passes]`,
 * 1,000,000 rows by slices of 100,000, 3 passes by default: each side of the engine runs that many
 * times, and each duration keeps its fastest run. Requires Cargo (the toolchain of `rust-toolchain.toml`) and
 * Node; the WebAssembly target of the toolchain is added when missing. The full results, with the
 * machine, go to `target/streaming-bench/results-<rows>.json`. The exit code is 1 if the prototype
 * or the streamed engine does not check itself on one side.
 */

import { execFileSync, spawnSync } from 'node:child_process'
import { mkdirSync, writeFileSync } from 'node:fs'
import os from 'node:os'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

/**
 * Heap allowed to Node, in megabytes: the text cells of 10 million rows exceed its default.
 */
const NODE_HEAP_MB = 24_576

/**
 * Target of the WebAssembly side.
 */
const WASM_TARGET = 'wasm32-unknown-unknown'

const here = dirname(fileURLToPath(import.meta.url))
const root = resolve(here, '../..')
const targetDir = join(root, 'target', 'streaming-bench')
const rows = process.argv[2] ?? '1000000'
const slice = process.argv[3] ?? '100000'
const passes = Number(process.argv[4] ?? 3)

/**
 * Runs a command, its output shown as it goes, and stops on failure.
 *
 * @param {string} command Program.
 * @param {string[]} args Arguments.
 */
function run(command, args) {
  const done = spawnSync(command, args, { cwd: root, stdio: 'inherit' })
  if (done.status !== 0) {
    console.error(`${command} failed (exit code ${done.status})`)
    process.exit(done.status ?? 1)
  }
}

/**
 * Runs a side of the benchmark, its progress shown as it goes.
 *
 * @param {string} command Program.
 * @param {string[]} args Arguments.
 * @returns {object} Results printed by the side, as JSON.
 */
function runSide(command, args) {
  const side = spawnSync(command, args, {
    cwd: root,
    stdio: ['ignore', 'pipe', 'inherit'],
    maxBuffer: 64 * 1024 * 1024,
    encoding: 'utf8',
  })
  if (side.status !== 0) {
    console.error(`${command} failed (exit code ${side.status})`)
    process.exit(side.status ?? 1)
  }
  return JSON.parse(side.stdout)
}

/**
 * Smallest of several durations, or of several lists of durations position by position; `null`
 * for a duration not measured.
 *
 * @param {(number | number[] | null)[]} values Durations, one per run.
 */
function smallest(values) {
  const [first] = values
  if (Array.isArray(first)) {
    return first.map((_, index) => Math.min(...values.map((value) => value[index])))
  }
  return values.includes(null) ? null : Math.min(...values)
}

/**
 * Fastest of several runs of a side: each duration (a field ending in `_s`) is the smallest of
 * its runs, the other fields those of the first run.
 *
 * @param {object[]} runs Results of each run, of the same shape.
 * @returns {object} Results of the fastest runs.
 */
function fastest(runs) {
  const [first, ...others] = runs
  if (Array.isArray(first)) {
    return first.map((item, index) => fastest([item, ...others.map((other) => other[index])]))
  }
  if (first === null || typeof first !== 'object') {
    return first
  }
  return Object.fromEntries(
    Object.entries(first).map(([key, value]) => [
      key,
      key.endsWith('_s')
        ? smallest([value, ...others.map((other) => other[key])])
        : key === 'checked'
          ? others.every((other) => other[key]) && value
          : fastest([value, ...others.map((other) => other[key])]),
    ]),
  )
}

/**
 * Runs a side `passes` times and keeps its fastest durations (see `fastest`).
 *
 * @param {string} command Program.
 * @param {string[]} args Arguments.
 * @returns {object} Results of the side.
 */
function runPasses(command, args) {
  return fastest(Array.from({ length: passes }, () => runSide(command, args)))
}

/**
 * Duration in milliseconds, without decimals, or a dash when not measured.
 *
 * @param {number | null} seconds Duration, in seconds.
 */
function ms(seconds) {
  return seconds === null ? '–' : `${Math.round(seconds * 1000).toLocaleString('en')} ms`
}

/**
 * Rows of the summary for one side of the engine: for each profile, each checkpoint of the
 * reception, then the whole reception.
 *
 * @param {string} name Name of the side.
 * @param {object} side Results of the side.
 */
function engineLines(name, side) {
  const lines = [
    `${name}: appending the slices to the store, ${ms(side.append_s)} for the whole reception, ${ms(side.longest_append_s)} at most per slice${side.module_peak_gb === undefined ? '' : `; memory of the module at its peak: ${side.module_peak_gb} GB`}`,
    '',
    '| Profile | Rows received | Full: new engine, caches and table | Full: table, caches kept | Incremental: update by one slice | Incremental: one pass on all the rows | Groups |',
    '| --- | ---: | ---: | ---: | ---: | ---: | ---: |',
  ]
  for (const profile of side.profiles) {
    profile.full.forEach((full, index) => {
      const update = profile.updates[index]
      lines.push(
        `| ${profile.name} | ${full.rows.toLocaleString('en')} | ${ms(full.first_s)} | ${ms(full.table_s)} | ${ms(update.update_s)} | ${ms(update.one_pass_s)} | ${update.groups.toLocaleString('en')} |`,
      )
    })
  }
  lines.push(
    '',
    '| Profile | All the updates | Longest update | Reading / groups / states / merge | Checked |',
    '| --- | ---: | ---: | ---: | --- |',
  )
  for (const profile of side.profiles) {
    lines.push(
      `| ${profile.name} | ${ms(profile.updates_s)} | ${ms(profile.longest_update_s)} | ${profile.phases_s.map(ms).join(' / ')} | ${profile.checked ? 'yes' : 'NO'} |`,
    )
  }
  return lines
}

/**
 * Rows of the summary for the engine of Prisme streamed (`src/engine.rs`), native / WebAssembly:
 * each checkpoint of the reception, then the whole reception.
 *
 * @param {object} native Results of the native side.
 * @param {object} wasm Results of the WebAssembly side.
 */
function streamedLines(native, wasm) {
  const both = (seconds) => `${ms(seconds(native))} / ${ms(seconds(wasm))}`
  const lines = [
    'Engine of Prisme streamed (Engine::update), native / WebAssembly',
    '',
    '| Profile | Rows received | grow, by one slice | update, by one slice | Finest groups |',
    '| --- | ---: | ---: | ---: | ---: |',
  ]
  native.engine.forEach((profile, index) => {
    profile.updates.forEach((update, position) => {
      const at = (side) => side.engine[index].updates[position]
      lines.push(
        `| ${profile.name} | ${update.rows.toLocaleString('en')} | ${both((side) => at(side).grow_s)} | ${both((side) => at(side).update_s)} | ${update.finest_groups.toLocaleString('en')} |`,
      )
    })
  })
  lines.push(
    '',
    '| Profile | All the grow | All the update | Longest slice | Table of all the rows, caches kept | Checked |',
    '| --- | ---: | ---: | ---: | ---: | --- |',
  )
  native.engine.forEach((profile, index) => {
    const checked = profile.checked && wasm.engine[index].checked
    const of = (side) => side.engine[index]
    lines.push(
      `| ${profile.name} | ${both((side) => of(side).grows_s)} | ${both((side) => of(side).updates_s)} | ${both((side) => of(side).longest_update_s)} | ${both((side) => of(side).full_s)} | ${checked ? 'yes' : 'NO'} |`,
    )
  })
  return lines
}

const manifest = join(here, 'Cargo.toml')
const build = ['build', '--release', '--manifest-path', manifest, '--target-dir', targetDir]
run('cargo', [...build, '--bin', 'streaming-native'])
run('rustup', ['target', 'add', WASM_TARGET])
run('cargo', [...build, '--lib', '--target', WASM_TARGET])

const node = [`--max-old-space-size=${NODE_HEAP_MB}`]
const native = runPasses(
  join(targetDir, 'release', `streaming-native${process.platform === 'win32' ? '.exe' : ''}`),
  [rows, slice],
)
const wasm = runPasses(process.execPath, [
  ...node,
  join(here, 'wasm.mjs'),
  join(targetDir, WASM_TARGET, 'release', 'streaming_bench.wasm'),
  rows,
  slice,
])
const post = runSide(process.execPath, [...node, join(here, 'post.mjs'), rows])

const machine = {
  cpu: os.cpus()[0]?.model.trim() ?? 'unknown',
  logical_cores: os.cpus().length,
  memory_gb: Math.round(os.totalmem() / 1e8) / 10,
  os: `${os.type()} ${os.release()}`,
  node: process.version,
  rustc: execFileSync('rustc', ['--version'], { cwd: root, encoding: 'utf8' }).trim(),
}
const checked = [native, wasm]
  .flatMap((side) => [...side.profiles, ...side.engine])
  .every((profile) => profile.checked)
const results = join('target', 'streaming-bench', `results-${rows}.json`)
mkdirSync(targetDir, { recursive: true })
writeFileSync(
  join(root, results),
  `${JSON.stringify({ machine, passes, checked, native, wasm, post }, null, 2)}\n`,
)

const lines = [
  `Streaming of the rows to the Pivoteur: ${Number(rows).toLocaleString('en')} rows by slices of ${Number(slice).toLocaleString('en')}, fastest of ${passes} passes`,
  `${machine.cpu}, ${machine.logical_cores} logical cores, ${machine.memory_gb} GB, ${machine.os}, Node ${machine.node}, ${machine.rustc}`,
  '',
  ...engineLines('Native, one thread', native),
  '',
  ...engineLines('WebAssembly (V8), one thread', wasm),
  '',
  ...streamedLines(native, wasm),
  '',
  'Main thread: sending the rows to a Worker (structured clone, best of 3)',
  '',
  '| Rows per block | Calls on the main thread | Until the Worker has every row |',
  '| ---: | ---: | ---: |',
  ...post.blocks.map(
    (block) =>
      `| ${block.block.toLocaleString('en')} | ${ms(block.main_s)} | ${ms(block.received_s)} |`,
  ),
  '',
  checked
    ? 'The prototype checks itself on both sides: same states slice by slice as in one pass, same finest groups as the engine of Prisme; the streamed engine gives the table of all the rows.'
    : 'The prototype or the streamed engine does NOT check itself on one side at least.',
  `Full results: ${results}`,
]
console.log(lines.join('\n'))
process.exit(checked ? 0 : 1)
