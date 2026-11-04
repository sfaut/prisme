/**
 * Benchmark of the saved results: what a result of many rows costs on its way into a result tab,
 * and when the page reads it again, on the real server.
 *
 * Builds Prisme, starts it on a throwaway data folder, saves a connection to the latest MariaDB
 * LTS of the development servers (`compose.yaml`) and an editor, then, for each number of rows:
 *
 * 1. runs a query of that many rows in the session of the editor, as the page does: the rows
 *    arrive in batches, the server writes them to the internal database in chunks as they
 *    arrive, then writes the last chunk and the state of the result before announcing its end
 *    (`done`, see `crates/server/src/recorder.rs`);
 * 2. reads the saved tabs again, as the page does on opening: their list
 *    (`GET /api/editors/{id}/results`), then the chunks of their rows one by one
 *    (`GET /api/results/{id}/chunks/{position}`), each one parsed;
 * 3. reattaches a WebSocket to the session, as a page reloaded after the end of the query: the
 *    last batch (`resume`), then its rows (`resumed_rows`), read again from their chunks.
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
 * received, then the end of the save of the result, which the server ends before sending `done`.
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
 * Reads the saved tabs of the editor again, as the page does on opening: their list, then the
 * chunks of their rows, one after the other, each one downloaded then parsed.
 *
 * @param {number} editor Id of the editor.
 */
async function reload(editor) {
  const started = performance.now()
  const response = await fetch(`http://${ADDRESS}/api/editors/${editor}/results`)
  const tabs = await response.json()
  const listed = performance.now()
  const result = {
    status: response.status,
    list_s: (listed - started) / 1000,
    chunks: 0,
    bytes: 0,
    download_s: 0,
    parse_s: 0,
    rows: 0,
    largest_chunk_bytes: 0,
  }
  for (const tab of tabs) {
    for (let position = 0; position < tab.chunks; position++) {
      const asked = performance.now()
      const chunk = await fetch(`http://${ADDRESS}/api/results/${tab.id}/chunks/${position}`)
      const body = await chunk.arrayBuffer()
      const downloaded = performance.now()
      const rows = JSON.parse(new TextDecoder().decode(body))
      result.parse_s += (performance.now() - downloaded) / 1000
      result.download_s += (downloaded - asked) / 1000
      result.chunks++
      result.bytes += body.byteLength
      result.largest_chunk_bytes = Math.max(result.largest_chunk_bytes, body.byteLength)
      result.rows += rows.length
    }
  }
  result.total_s = (performance.now() - started) / 1000
  return result
}

/**
 * Reattaches a WebSocket to the session of the editor, as a page reloaded after the end of the
 * query, and measures the reception of the last batch and of its rows.
 *
 * @param {number} editor Id of the editor.
 */
function resume(editor) {
  return new Promise((done, fail) => {
    const started = performance.now()
    const socket = new WebSocket(`ws://${ADDRESS}/api/editors/${editor}/session`)
    let expected = null
    let rows = 0
    let messages = 0
    socket.addEventListener('error', () => fail(new Error('WebSocket error')))
    socket.addEventListener('close', () => {
      if (expected === null || rows < expected) {
        fail(new Error(`WebSocket closed after ${rows} of ${expected} rows`))
      }
    })
    const end = () => {
      socket.close()
      done({ rows, messages, total_s: (performance.now() - started) / 1000 })
    }
    socket.addEventListener('message', (event) => {
      const message = JSON.parse(event.data)
      switch (message.type) {
        case 'resume':
          expected = message.queries.reduce((total, query) => total + query.resumed_rows, 0)
          if (expected === 0) {
            end()
          }
          break
        case 'resumed_rows':
          rows += message.rows.length
          messages++
          if (rows >= expected) {
            end()
          }
          break
      }
    })
  })
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
    // A failed save leaves the tab without some rows, and only a warning in the log.
    const log = prisme.log()
    entry.query.saved = !log.includes('result not saved') && !log.includes('rows not saved')
    console.error(`reload: ${count} rows, reading the saved tabs again`)
    entry.reload = await reload(editor.id)
    const answer = plain(prisme.log()).match(
      new RegExp(`GET /api/editors/${editor.id}/results \\d+ (\\d+) ms`),
    )
    entry.reload.list_server_s = answer === null ? null : Number(answer[1]) / 1000
    console.error(`reload: ${count} rows, reattaching to the session`)
    entry.resume = await resume(editor.id)
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
