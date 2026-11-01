/**
 * Benchmark of `sqlparser` compiled to WebAssembly (`docs/PERFORMANCES.md`, chapter 13), one
 * module: compiles and instantiates it under Node (V8, the engine of Chrome), then times the parse
 * of expressions such as the Pivoteur writes them.
 *
 * Each module runs in a process of its own, so that its first compilation and its first parse
 * start cold: V8 compiles the functions of a module lazily, on their first call
 * (`--wasm-lazy-compilation`, on by default in Node 26.7.0), and a module compiled again from the
 * same bytes in the same process reuses the functions already compiled
 * (`--wasm-native-module-cache`, on by default): its first parse takes 0.1 to 0.4 ms, against 11 to
 * 15 ms for the first module (chapter 13). The command measures this second module as well.
 *
 * `node measure.mjs <module.wasm>`. The results go to stdout, as one JSON object.
 */

import { readFileSync } from 'node:fs'
import { basename } from 'node:path'
import { performance } from 'node:perf_hooks'

/**
 * Rounds of parses of each expression; the best one is kept.
 */
const ROUNDS = 5

/**
 * Parses of an expression in a round, and in the warm-up before the rounds.
 */
const PARSES = 1_000

/**
 * Terms of the long expression: a sum of many columns, to show how the time grows with the text.
 */
const LONG_TERMS = 200

/**
 * @typedef {object} Expression
 * @property {string} name Name of the expression in the results.
 * @property {string} text Text parsed.
 * @property {boolean} valid Whether the dialect of the Pivoteur accepts it.
 */

/**
 * Expressions parsed: virtual columns and measures of the specification of the Pivoteur, then
 * two texts that must be refused.
 *
 * @type {Expression[]}
 */
const EXPRESSIONS = [
  { name: 'sum', text: 'SUM(montant)', valid: true },
  { name: 'ratio', text: 'SUM(montant) / NULLIF(SUM(qte), 0)', valid: true },
  { name: 'quarter', text: "CONCAT(EXTRACT(YEAR FROM d), '-T', QUARTER(d))", valid: true },
  {
    name: 'case',
    text: "CASE WHEN qte >= 10 THEN 'gros' WHEN qte > 0 THEN 'petit' ELSE 'aucun' END",
    valid: true,
  },
  { name: 'filter', text: "COUNT(*) FILTER (WHERE canal = 'web')", valid: true },
  { name: 'listagg', text: "LISTAGG(produit, ', ') WITHIN GROUP (ORDER BY produit)", valid: true },
  { name: 'cast', text: 'CAST(d AS DATE)', valid: true },
  {
    name: `sum of ${LONG_TERMS} columns`,
    text: Array.from({ length: LONG_TERMS }, (_, index) => `c${index + 1}`).join(' + '),
    valid: true,
  },
  { name: 'unclosed', text: 'SUM(montant', valid: false },
  { name: 'trailing token', text: 'SUM(montant) montant', valid: false },
]

const [modulePath] = process.argv.slice(2)
const bytes = readFileSync(modulePath)

let started = performance.now()
const module = await WebAssembly.compile(bytes)
const compileMs = performance.now() - started
started = performance.now()
const instance = await WebAssembly.instantiate(module, {})
const instantiateMs = performance.now() - started
const parser = instance.exports

const encoder = new TextEncoder()

/**
 * Copies a text into the memory of a module, where it stays for every parse.
 *
 * @param {WebAssembly.Exports} exports Functions and memory of the module.
 * @param {string} text Text of an expression.
 * @returns {{ pointer: number, length: number }} Its address and its length in bytes.
 */
function write(exports, text) {
  const encoded = encoder.encode(text)
  const pointer = exports.reserve(encoded.length)
  // The memory may grow in `reserve`, which replaces its buffer: it is read after.
  new Uint8Array(exports.memory.buffer, pointer, encoded.length).set(encoded)
  return { pointer, length: encoded.length }
}

/**
 * Duration of the first parse of the first expression in a module, in milliseconds.
 *
 * @param {WebAssembly.Exports} exports Functions and memory of the module.
 */
function firstParse(exports) {
  const { pointer, length } = write(exports, EXPRESSIONS[0].text)
  const parseStarted = performance.now()
  exports.parse(pointer, length)
  return performance.now() - parseStarted
}

const firstParseMs = firstParse(parser)
const again = await WebAssembly.instantiate(await WebAssembly.compile(bytes), {})
const secondModuleFirstParseMs = firstParse(again.exports)

const written = EXPRESSIONS.map((expression) => ({
  ...expression,
  ...write(parser, expression.text),
}))

const expressions = written.map(({ name, text, valid, pointer, length }) => {
  let parsed = 0
  for (let index = 0; index < PARSES; index++) {
    parsed = parser.parse(pointer, length)
  }
  let best = Infinity
  for (let round = 0; round < ROUNDS; round++) {
    const roundStarted = performance.now()
    for (let index = 0; index < PARSES; index++) {
      parser.parse(pointer, length)
    }
    best = Math.min(best, (performance.now() - roundStarted) / PARSES)
  }
  return { name, text, bytes: length, valid, parsed: parsed === 1, parse_us: best * 1000 }
})

console.log(
  JSON.stringify({
    module: basename(modulePath),
    compile_ms: compileMs,
    instantiate_ms: instantiateMs,
    first_parse_ms: firstParseMs,
    second_module_first_parse_ms: secondModuleFirstParseMs,
    memory_mb: parser.memory.buffer.byteLength / 1e6,
    rounds: ROUNDS,
    parses: PARSES,
    expressions,
  }),
)
