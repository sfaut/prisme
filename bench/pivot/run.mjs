/**
 * Benchmark of the Pivoteur's engine (point A1 of its specification): runs the native side (Rust,
 * in the server), the WebAssembly side (the same Rust, in the page) and the JavaScript side (what
 * the TypeScript of the page becomes) on the same generated rows, checks that they give the same
 * pivot, and prints a summary.
 *
 * From the root of the repository: `node bench/pivot/run.mjs [rows]` (10,000,000 rows by
 * default). Requires Cargo (the toolchain of `rust-toolchain.toml`) and Node; the WebAssembly
 * target of the toolchain is added when missing. The full results, with the machine, go to
 * `target/pivot-bench/results-<rows>.json`. The exit code is 1 if the sides disagree.
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
 * Target of the WebAssembly side, with its default features: SIMD (`simd128`) gave no measurable
 * gain on this engine.
 */
const WASM_TARGET = 'wasm32-unknown-unknown'

const here = dirname(fileURLToPath(import.meta.url))
const root = resolve(here, '../..')
const targetDir = join(root, 'target', 'pivot-bench')
const rows = process.argv[2] ?? '10000000'

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
 * Differences between the pivots computed by the native side and another one, empty if they
 * agree.
 *
 * @param {object} native Results of the native side.
 * @param {object} other Results of the other side.
 */
function differences(native, other) {
  const found = []
  native.profiles.forEach((profile, index) => {
    const theirs = other.profiles[index]
    for (const key of ['groups', 'set_sizes', 'checksum', 'float_bits', 'grand_total']) {
      if (JSON.stringify(profile[key]) !== JSON.stringify(theirs[key])) {
        found.push(
          `${other.engine} ${profile.name}.${key}: ${JSON.stringify(profile[key])} / ${JSON.stringify(theirs[key])}`,
        )
      }
    }
  })
  return found
}

/**
 * Duration in seconds, with 3 digits, or a dash.
 *
 * @param {number | null | undefined} seconds Duration.
 */
function seconds(seconds) {
  return seconds === null || seconds === undefined ? '–' : seconds.toFixed(3)
}

/**
 * Cells of a row of the summary for the durations of a pivot.
 *
 * @param {object | null} timings Durations of the computation.
 */
function phases(timings) {
  if (timings === null) {
    return '–'
  }
  const { levels_s, groups_s, sets_s, measures_s } = timings
  return [levels_s, groups_s, sets_s, measures_s].map(seconds).join(' / ')
}

/**
 * Memory of a side in the summary, in gigabytes, with the heap of V8 and the memory of the
 * module when the side has them.
 *
 * @param {object} side Results of a side.
 * @param {string} moment `after_generation`, `after_preparation` or `peak`.
 */
function memory(side, moment) {
  const values = side.memory_gb
  const parts = []
  if (values[`heap_${moment}`] !== undefined) {
    parts.push(`heap ${values[`heap_${moment}`].toFixed(2)}`)
  }
  const module = moment === 'peak' ? values.module_peak : values[`module_${moment}`]
  if (module !== undefined) {
    parts.push(`module ${module.toFixed(2)}`)
  }
  return `${values[moment].toFixed(2)}${parts.length > 0 ? ` (${parts.join(', ')})` : ''}`
}

const manifest = join(here, 'Cargo.toml')
const build = ['build', '--release', '--manifest-path', manifest, '--target-dir', targetDir]
run('cargo', [...build, '--package', 'pivot-native'])
run('rustup', ['target', 'add', WASM_TARGET])
run('cargo', [...build, '--package', 'pivot-wasm', '--target', WASM_TARGET])

const node = [`--max-old-space-size=${NODE_HEAP_MB}`, '--expose-gc']
const native = runSide(
  join(targetDir, 'release', `pivot-native${process.platform === 'win32' ? '.exe' : ''}`),
  [rows],
)
const wasm = runSide(process.execPath, [
  ...node,
  join(here, 'js', 'wasm.mjs'),
  join(targetDir, WASM_TARGET, 'release', 'pivot_wasm.wasm'),
  rows,
])
const javascript = runSide(process.execPath, [...node, join(here, 'js', 'bench.mjs'), rows])

const machine = {
  cpu: os.cpus()[0]?.model.trim() ?? 'unknown',
  logical_cores: os.cpus().length,
  memory_gb: Math.round(os.totalmem() / 1e8) / 10,
  os: `${os.type()} ${os.release()}`,
  node: process.version,
  rustc: execFileSync('rustc', ['--version'], { cwd: root, encoding: 'utf8' }).trim(),
}
const found = [...differences(native, wasm), ...differences(native, javascript)]
const results = join('target', 'pivot-bench', `results-${rows}.json`)
mkdirSync(targetDir, { recursive: true })
writeFileSync(
  join(root, results),
  `${JSON.stringify({ machine, identical: found.length === 0, native, wasm, javascript }, null, 2)}\n`,
)

const [nativeHigh, nativeLow] = native.profiles
const [wasmHigh, wasmLow] = wasm.profiles
const [javascriptHigh, javascriptLow] = javascript.profiles
const json = (profile) =>
  `${(profile.serialization.bytes / 1e6).toFixed(0)} MB, ${seconds(profile.serialization.seconds)}`
const lines = [
  `Pivoteur engine benchmark: ${Number(rows).toLocaleString('en')} rows`,
  `${machine.cpu}, ${machine.logical_cores} logical cores, ${machine.memory_gb} GB, ${machine.os}, Node ${machine.node}, ${machine.rustc}`,
  '',
  '| Step | Native, one thread | Native, all threads | WebAssembly (V8) | JavaScript (V8) |',
  '| --- | ---: | ---: | ---: | ---: |',
  `| Copy of the cells into the module (s) | | | ${seconds(wasm.copy_s)} | |`,
  `| Preparation, first pass (s) | ${seconds(native.preparation.one_thread_s)} | ${seconds(native.preparation.all_threads_s)} | ${seconds(wasm.preparation.one_thread_s)} | ${seconds(javascript.preparation.one_thread_s)} |`,
  `| Pivot "high", best of 3 (s) | ${seconds(nativeHigh.one_thread.total_s)} | ${seconds(nativeHigh.all_threads.total_s)} | ${seconds(wasmHigh.one_thread.total_s)} | ${seconds(javascriptHigh.one_thread.total_s)} |`,
  `| levels / groups / sets / measures (s) | ${phases(nativeHigh.one_thread)} | ${phases(nativeHigh.all_threads)} | ${phases(wasmHigh.one_thread)} | ${phases(javascriptHigh.one_thread)} |`,
  `| Pivot "low", best of 3 (s) | ${seconds(nativeLow.one_thread.total_s)} | ${seconds(nativeLow.all_threads.total_s)} | ${seconds(wasmLow.one_thread.total_s)} | ${seconds(javascriptLow.one_thread.total_s)} |`,
  `| levels / groups / sets / measures (s) | ${phases(nativeLow.one_thread)} | ${phases(nativeLow.all_threads)} | ${phases(wasmLow.one_thread)} | ${phases(javascriptLow.one_thread)} |`,
  `| Groups "high" / "low" | ${nativeHigh.groups} / ${nativeLow.groups} | | ${wasmHigh.groups} / ${wasmLow.groups} | ${javascriptHigh.groups} / ${javascriptLow.groups} |`,
  `| Memory after generation (GB) | ${memory(native, 'after_generation')} | | ${memory(wasm, 'after_generation')} | ${memory(javascript, 'after_generation')} |`,
  `| Memory after preparation (GB) | ${memory(native, 'after_preparation')} | | ${memory(wasm, 'after_preparation')} | ${memory(javascript, 'after_preparation')} |`,
  `| Peak memory (GB) | ${memory(native, 'peak')} | | ${memory(wasm, 'peak')} | ${memory(javascript, 'peak')} |`,
  `| JSON of the whole pivot "high" / "low" | ${json(nativeHigh)} / ${json(nativeLow)} | | | |`,
  '',
  'Memory: private bytes for the native side, resident set for the others (heap: V8 objects and',
  'texts; module: memory of the WebAssembly module, which never shrinks).',
  found.length === 0
    ? 'Same pivot on every side: groups, sizes of the grouping sets, checksums, grand total.'
    : `The sides disagree:\n${found.join('\n')}`,
  `Full results: ${results}`,
]
console.log(lines.join('\n'))
process.exit(found.length === 0 ? 0 : 1)
