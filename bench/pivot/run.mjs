/**
 * Benchmark of the Pivoteur's engine (point A1 of its specification): runs the Rust side and the
 * JavaScript side on the same generated rows, checks that both give the same pivot, and prints
 * a summary.
 *
 * From the root of the repository: `node bench/pivot/run.mjs [rows]` (10,000,000 rows by
 * default). Requires Cargo (the toolchain of `rust-toolchain.toml`) and Node; nothing to install.
 * The full results, with the machine, go to `target/pivot-bench/results-<rows>.json`. The exit code is
 * 1 if the two sides disagree.
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

const here = dirname(fileURLToPath(import.meta.url))
const root = resolve(here, '../..')
const targetDir = join(root, 'target', 'pivot-bench')
const rows = process.argv[2] ?? '10000000'

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
 * Differences between the pivots computed by both sides, empty if they agree.
 *
 * @param {object} rust Results of the Rust side.
 * @param {object} javascript Results of the JavaScript side.
 */
function differences(rust, javascript) {
  const found = []
  rust.profiles.forEach((profile, index) => {
    const other = javascript.profiles[index]
    for (const key of ['groups', 'set_sizes', 'checksum', 'float_bits', 'grand_total']) {
      if (JSON.stringify(profile[key]) !== JSON.stringify(other[key])) {
        found.push(
          `${profile.name}.${key}: ${JSON.stringify(profile[key])} / ${JSON.stringify(other[key])}`,
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

const build = spawnSync(
  'cargo',
  [
    'build',
    '--release',
    '--manifest-path',
    join(here, 'rust', 'Cargo.toml'),
    '--target-dir',
    targetDir,
  ],
  { cwd: root, stdio: 'inherit' },
)
if (build.status !== 0) {
  process.exit(build.status ?? 1)
}
const executable = join(
  targetDir,
  'release',
  `pivot-bench${process.platform === 'win32' ? '.exe' : ''}`,
)

const rust = runSide(executable, [rows])
const javascript = runSide(process.execPath, [
  `--max-old-space-size=${NODE_HEAP_MB}`,
  '--expose-gc',
  join(here, 'js', 'bench.mjs'),
  rows,
])

const machine = {
  cpu: os.cpus()[0]?.model.trim() ?? 'unknown',
  logical_cores: os.cpus().length,
  memory_gb: Math.round(os.totalmem() / 1e8) / 10,
  os: `${os.type()} ${os.release()}`,
  node: process.version,
  rustc: execFileSync('rustc', ['--version'], { cwd: root, encoding: 'utf8' }).trim(),
}
const found = differences(rust, javascript)
mkdirSync(targetDir, { recursive: true })
writeFileSync(
  join(targetDir, `results-${rows}.json`),
  `${JSON.stringify({ machine, identical: found.length === 0, rust, javascript }, null, 2)}\n`,
)

const [rustHigh, rustLow] = rust.profiles
const [javascriptHigh, javascriptLow] = javascript.profiles
const memory = (side, key) => side.memory_gb[key].toFixed(2)
const lines = [
  `Pivoteur engine benchmark: ${Number(rows).toLocaleString('en')} rows`,
  `${machine.cpu}, ${machine.logical_cores} logical cores, ${machine.memory_gb} GB, ${machine.os}, Node ${machine.node}, ${machine.rustc}`,
  '',
  '| Step | Rust, one thread | Rust, all threads | JavaScript (V8) |',
  '| --- | ---: | ---: | ---: |',
  `| Preparation, first pass (s) | ${seconds(rust.preparation.one_thread_s)} | ${seconds(rust.preparation.all_threads_s)} | ${seconds(javascript.preparation.one_thread_s)} |`,
  `| Pivot "high", best of 3 (s) | ${seconds(rustHigh.one_thread.total_s)} | ${seconds(rustHigh.all_threads.total_s)} | ${seconds(javascriptHigh.one_thread.total_s)} |`,
  `| levels / groups / sets / measures (s) | ${phases(rustHigh.one_thread)} | ${phases(rustHigh.all_threads)} | ${phases(javascriptHigh.one_thread)} |`,
  `| Pivot "low", best of 3 (s) | ${seconds(rustLow.one_thread.total_s)} | ${seconds(rustLow.all_threads.total_s)} | ${seconds(javascriptLow.one_thread.total_s)} |`,
  `| levels / groups / sets / measures (s) | ${phases(rustLow.one_thread)} | ${phases(rustLow.all_threads)} | ${phases(javascriptLow.one_thread)} |`,
  `| Groups "high" / "low" | ${rustHigh.groups} / ${rustLow.groups} | | ${javascriptHigh.groups} / ${javascriptLow.groups} |`,
  `| Memory after generation (GB) | ${memory(rust, 'after_generation')} | | ${memory(javascript, 'after_generation')} (heap ${memory(javascript, 'heap_after_generation')}) |`,
  `| Memory after preparation (GB) | ${memory(rust, 'after_preparation')} | | ${memory(javascript, 'after_preparation')} (heap ${memory(javascript, 'heap_after_preparation')}) |`,
  `| Peak memory (GB) | ${memory(rust, 'peak')} | | ${memory(javascript, 'peak')} |`,
  `| JSON of the whole pivot "high" / "low" (MB, s) | ${(rustHigh.serialization.bytes / 1e6).toFixed(0)} MB, ${seconds(rustHigh.serialization.seconds)} / ${(rustLow.serialization.bytes / 1e6).toFixed(0)} MB, ${seconds(rustLow.serialization.seconds)} | | |`,
  '',
  'Memory: private bytes for Rust, resident set for JavaScript (heap: V8 objects and texts).',
  found.length === 0
    ? 'Same pivot on both sides: groups, sizes of the grouping sets, checksums, grand total.'
    : `The two sides disagree:\n${found.join('\n')}`,
  `Full results: ${join('target', 'pivot-bench', `results-${rows}.json`)}`,
]
console.log(lines.join('\n'))
process.exit(found.length === 0 ? 0 : 1)
