/**
 * Benchmark of the batches of rows: how the rows of a result go from the server to the page, by
 * which messages, and how soon the page shows them, for results that arrive fast, slowly, or
 * heavy.
 *
 * Builds Prisme (or takes the executable named by `PRISME_EXECUTABLE`), starts it on a throwaway
 * data folder, without its log (`--verbose` saves a line per message in the internal database),
 * with a connection to the latest MariaDB LTS of the development servers (`compose.yaml`). For
 * each scenario, the query runs:
 *
 * 1. in the session of an editor, by a client under Node: time, rows and bytes of each `rows`
 *    message, from the sending of the query;
 * 2. with `--interface`, in the interface of Prisme, in Chrome without a window (`../store/`):
 *    the same messages as the page receives them, the first row drawn in the grid, the long
 *    tasks of the main thread.
 *
 * From the root of the repository, the development servers started
 * (`docker compose up --detach --wait`) and the interface built (`npm run build` in `web/`):
 * `node bench/batches/run.mjs [--interface] [--label=<name>] [scenario...]` (all the scenarios
 * by default). The results go to `target/batch-bench/<name>.json` (`release` by default).
 */

import { mkdirSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { performance } from 'node:perf_hooks'
import { ADDRESS, ROOT, build, call, query, start } from '../prisme.mjs'
import { evaluate, launch, pause } from '../store/cdp.mjs'
import { mainThread, openInterface, runInInterface } from '../store/interface.mjs'

/**
 * Results, by their kind of arrival. `LIMIT` is written: Prisme's limit does not apply.
 */
const SCENARIOS = {
  // 1 million rows of 10 short columns, as fast as the server reads them (104 MB of JSON).
  fast: query(1_000_000),
  // 600 rows of 1 KB, one every 5 ms: MariaDB sends them as its network buffer fills.
  slow: "SELECT seq, REPEAT('x', 1000) AS filler, SLEEP(0.005) AS pause FROM seq_1_to_600 LIMIT 600",
  // 600 rows of a few bytes, one every 5 ms.
  'slow-small': 'SELECT seq, SLEEP(0.005) AS pause FROM seq_1_to_600 LIMIT 600',
  // 2,000 rows of 200 bytes found along a scan of 50 million rows.
  rare: "SELECT seq, REPEAT('x', 200) AS filler FROM seq_1_to_50000000 WHERE seq % 25000 = 0 LIMIT 2000",
  // 2,000 rows of 100 KB (200 MB).
  heavy: 'SELECT seq, REPEAT(CHAR(65 + seq % 26), 100000) AS doc FROM seq_1_to_2000 LIMIT 2000',
}

/**
 * Time left to the interface after `done`, to apply its last messages and draw.
 */
const SETTLE_MS = 2_000

const workDir = join(ROOT, 'target', 'batch-bench')
const withInterface = process.argv.includes('--interface')
const label =
  process.argv.find((arg) => arg.startsWith('--label='))?.slice('--label='.length) ?? 'release'
const names = process.argv.slice(2).filter((arg) => !arg.startsWith('--'))
const chosen = names.length > 0 ? names : Object.keys(SCENARIOS)

/**
 * Summary of the `rows` messages of a result: their number, their rows and bytes (median and
 * largest), the first and the last, and the longest silence between two of them.
 *
 * @param {[number, number, number][]} messages Time (ms from the sending), rows, bytes.
 */
function summary(messages) {
  const sorted = (values) => [...values].sort((a, b) => a - b)
  const median = (values) => sorted(values)[Math.floor(values.length / 2)] ?? 0
  const times = messages.map(([time]) => time)
  const gaps = times.slice(1).map((time, index) => time - times[index])
  return {
    messages: messages.length,
    rows: messages.reduce((total, [, rows]) => total + rows, 0),
    rows_median: median(messages.map(([, rows]) => rows)),
    rows_largest: Math.max(0, ...messages.map(([, rows]) => rows)),
    kb_median: Math.round(median(messages.map(([, , bytes]) => bytes)) / 1000),
    kb_largest: Math.round(Math.max(0, ...messages.map(([, , bytes]) => bytes)) / 1000),
    first_ms: Math.round(times[0] ?? 0),
    last_ms: Math.round(times.at(-1) ?? 0),
    longest_silence_ms: Math.round(Math.max(0, ...gaps)),
  }
}

/**
 * Runs the query in the session of an editor, as the page does, and notes each `rows` message.
 * The editor is closed afterwards: the interface opens on the editor of its own query.
 *
 * @param {number} connection Id of the connection.
 * @param {string} sql Query.
 */
async function inNode(connection, sql) {
  const editor = await call('POST', '/editors', { connection_id: connection })
  return new Promise((done, fail) => {
    const socket = new WebSocket(`ws://${ADDRESS}/api/editors/${editor.id}/session`)
    const messages = []
    let sent = 0
    socket.addEventListener('error', () => fail(new Error('WebSocket error')))
    socket.addEventListener('message', (event) => {
      const now = performance.now()
      const message = JSON.parse(event.data)
      switch (message.type) {
        case 'ready':
          sent = performance.now()
          socket.send(JSON.stringify({ type: 'run', sql }))
          break
        case 'rows':
          messages.push([now - sent, message.rows.length, event.data.length])
          break
        case 'error':
          socket.close()
          fail(new Error(message.message))
          break
        case 'done':
          socket.close()
          done({ ...summary(messages), done_ms: Math.round(now - sent), messages_list: messages })
          break
      }
    })
  }).finally(() => call('DELETE', `/editors/${editor.id}`))
}

/**
 * Probe of the batches in the page, installed before its scripts, next to that of `../store/`:
 * notes each `rows` message (time, rows, bytes) and the first row drawn in the grid. The probe is
 * `window.batchProbe`; its times count from the loading of the page, as those of the other one.
 */
function batchProbe() {
  const probe = { messages: [], drawn: null }
  window.batchProbe = probe
  const Native = window.WebSocket
  window.WebSocket = class extends Native {
    constructor(...args) {
      super(...args)
      this.addEventListener('message', (event) => {
        if (typeof event.data === 'string' && event.data.startsWith('{"type":"rows"')) {
          const rows = (event.data.match(/\],\[/g)?.length ?? 0) + 1
          probe.messages.push([performance.now(), rows, event.data.length])
        }
      })
    }
  }
  new MutationObserver(() => {
    if (probe.drawn === null && document.querySelector('.grid-body .grid-row') !== null) {
      probe.drawn = performance.now()
    }
  }).observe(document, { childList: true, subtree: true })
}

/**
 * Runs the query in the interface, on an editor of its own: messages as the page receives them,
 * first row drawn, long tasks and main thread, from Ctrl+Enter to the drawing after `done`.
 *
 * @param {{ send: Function, next: Function }} page Page.
 * @param {number} connection Id of the connection.
 * @param {string} sql Query.
 */
async function inInterface(page, connection, sql) {
  // The interface opens on the editor of the query only: those of the previous scenarios close.
  for (const editor of await call('GET', '/editors')) {
    await call('DELETE', `/editors/${editor.id}`)
  }
  const { identifier } = await page.send('Page.addScriptToEvaluateOnNewDocument', {
    source: `(${batchProbe})()`,
  })
  await openInterface(page, connection, sql)
  await page.send('Page.removeScriptToEvaluateOnNewDocument', { identifier })
  const before = await mainThread(page)
  await runInInterface(page)
  await pause(SETTLE_MS)
  const after = await mainThread(page)
  const probes = await evaluate(page, () => ({
    sent: window.benchProbe.sent,
    done: window.benchProbe.done,
    longTasks: window.benchProbe.longTasks,
    messages: window.batchProbe.messages,
    drawn: window.batchProbe.drawn,
  }))
  const from = (time) => (time === null ? null : Math.round(time - probes.sent))
  const tasks = probes.longTasks.filter(([startTime]) => startTime >= probes.sent)
  return {
    ...summary(probes.messages.map(([time, rows, bytes]) => [time - probes.sent, rows, bytes])),
    drawn_ms: from(probes.drawn),
    done_ms: from(probes.done),
    long_tasks: tasks.length,
    longest_task_ms: Math.round(Math.max(0, ...tasks.map(([, duration]) => duration))),
    main_thread_s: Math.round((after.task_s - before.task_s) * 10) / 10,
  }
}

build()
mkdirSync(workDir, { recursive: true })
const prisme = await start(join(workDir, `data-${label}`), { verbose: false })
const page = withInterface ? await launch(join(workDir, 'chrome')) : null
const results = { label, executable: process.env.PRISME_EXECUTABLE ?? 'release', scenarios: {} }
try {
  for (const name of chosen) {
    const sql = SCENARIOS[name]
    if (sql === undefined) {
      throw new Error(`unknown scenario: ${name}`)
    }
    console.error(`batches: ${label}, ${name}, Node`)
    const entry = { node: await inNode(prisme.connection, sql) }
    if (page !== null) {
      console.error(`batches: ${label}, ${name}, interface`)
      entry.interface = await inInterface(page, prisme.connection, sql)
    }
    results.scenarios[name] = entry
    const { messages_list: _, ...node } = entry.node
    console.log(JSON.stringify({ label, scenario: name, node, interface: entry.interface }))
  }
} finally {
  page?.close()
  prisme.stop()
  writeFileSync(join(workDir, `server-${label}.log`), prisme.log())
}
writeFileSync(join(workDir, `${label}.json`), `${JSON.stringify(results, null, 2)}\n`)
