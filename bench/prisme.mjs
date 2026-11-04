/**
 * Prisme on the real server, for the benchmarks that measure it (`reload/`, `store/`): its build,
 * its start on a throwaway data folder, the calls to its API, and a query of many rows on the
 * latest MariaDB LTS of the development servers (`compose.yaml`).
 */

import { spawn, spawnSync } from 'node:child_process'
import { existsSync, mkdirSync, rmSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

/**
 * Address of the throwaway server, away from the port of the usual one.
 */
export const ADDRESS = '127.0.0.1:18484'

/**
 * Port of the latest MariaDB LTS of the development servers.
 */
const MARIADB_PORT = 3322

/**
 * Container of the latest MariaDB LTS of the development servers.
 */
export const MARIADB_CONTAINER = 'prisme-mariadb-lts'

/**
 * Root of the repository.
 */
export const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..')

const api = `http://${ADDRESS}/api`

/**
 * Query of `count` rows, with columns like those of the benchmark of the Pivoteur: texts,
 * datetime, integer with NULLs, decimal, float. `seq_1_to_<count>` is a table of the sequence
 * engine of MariaDB; the `LIMIT` written passes as it is.
 *
 * @param {number} count Number of rows.
 */
export function query(count) {
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
 * Calls the API and returns the JSON of its answer, `null` for an answer without a body.
 *
 * @param {string} method Method.
 * @param {string} path Path after `/api`.
 * @param {object} [body] Body, sent as JSON.
 */
export async function call(method, path, body) {
  const response = await fetch(`${api}${path}`, {
    method,
    headers: body === undefined ? {} : { 'Content-Type': 'application/json' },
    body: body === undefined ? undefined : JSON.stringify(body),
  })
  if (!response.ok) {
    throw new Error(`${method} ${path}: ${response.status} ${await response.text()}`)
  }
  const text = await response.text()
  return text === '' ? null : JSON.parse(text)
}

/**
 * Executable of Prisme: the one named by the `PRISME_EXECUTABLE` variable when set (another build,
 * to compare), otherwise the release build of the repository.
 */
const EXECUTABLE =
  process.env.PRISME_EXECUTABLE ??
  join(ROOT, 'target', 'release', `prisme${process.platform === 'win32' ? '.exe' : ''}`)

/**
 * Builds Prisme in release, with its interface embedded. Exits when the interface is not built
 * or when the build fails. Builds nothing when `PRISME_EXECUTABLE` names another executable.
 */
export function build() {
  if (process.env.PRISME_EXECUTABLE !== undefined) {
    return
  }
  if (!existsSync(join(ROOT, 'web', 'dist', 'index.html'))) {
    console.error('web/dist is missing: run npm run build in web/ first')
    process.exit(1)
  }
  const cargo = spawnSync('cargo', ['build', '--release', '--package', 'prisme-server'], {
    cwd: ROOT,
    stdio: 'inherit',
  })
  if (cargo.status !== 0) {
    process.exit(cargo.status ?? 1)
  }
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
 * Starts Prisme (`EXECUTABLE`) on an emptied data folder, with its log (`--verbose`) unless told
 * otherwise, waits until it answers, then saves a connection to MariaDB. The server is stopped if
 * it fails to start.
 *
 * With `--verbose`, the server also saves each line of its log in its internal database: one line
 * per message sent, which slows down the reception of a large result (`docs/PERFORMANCES.md`,
 * section 10.7).
 *
 * @param {string} dataDir Data folder.
 * @param {{ verbose?: boolean }} [options] Start with the log of the messages (true by default).
 * @returns {Promise<{ pid: number, connection: number, log: () => string, stop: () => void }>}
 *   Id of the process, id of the connection, log so far, and the stop of the server.
 */
export async function start(dataDir, { verbose = true } = {}) {
  rmSync(dataDir, { recursive: true, force: true })
  mkdirSync(dataDir, { recursive: true })
  const options = ['--data-dir', dataDir, '--listen', ADDRESS, '--user-email', 'bench@local']
  const server = spawn(EXECUTABLE, verbose ? [...options, '--verbose'] : options, {
    cwd: ROOT,
    stdio: ['ignore', 'pipe', 'pipe'],
  })
  let log = ''
  server.stdout.on('data', (chunk) => (log += chunk))
  server.stderr.on('data', (chunk) => (log += chunk))
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
    return { pid: server.pid, connection: connection.id, log: () => log, stop: () => server.kill() }
  } catch (error) {
    server.kill()
    throw error
  }
}

/**
 * Lines of a log, without their colors.
 *
 * @param {string} log Log.
 */
export function plain(log) {
  return log.replace(/\u001b\[[0-9;]*m/g, '')
}

/**
 * Peak of the working set of a process, in gigabytes (Windows only, `null` elsewhere).
 *
 * @param {number} pid Id of the process.
 */
export function peakMemory(pid) {
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
