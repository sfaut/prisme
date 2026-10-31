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

import { spawn, spawnSync } from 'node:child_process'
import { existsSync, mkdirSync, rmSync, writeFileSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { performance } from 'node:perf_hooks'
import { fileURLToPath } from 'node:url'

/**
 * Address of the throwaway server, away from the port of the usual one.
 */
const ADDRESS = '127.0.0.1:18484'

/**
 * Port of the latest MariaDB LTS of the development servers.
 */
const MARIADB_PORT = 3322

const here = dirname(fileURLToPath(import.meta.url))
const root = resolve(here, '../..')
const workDir = join(root, 'target', 'reload-bench')
const api = `http://${ADDRESS}/api`
const counts = process.argv.slice(2).map(Number)
if (counts.length === 0) {
  counts.push(1_000_000, 10_000_000)
}

/**
 * Query of `count` rows, with columns like those of the benchmark of the Pivoteur: texts,
 * datetime, integer with NULLs, decimal, float. `seq_1_to_<count>` is a table of the sequence
 * engine of MariaDB; the `LIMIT` written passes as it is.
 *
 * @param {number} count Number of rows.
 */
function query(count) {
  return `SELECT CONCAT('P', seq % 10) AS pays,
  CONCAT('P', seq % 10, '-R', seq % 7 % 5) AS region,
  CONCAT('Cat', LPAD(seq % 20, 2, '0')) AS categorie,
  CONCAT('Cat', LPAD(seq % 20, 2, '0'), '-P', LPAD(seq % 50, 2, '0')) AS produit,
  ELT(1 + seq % 4, 'web', 'magasin', 'telephone', 'partenaire') AS canal,
  TIMESTAMP '2024-01-01 00:00:00' + INTERVAL (seq * 7919) % 94694400 SECOND AS d,
  IF(seq % 20 = 0, NULL, 1 + seq % 100) AS qte,
  CAST(((seq * 104729) % 9999999 + 1) / 100 AS DECIMAL(12, 2)) AS montant,
  CAST((seq * 7) % 500000 / 1000000 AS DOUBLE) AS remise,
  CONCAT('C', LPAD(seq % 100000, 6, '0')) AS client
FROM seq_1_to_${count}
LIMIT ${count}`
}

/**
 * Calls the API and returns the JSON of its answer.
 *
 * @param {string} method Method.
 * @param {string} path Path after `/api`.
 * @param {object} [body] Body, sent as JSON.
 */
async function call(method, path, body) {
  const response = await fetch(`${api}${path}`, {
    method,
    headers: body === undefined ? {} : { 'Content-Type': 'application/json' },
    body: body === undefined ? undefined : JSON.stringify(body),
  })
  if (!response.ok) {
    throw new Error(`${method} ${path}: ${response.status} ${await response.text()}`)
  }
  return response.json()
}

/**
 * Waits until the server answers.
 */
async function serverUp() {
  for (let attempt = 0; attempt < 100; attempt++) {
    try {
      await fetch(`${api}/health`)
      return
    } catch {
      await new Promise((done) => setTimeout(done, 100))
    }
  }
  throw new Error('the server does not answer')
}

/**
 * Peak of the working set of a process, in gigabytes (Windows only, `null` elsewhere).
 *
 * @param {number} pid Id of the process.
 */
function peakMemory(pid) {
  if (process.platform !== 'win32') {
    return null
  }
  const peak = spawnSync(
    'powershell',
    ['-NoProfile', '-Command', `(Get-Process -Id ${pid}).PeakWorkingSet64`],
    { encoding: 'utf8' },
  )
  return Math.round(Number(peak.stdout.trim()) / 1e7) / 100
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
  const response = await fetch(`${api}/editors/${editor}/results`)
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

const indexFile = join(root, 'web', 'dist', 'index.html')
if (!existsSync(indexFile)) {
  console.error('web/dist is missing: run npm run build in web/ first')
  process.exit(1)
}
const build = spawnSync('cargo', ['build', '--release', '--package', 'prisme-server'], {
  cwd: root,
  stdio: 'inherit',
})
if (build.status !== 0) {
  process.exit(build.status ?? 1)
}

const results = []
for (const count of counts) {
  const dataDir = join(workDir, `data-${count}`)
  rmSync(dataDir, { recursive: true, force: true })
  mkdirSync(dataDir, { recursive: true })
  const executable = join(
    root,
    'target',
    'release',
    `prisme${process.platform === 'win32' ? '.exe' : ''}`,
  )
  const server = spawn(
    executable,
    ['--data-dir', dataDir, '--listen', ADDRESS, '--user-email', 'bench@local', '--verbose'],
    { cwd: root, stdio: ['ignore', 'pipe', 'pipe'] },
  )
  let log = ''
  server.stdout.on('data', (chunk) => (log += chunk))
  server.stderr.on('data', (chunk) => (log += chunk))
  const entry = { rows: count }
  try {
    await serverUp()
    const connection = await call('POST', '/connections', {
      name: 'bench',
      host: '127.0.0.1',
      port: MARIADB_PORT,
      username: 'root',
      password: '',
      database_name: 'mysql',
    })
    const editor = await call('POST', '/editors', { connection_id: connection.id })
    console.error(`reload: ${count} rows, query`)
    entry.query = await runQuery(editor.id, count)
    entry.server_peak_gb_after_query = peakMemory(server.pid)
    // A failed save leaves the tab without rows, and only a warning in the log.
    entry.query.saved = !log.includes('result not saved')
    console.error(`reload: ${count} rows, reading the saved tabs again`)
    entry.reload = await reload(editor.id)
    const lines = log.replace(/\u001b\[[0-9;]*m/g, '')
    const answer = lines.match(new RegExp(`GET /api/editors/${editor.id}/results \\d+ (\\d+) ms`))
    entry.reload.server_s = answer === null ? null : Number(answer[1]) / 1000
    entry.server_peak_gb = peakMemory(server.pid)
  } catch (error) {
    entry.error = String(error)
  }
  server.kill()
  writeFileSync(join(workDir, `server-${count}.log`), log)
  results.push(entry)
  console.log(JSON.stringify(entry))
}

mkdirSync(workDir, { recursive: true })
writeFileSync(join(workDir, 'results.json'), `${JSON.stringify(results, null, 2)}\n`)
