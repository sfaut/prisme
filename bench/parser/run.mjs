/**
 * Benchmark of `sqlparser` compiled to WebAssembly (`docs/PERFORMANCES.md`, chapter 13): the
 * parser that the Worker of the Pivoteur runs on its expressions. Builds the module of
 * `bench/parser/` twice, in the release profile of Prisme and with the optimizations across
 * crates (`lto` profile), gives the size of each, raw and compressed, then measures each one in a
 * process of its own (`measure.mjs`): compilation, instantiation, first parse, parse of each
 * expression.
 *
 * From the root of the repository: `node bench/parser/run.mjs`. Requires Cargo (the toolchain of
 * `rust-toolchain.toml`) and Node; the WebAssembly target of the toolchain is added when missing.
 * The full results, with the machine, go to `target/parser-bench/results.json`. The exit code is
 * 1 if a module refuses an expression of the Pivoteur or accepts a text it must refuse.
 */

import { execFileSync, spawnSync } from 'node:child_process'
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import os from 'node:os'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { brotliCompressSync, constants, gzipSync } from 'node:zlib'

/**
 * Target of the module, the one of the engine of the Pivoteur.
 */
const WASM_TARGET = 'wasm32-unknown-unknown'

/**
 * Cargo profiles built: the one of Prisme, and the one optimized across crates.
 */
const PROFILES = ['release', 'lto']

const here = dirname(fileURLToPath(import.meta.url))
const root = resolve(here, '../..')
const targetDir = join(root, 'target', 'parser-bench')

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
 * Measures a module in a process of its own.
 *
 * @param {string} modulePath Path of the module.
 * @returns {object} Results printed by `measure.mjs`.
 */
function measure(modulePath) {
  const side = spawnSync(process.execPath, [join(here, 'measure.mjs'), modulePath], {
    cwd: root,
    stdio: ['ignore', 'pipe', 'inherit'],
    encoding: 'utf8',
  })
  if (side.status !== 0) {
    console.error(`measure.mjs failed (exit code ${side.status})`)
    process.exit(side.status ?? 1)
  }
  return JSON.parse(side.stdout)
}

/**
 * Size in kilobytes, with one decimal.
 *
 * @param {number} bytes Size in bytes.
 */
function kilobytes(bytes) {
  return `${(bytes / 1000).toFixed(1)} kB`
}

run('rustup', ['target', 'add', WASM_TARGET])
const modules = PROFILES.map((profile) => {
  run('cargo', [
    'build',
    '--manifest-path',
    join(here, 'Cargo.toml'),
    '--target-dir',
    targetDir,
    '--target',
    WASM_TARGET,
    '--profile',
    profile,
  ])
  const path = join(targetDir, WASM_TARGET, profile, 'parser_wasm.wasm')
  const bytes = readFileSync(path)
  return {
    profile,
    bytes: bytes.length,
    gzip_bytes: gzipSync(bytes, { level: 9 }).length,
    brotli_bytes: brotliCompressSync(bytes, {
      params: { [constants.BROTLI_PARAM_QUALITY]: 11 },
    }).length,
    ...measure(path),
  }
})

const machine = {
  cpu: os.cpus()[0]?.model.trim() ?? 'unknown',
  logical_cores: os.cpus().length,
  memory_gb: Math.round(os.totalmem() / 1e8) / 10,
  os: `${os.type()} ${os.release()}`,
  node: process.version,
  rustc: execFileSync('rustc', ['--version'], { cwd: root, encoding: 'utf8' }).trim(),
}
const wrong = modules.flatMap((module) =>
  module.expressions
    .filter((expression) => expression.parsed !== expression.valid)
    .map(
      (expression) =>
        `${module.profile}: "${expression.text}" ${expression.parsed ? 'accepted' : 'refused'}`,
    ),
)
const results = join('target', 'parser-bench', 'results.json')
mkdirSync(targetDir, { recursive: true })
writeFileSync(
  join(root, results),
  `${JSON.stringify({ machine, correct: wrong.length === 0, modules }, null, 2)}\n`,
)

const [first] = modules
const lines = [
  'sqlparser compiled to WebAssembly',
  `${machine.cpu}, ${machine.logical_cores} logical cores, ${machine.memory_gb} GB, ${machine.os}, Node ${machine.node}, ${machine.rustc}`,
  '',
  `| Step | ${modules.map((module) => `profile ${module.profile}`).join(' | ')} |`,
  `| --- | ${modules.map(() => '---:').join(' | ')} |`,
  `| Module | ${modules.map((module) => kilobytes(module.bytes)).join(' | ')} |`,
  `| gzip, level 9 | ${modules.map((module) => kilobytes(module.gzip_bytes)).join(' | ')} |`,
  `| Brotli, quality 11 | ${modules.map((module) => kilobytes(module.brotli_bytes)).join(' | ')} |`,
  `| Compilation (ms) | ${modules.map((module) => module.compile_ms.toFixed(1)).join(' | ')} |`,
  `| Instantiation (ms) | ${modules.map((module) => module.instantiate_ms.toFixed(1)).join(' | ')} |`,
  `| First parse, "${first.expressions[0].text}" (ms) | ${modules.map((module) => module.first_parse_ms.toFixed(1)).join(' | ')} |`,
  `| First parse in a module compiled again from the same bytes (ms) | ${modules.map((module) => module.second_module_first_parse_ms.toFixed(2)).join(' | ')} |`,
  `| Memory of the module (MB) | ${modules.map((module) => module.memory_mb.toFixed(1)).join(' | ')} |`,
  ...first.expressions.map(
    (expression, index) =>
      `| Parse of "${expression.name}", ${expression.bytes} bytes (µs) | ${modules.map((module) => module.expressions[index].parse_us.toFixed(1)).join(' | ')} |`,
  ),
  '',
  `Parse: best of ${first.rounds} rounds of ${first.parses} parses, after as many parses of warm-up.`,
  wrong.length === 0
    ? 'Every expression of the Pivoteur is accepted, and the wrong texts are refused.'
    : `Wrong answers:\n${wrong.join('\n')}`,
  `Full results: ${results}`,
]
console.log(lines.join('\n'))
process.exit(wrong.length === 0 ? 0 : 1)
