/**
 * Comparison of the SQL engines: runs each case of `cases.mjs` on every engine Prisme compares,
 * and notes its value, its type, its warnings and its error, for `docs/SQL-COMPARISON.md`.
 *
 * - MySQL and MariaDB: the six servers of `compose.yaml`, through the client of each container,
 *   as `root`: values, warnings and errors in batch mode, types by `--column-type-info`;
 * - PostgreSQL: the container of the version research `prisme-probe-postgres-<version>`, started
 *   for the run when it is stopped, then stopped again;
 * - DuckDB: `@duckdb/node-api`, installed on the first run into `target/sql-comparison/`, outside
 *   the dependencies of Prisme;
 * - SQLite: the one of Node (`node:sqlite`), in memory.
 *
 * Each case runs alone, so that an error stays its own. From the root of the repository, the
 * development servers started (`docker compose up --detach --wait`): `node bench/sql/run.mjs`.
 * The command prints the table of the values, and writes the results, with the versions, to
 * `target/sql-comparison/results.json`.
 */

import { spawnSync } from 'node:child_process'
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import { join } from 'node:path'
import { DatabaseSync } from 'node:sqlite'
import { pathToFileURL } from 'node:url'
import { ROOT } from '../prisme.mjs'
import { CASES, expression } from './cases.mjs'

/**
 * Version of `@duckdb/node-api` measured; changing it changes the DuckDB of the comparison.
 */
const DUCKDB_PACKAGE = '@duckdb/node-api@2.0.0-r.1'

/**
 * Services of `compose.yaml` measured, in the order of the columns.
 */
const MYSQL_SERVICES = [
  'mysql-oldest',
  'mysql-lts',
  'mysql-latest',
  'mariadb-oldest',
  'mariadb-lts',
  'mariadb-latest',
]

const workDir = join(ROOT, 'target', 'sql-comparison')

/**
 * @typedef {object} Measure
 * @property {string} expression Expression run.
 * @property {string | null} value Value as the engine writes it as text, `NULL` for NULL.
 * @property {string | null} type Type of the value, as the engine names it.
 * @property {string[]} warnings Warnings of the engine (MySQL and MariaDB).
 * @property {string | null} error Error of the engine, which replaces the value.
 */

/**
 * Runs a command and returns its output; the command must be found.
 *
 * @param {string} command Program.
 * @param {string[]} args Arguments.
 * @param {string} [input] Standard input.
 */
function run(command, args, input) {
  const done = spawnSync(command, args, { input, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 })
  if (done.error) {
    throw done.error
  }
  return { status: done.status, stdout: done.stdout, stderr: done.stderr }
}

/**
 * The cases that apply to an engine, with their expression.
 *
 * @param {import('./cases.mjs').Engine} engine Family of the engine.
 * @returns {[string, string][]} Pairs of key and expression.
 */
function casesOf(engine) {
  return CASES.map((entry) => [entry.key, expression(entry, engine)]).filter(
    ([, text]) => text !== null,
  )
}

/**
 * An empty measure of an expression.
 *
 * @param {string} text Expression.
 * @returns {Measure}
 */
function measure(text) {
  return { expression: text, value: null, type: null, warnings: [], error: null }
}

/**
 * Measures the cases on a server of `compose.yaml`. The script holds the version on its line 1,
 * then, for each case, a marker line and the line of the case: an error names its line.
 *
 * @param {string} service Service of `compose.yaml`.
 */
function mysql(service) {
  const client = service.startsWith('mysql') ? 'mysql' : 'mariadb'
  const cases = casesOf('mysql')
  const script =
    'SELECT VERSION() AS version;\n' +
    cases.map(([key, text]) => `SELECT 'case:${key}' AS m;\nSELECT ${text} AS v;\n`).join('')
  const execute = (mode) =>
    run(
      'docker',
      [
        'exec',
        '-i',
        `prisme-${service}`,
        client,
        '--user=root',
        '--default-character-set=utf8mb4',
        '--show-warnings',
        '--force',
        ...(mode === 'batch' ? ['--batch', '--raw'] : ['--table', '--column-type-info']),
      ],
      script,
    )
  const entries = Object.fromEntries(cases.map(([key, text]) => [key, measure(text)]))
  const batch = execute('batch')
  const lines = batch.stdout.split(/\r?\n/)
  let current = null
  for (let index = 0; index < lines.length; index++) {
    const text = lines[index]
    if (text === 'm' && lines[index + 1]?.startsWith('case:')) {
      current = lines[++index].slice('case:'.length)
    } else if (text === 'v' && current !== null && index + 1 < lines.length) {
      entries[current].value = lines[++index]
    } else if (current !== null && /^(Warning|Error) \(Code/.test(text)) {
      entries[current].warnings.push(text)
    }
  }
  for (const text of batch.stderr.split(/\r?\n/)) {
    const found = text.match(/^ERROR .* at line (\d+): .*/)
    if (found) {
      entries[cases[Math.floor((Number(found[1]) - 2) / 2)][0]].error = found[0]
    }
  }
  current = null
  let field = null
  for (const text of execute('table').stdout.split(/\r?\n/)) {
    current = text.match(/^\| case:(\S+)\s*\|/)?.[1] ?? current
    field = text.match(/^Field\s+\d+:\s+`(\w+)`/)?.[1] ?? field
    const type = text.match(/^Type:\s+(.*)/)?.[1]
    if (field === 'v' && current !== null && type) {
      entries[current].type = type.trim()
    }
  }
  return {
    engine: client === 'mysql' ? 'MySQL' : 'MariaDB',
    version: lines[1].split('-')[0],
    cases: entries,
  }
}

/**
 * Measures the cases on the container of the version research of PostgreSQL, through `psql`.
 * Several containers: the most recent version. The container is stopped again when the run
 * started it.
 */
function postgres() {
  const names = run('docker', [
    'ps',
    '--all',
    '--filter',
    'name=^prisme-probe-postgres-',
    '--format',
    '{{.Names}}',
  ])
    .stdout.split(/\r?\n/)
    .filter(Boolean)
    .sort((a, b) => a.localeCompare(b, 'en', { numeric: true }))
  const container = names.at(-1)
  if (!container) {
    throw new Error(
      'no container prisme-probe-postgres-<version> (docs/DEVELOPER.md, "Containers of the version research")',
    )
  }
  const running =
    run('docker', ['inspect', '--format', '{{.State.Running}}', container]).stdout.trim() === 'true'
  if (!running) {
    run('docker', ['start', container])
    for (
      let attempt = 0;
      run('docker', ['exec', container, 'pg_isready', '--username=postgres']).status !== 0;
      attempt++
    ) {
      if (attempt === 100) {
        throw new Error(`${container} does not answer`)
      }
      spawnSync(process.execPath, ['-e', 'setTimeout(() => {}, 200)'])
    }
  }
  try {
    const cases = casesOf('postgres')
    const script =
      "SELECT current_setting('server_version');\n" +
      cases
        .map(
          ([key, text]) =>
            `SELECT 'case:${key}';\nSELECT v, pg_typeof(v) FROM (SELECT ${text} AS v) AS t;\n`,
        )
        .join('')
    const done = run(
      'docker',
      [
        'exec',
        '-i',
        container,
        'psql',
        '--username=postgres',
        '--no-align',
        '--tuples-only',
        '--field-separator=\t',
        '--no-psqlrc',
        '--quiet',
        '--file=-',
      ],
      script,
    )
    const entries = Object.fromEntries(cases.map(([key, text]) => [key, measure(text)]))
    const lines = done.stdout.split(/\r?\n/)
    let current = null
    for (const text of lines.slice(1)) {
      if (text.startsWith('case:')) {
        current = text.slice('case:'.length)
      } else if (current !== null && entries[current].type === null && text.includes('\t')) {
        const cut = text.lastIndexOf('\t')
        entries[current].value = cut === 0 ? 'NULL' : text.slice(0, cut)
        entries[current].type = text.slice(cut + 1)
      }
    }
    for (const text of done.stderr.split(/\r?\n/)) {
      const found = text.match(/^psql:<stdin>:(\d+): (ERROR:\s+.*)/)
      if (found) {
        entries[cases[Math.floor((Number(found[1]) - 2) / 2)][0]].error = found[2]
      }
    }
    return { engine: 'PostgreSQL', version: lines[0].split(' ')[0], cases: entries }
  } finally {
    if (!running) {
      run('docker', ['stop', container])
    }
  }
}

/**
 * Measures the cases on DuckDB, in memory, installing its package for Node on the first run.
 */
async function duckdb() {
  const manifest = join(workDir, 'package.json')
  const [name, version] = [
    DUCKDB_PACKAGE.slice(0, DUCKDB_PACKAGE.lastIndexOf('@')),
    DUCKDB_PACKAGE.split('@').at(-1),
  ]
  const installed = join(workDir, 'node_modules', ...name.split('/'), 'package.json')
  if (!existsSync(installed) || JSON.parse(readFileSync(installed, 'utf8')).version !== version) {
    if (!existsSync(manifest)) {
      writeFileSync(manifest, '{ "private": true }\n')
    }
    // One command line through the shell: npm is a script on Windows, which only a shell runs.
    const done = spawnSync(`npm install --no-save --no-audit --no-fund ${DUCKDB_PACKAGE}`, {
      cwd: workDir,
      stdio: 'inherit',
      shell: true,
    })
    if (done.status !== 0) {
      throw new Error(`npm install ${DUCKDB_PACKAGE} failed`)
    }
  }
  const { DuckDBInstance } = await import(pathToFileURL(createRequire(manifest).resolve(name)).href)
  const connection = await (await DuckDBInstance.create(':memory:')).connect()
  const engineVersion = (await connection.runAndReadAll('SELECT version()')).getRows()[0][0]
  const entries = {}
  for (const [key, text] of casesOf('duckdb')) {
    entries[key] = measure(text)
    try {
      const row = (
        await connection.runAndReadAll(
          `SELECT CAST(v AS VARCHAR), typeof(v) FROM (SELECT ${text} AS v) AS t`,
        )
      ).getRows()[0]
      entries[key].value = row[0] === null ? 'NULL' : String(row[0])
      entries[key].type = String(row[1])
    } catch (error) {
      entries[key].error = String(error.message).split('\n')[0]
    }
  }
  connection.closeSync()
  return { engine: 'DuckDB', version: String(engineVersion).replace(/^v/, ''), cases: entries }
}

/**
 * Measures the cases on the SQLite of Node, in memory.
 */
function sqlite() {
  const database = new DatabaseSync(':memory:')
  const entries = {}
  for (const [key, text] of casesOf('sqlite')) {
    entries[key] = measure(text)
    try {
      const row = database
        .prepare(`SELECT CAST(v AS TEXT) AS v, typeof(v) AS t FROM (SELECT ${text} AS v)`)
        .get()
      entries[key].value = row.v === null ? 'NULL' : row.v
      entries[key].type = row.t
    } catch (error) {
      entries[key].error = String(error.message).split('\n')[0]
    }
  }
  const version = database.prepare('SELECT sqlite_version() AS v').get().v
  database.close()
  return { engine: 'SQLite', version, cases: entries }
}

/**
 * A measure as a cell of the table: the value and its type, `⚠` for a warning, or the error.
 *
 * @param {Measure | undefined} entry Measure, absent when the case does not apply.
 */
function cell(entry) {
  if (!entry) {
    return '—'
  }
  if (entry.error) {
    const text = entry.error.replace(/^ERROR:?\s+/, '').replace(/ at line \d+/, '')
    return `error: ${text.replaceAll('|', '/').slice(0, 70)}`
  }
  return `\`${entry.value}\`${entry.warnings.length ? ' ⚠' : ''} ${entry.type ?? ''}`.trim()
}

mkdirSync(workDir, { recursive: true })
const results = {}
for (const service of MYSQL_SERVICES) {
  results[service] = mysql(service)
}
results.postgres = postgres()
results.duckdb = await duckdb()
results.sqlite = sqlite()
writeFileSync(join(workDir, 'results.json'), `${JSON.stringify(results, null, 1)}\n`)

const columns = Object.values(results)
console.log(
  `| Case | ${columns.map((column) => `${column.engine} ${column.version}`).join(' | ')} |`,
)
console.log(`|---|${columns.map(() => '---|').join('')}`)
for (const { key } of CASES) {
  console.log(`| ${key} | ${columns.map((column) => cell(column.cases[key])).join(' | ')} |`)
}
