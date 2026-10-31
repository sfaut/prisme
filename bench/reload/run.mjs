/**
 * Benchmark of the saved results: what a result of many rows costs on its way into a result tab,
 * and when the page reads it again, on the real server.
 *
 * Builds Prisme, starts it on a throwaway data folder, saves a connection to the latest MariaDB
 * LTS of the development servers (`compose.yaml`) and an editor, then, for each number of rows:
 *
 * 1. runs a query of that many rows in the session of the editor, as the page does: the rows
 *    arrive in batches, the server keeps them, then writes the whole result to the internal
 *    database before announcing its end (`done`, see `crates/server/src/recorder.rs`);
 * 2. reads the saved tabs again (`GET /api/editors/{id}/results`), as the page does on opening:
 *    the server reads them from the internal database, the page parses them.
 *
 * From the root of the repository, the development servers started
 * (`docker compose up --detach --wait`) and the interface built (`npm run build` in `web/`):
 * `node bench/reload/run.mjs [rows...]` (1,000,000 then 10,000,000 rows by default). The results
 * go to `target/reload-bench/results.json`.
 */

import { mkdirSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { performance } from 'node:perf_hooks'
import { ADDRESS, ROOT, build, call, peakMemory, plain, query, start } from '../prisme.mjs'

const workDir = join(ROOT, 'target', 'reload-bench')
const counts = process.argv.slice(2).map(Number)
if (counts.length === 0) {
  counts.push(1_000_000, 10_000_000)
}

/**
 * Runs a query of `count` rows in the session of the editor, and measures its way: rows
 * received, then the save of the result, which the server ends before sending `done`.
 *
 * @param {number} editor Id of the editor.
 * @param {number} count Number of rows.
 */
function runQuery(editor, count) {
  return new Promise((done, fail) => {
    const socket = new WebSocket(`ws://${ADDRESS}/api/editors/${editor}/session`)
    let sent = 0
    let firstRows = 0
    let lastRows = 0
    let rows = 0
    socket.addEventListener('error', () => fail(new Error('WebSocket error')))
    socket.addEventListener('message', (event) => {
      const message = JSON.parse(event.data)
      switch (message.type) {
        case 'ready':
          sent = performance.now()
          socket.send(JSON.stringify({ type: 'run', sql: query(count) }))
          break
        case 'rows':
          lastRows = performance.now()
          firstRows ||= lastRows
          rows += message.rows.length
          break
        case 'error':
          socket.close()
          fail(new Error(JSON.stringify(message)))
          break
        case 'done': {
          const end = performance.now()
          socket.close()
          done({
            rows,
            first_rows_s: (firstRows - sent) / 1000,
            rows_s: (lastRows - sent) / 1000,
            save_s: (end - lastRows) / 1000,
          })
          break
        }
      }
    })
  })
}

/**
 * Reads the saved tabs of the editor again, as the page does on opening: the answer of the
 * server, its download, then its parsing.
 *
 * @param {number} editor Id of the editor.
 */
async function reload(editor) {
  const started = performance.now()
  const response = await fetch(`http://${ADDRESS}/api/editors/${editor}/results`)
  const answered = performance.now()
  const body = await response.arrayBuffer()
  const downloaded = performance.now()
  const result = {
    status: response.status,
    bytes: body.byteLength,
    answer_s: (answered - started) / 1000,
    download_s: (downloaded - answered) / 1000,
    parse_s: null,
    parse_error: null,
  }
  try {
    const tabs = JSON.parse(new TextDecoder().decode(body))
    result.parse_s = (performance.now() - downloaded) / 1000
    result.rows = tabs.reduce((total, tab) => total + tab.rows.length, 0)
  } catch (error) {
    result.parse_error = String(error)
  }
  return result
}

build()
const results = []
for (const count of counts) {
  const entry = { rows: count }
  let prisme = null
  try {
    prisme = await start(join(workDir, `data-${count}`))
    const editor = await call('POST', '/editors', { connection_id: prisme.connection })
    console.error(`reload: ${count} rows, query`)
    entry.query = await runQuery(editor.id, count)
    entry.server_peak_gb_after_query = peakMemory(prisme.pid)
    // A failed save leaves the tab without rows, and only a warning in the log.
    entry.query.saved = !prisme.log().includes('result not saved')
    console.error(`reload: ${count} rows, reading the saved tabs again`)
    entry.reload = await reload(editor.id)
    const answer = plain(prisme.log()).match(
      new RegExp(`GET /api/editors/${editor.id}/results \\d+ (\\d+) ms`),
    )
    entry.reload.server_s = answer === null ? null : Number(answer[1]) / 1000
    entry.server_peak_gb = peakMemory(prisme.pid)
  } catch (error) {
    entry.error = String(error)
  }
  if (prisme !== null) {
    prisme.stop()
    writeFileSync(join(workDir, `server-${count}.log`), prisme.log())
  }
  results.push(entry)
  console.log(JSON.stringify(entry))
}

mkdirSync(workDir, { recursive: true })
writeFileSync(join(workDir, 'results.json'), `${JSON.stringify(results, null, 2)}\n`)
