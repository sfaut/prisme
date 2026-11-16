/**
 * Benchmark of the streaming of the rows to the Pivoteur, main thread: what sending the rows of a
 * reception to a Worker costs the main thread of the page, block by block, as they arrive.
 *
 * The page holds the rows as arrays of text cells (`Cell[][]`, `querySession.ts`); sending a block
 * to a Worker (`postMessage`) copies it by structured clone, on the main thread, before the call
 * returns. Node runs the same serializer as Chrome (that of V8), between `worker_threads`. The
 * rows are those of `bench/pivot` (`../pivot/js/common.mjs`): 10 text columns.
 *
 * Each size of block is measured: the time of the calls on the main thread (the slicing of the
 * block included), and the time until the Worker has received the last block.
 *
 * `node post.mjs [rows]`, 1,000,000 rows by default. The results go to stdout, as one JSON object.
 */

import { performance } from 'node:perf_hooks'
import { Worker } from 'node:worker_threads'

import { generate } from '../pivot/js/common.mjs'

/**
 * Sizes of the blocks sent: about one flush of the page (applied every 100 ms, the server
 * sending 100,000 rows per second or so), and the blocks of today's copy (`BLOCK_ROWS` of
 * `pivotClient.ts`).
 */
const BLOCKS = [10_000, 20_000]

/**
 * Passes per size of block; the fastest is kept.
 */
const PASSES = 3

const count = Number(process.argv[2] ?? 1_000_000)

/**
 * Worker that counts the rows it receives, and answers once it has them all.
 */
const WORKER = `
const { parentPort } = require('node:worker_threads')
let received = 0
let expected = 0
parentPort.on('message', (message) => {
  if (message.type === 'begin') {
    received = 0
    expected = message.count
  } else {
    received += message.rows.length
  }
  if (received === expected) parentPort.postMessage('done')
})
`

/**
 * Sends all the rows to the Worker by blocks of `block` rows.
 *
 * @param {Worker} worker Worker.
 * @param {string[][]} rows Rows.
 * @param {number} block Rows per block.
 * @returns {Promise<{ mainS: number, receivedS: number }>}
 *     Time of the calls on the main thread, and time until the Worker has every row, in seconds.
 */
async function send(worker, rows, block) {
  const done = new Promise((resolve) => worker.once('message', resolve))
  worker.postMessage({ type: 'begin', count: rows.length })
  const started = performance.now()
  let main = 0
  for (let start = 0; start < rows.length; start += block) {
    const call = performance.now()
    worker.postMessage({ type: 'rows', rows: rows.slice(start, start + block) })
    main += performance.now() - call
  }
  await done
  return { mainS: main / 1000, receivedS: (performance.now() - started) / 1000 }
}

console.error(`post: generating ${count} rows`)
const rows = generate(count)
const worker = new Worker(WORKER, { eval: true })
const results = []
for (const block of BLOCKS) {
  console.error(`post: blocks of ${block} rows`)
  let best = null
  for (let pass = 0; pass < PASSES; pass++) {
    const measured = await send(worker, rows, block)
    if (best === null || measured.mainS < best.mainS) {
      best = measured
    }
  }
  results.push({
    block,
    main_s: Math.round(best.mainS * 1000) / 1000,
    received_s: Math.round(best.receivedS * 1000) / 1000,
  })
}
await worker.terminate()
console.log(JSON.stringify({ rows: count, blocks: results }))
