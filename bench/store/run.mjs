/**
 * Benchmark of the store of the results: where the time of the reception of a large result goes
 * (database, server, transport, page), and the round trip of the visible rows between the page
 * and a Worker that holds the rows (single store of the results, `docs/QUESTIONS.md`, section 2).
 *
 * Builds Prisme, starts it on a throwaway data folder with a connection to the latest MariaDB LTS
 * of the development servers (`compose.yaml`), then Chrome without a window (`cdp.mjs`). For each
 * number of rows, the same query of that many rows (`../prisme.mjs`):
 *
 * 1. by the database alone: run by the client of MariaDB in its container, the rows thrown away;
 * 2. in the interface of Prisme: written in an editor, run by Ctrl+Enter once the interface is
 *    quiet, the WebSocket and the main thread of the page observed (`interface.mjs`);
 * 3. in a page that throws the rows away: the server and the transport alone;
 * 4. in a page that handles them as the interface does, without the interface;
 * 5. in a Worker that keeps them in columns, while the page asks for windows of visible rows at
 *    each frame, then once the Worker is idle.
 *
 * Steps 3 to 5 run in a page of the address of Prisme (`/api/health`), which the server accepts
 * as the origin of the WebSocket. During each step, the processor time of Prisme is sampled
 * (Windows only), and that of MariaDB read before and after.
 *
 * From the root of the repository, the development servers started
 * (`docker compose up --detach --wait`) and the interface built (`npm run build` in `web/`):
 * `node bench/store/run.mjs [rows...]` (1,000,000 rows by default). The results go to
 * `target/store-bench/results-<rows>.json`. With `--interface`, only the steps 1 and 2 run, to
 * compare changes of the interface; their results go to `results-<rows>-interface.json`.
 */

import { spawn, spawnSync } from 'node:child_process'
import { mkdirSync, writeFileSync } from 'node:fs'
import { cpus, release, totalmem, type } from 'node:os'
import { join } from 'node:path'
import { performance } from 'node:perf_hooks'
import { utf8Length } from '../pivot/js/transfer.mjs'
import { ADDRESS, MARIADB_CONTAINER, ROOT, build, call, query, start } from '../prisme.mjs'
import { receiveInPage, receiveInWorker, workerMain } from './browser.mjs'
import { evaluate, launch, navigate, pause } from './cdp.mjs'
import { mainThread, openInterface, runInInterface } from './interface.mjs'

/**
 * Time left to the interface after `done`, to apply its last batch and draw.
 */
const SETTLE_MS = 2_000

/**
 * Duration of each phase of the windows asked to the idle Worker.
 */
const IDLE_MS = 3_000

const workDir = join(ROOT, 'target', 'store-bench')
const interfaceOnly = process.argv.includes('--interface')
const counts = process.argv
  .slice(2)
  .filter((arg) => !arg.startsWith('--'))
  .map(Number)
if (counts.length === 0) {
  counts.push(1_000_000)
}

/**
 * Samples the processor time of a process every 100 ms (Windows only: no sample elsewhere).
 *
 * @param {number} pid Id of the process.
 * @returns {{ samples: [number, number][], stop: () => void }}
 *   Samples, as instant (ms since 1970) and processor time (ms), and the stop of the sampling.
 */
function sampleCpu(pid) {
  const samples = []
  if (process.platform !== 'win32') {
    return { samples, stop: () => {} }
  }
  const script = [
    `$p = Get-Process -Id ${pid}`,
    'while (-not $p.HasExited) {',
    '$p.Refresh()',
    '[Console]::Out.WriteLine("$([DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()) $($p.TotalProcessorTime.TotalMilliseconds)")',
    'Start-Sleep -Milliseconds 100',
    '}',
  ].join('; ')
  const sampler = spawn('powershell', ['-NoProfile', '-Command', script], {
    stdio: ['ignore', 'pipe', 'ignore'],
  })
  let rest = ''
  sampler.stdout.on('data', (chunk) => {
    const lines = (rest + chunk).split('\n')
    rest = lines.pop()
    for (const line of lines) {
      const [at, cpu] = line.trim().split(' ').map(Number)
      samples.push([at, cpu])
    }
  })
  return { samples, stop: () => sampler.kill() }
}

/**
 * Processor time of the process at an instant, interpolated between two samples, in seconds;
 * `null` without samples around the instant.
 *
 * @param {[number, number][]} samples Samples.
 * @param {number} at Instant, in ms since 1970.
 */
function cpuAt(samples, at) {
  const after = samples.findIndex(([instant]) => instant >= at)
  if (after < 1) {
    return null
  }
  const [t0, c0] = samples[after - 1]
  const [t1, c1] = samples[after]
  return (c0 + ((c1 - c0) * (at - t0)) / Math.max(1, t1 - t0)) / 1000
}

/**
 * Processor time of the server of MariaDB (process 1 of its container), in seconds.
 */
function mariadbCpu() {
  const stat = spawnSync('docker', ['exec', MARIADB_CONTAINER, 'cat', '/proc/1/stat'], {
    encoding: 'utf8',
  })
  // After the name of the process, between parentheses: utime and stime are the 12th and 13th
  // fields, in hundredths of a second.
  const fields = stat.stdout.slice(stat.stdout.lastIndexOf(')') + 2).split(' ')
  return (Number(fields[11]) + Number(fields[12])) / 100
}

/**
 * Runs SQL with the client of MariaDB in its container, the rows thrown away, and returns its
 * duration, in seconds, measured in the container: the start of `docker exec` is left out.
 *
 * @param {string} sql SQL.
 */
function mariadbClient(sql) {
  const script = [
    'started=$(date +%s%N)',
    'mariadb --quick --batch --skip-column-names --user=root mysql > /dev/null',
    'echo $(( $(date +%s%N) - started ))',
  ].join('; ')
  const client = spawnSync(
    'docker',
    ['exec', '--interactive', MARIADB_CONTAINER, 'sh', '-c', script],
    { input: sql, encoding: 'utf8' },
  )
  if (client.status !== 0) {
    throw new Error(`mariadb: ${client.stderr}`)
  }
  return Number(client.stdout.trim()) / 1e9
}

/**
 * The query run by the database alone, once to warm it up, then measured.
 *
 * @param {string} sql Query.
 */
function databaseAlone(sql) {
  mariadbClient(sql)
  const cpu = mariadbCpu()
  const duration = mariadbClient(sql)
  return { duration_s: duration, mariadb_cpu_s: mariadbCpu() - cpu }
}

/**
 * Runs a step and adds to its result the processor time of Prisme, until the last rows then
 * during the save, and that of MariaDB.
 *
 * @param {{ samples: [number, number][] }} cpu Samples of Prisme.
 * @param {() => Promise<{ epoch: { sent: number, last_rows: number, done: number } }>} step Step.
 */
async function measured(cpu, step) {
  const before = mariadbCpu()
  const result = await step()
  result.mariadb_cpu_s = mariadbCpu() - before
  // The sample after `done`.
  await pause(300)
  const { sent, last_rows: lastRows, done } = result.epoch
  const [atSent, atLast, atDone] = [sent, lastRows, done].map((at) => cpuAt(cpu.samples, at))
  result.prisme_cpu_s = atSent === null ? null : { rows: atLast - atSent, save: atDone - atLast }
  return result
}

/**
 * JavaScript memory of the page after a full garbage collection, in megabytes.
 *
 * @param {{ send: Function }} page Page.
 */
async function pageHeap(page) {
  await page.send('HeapProfiler.collectGarbage')
  const { usedSize } = await page.send('Runtime.getHeapUsage')
  return Math.round(usedSize / 1e6)
}

/**
 * Runs the query of the interface opened by `openInterface`, then waits for the drawing. Gives the
 * work of the main thread and the memory of the page before and after.
 *
 * @param {{ send: Function, next: Function }} page Page.
 * @param {number} count Number of rows.
 */
async function inInterface(page, count) {
  const heapBefore = await pageHeap(page)
  const before = await mainThread(page)
  await runInInterface(page)
  await pause(SETTLE_MS)
  const after = await mainThread(page)
  const probe = await evaluate(
    page,
    (rows) => ({
      ...window.benchProbe,
      timeOrigin: performance.timeOrigin,
      // The status bar of the result: "1 000 000 lignes", whatever the spaces.
      shown: document.body.innerText.replace(/\s/g, '').includes(`${rows}lignes`),
    }),
    count,
  )
  const settled = probe.done + SETTLE_MS
  const tasks = probe.longTasks
    .filter(([startTime]) => startTime >= probe.sent && startTime <= settled)
    .map(([, duration]) => duration)
  const result = {
    epoch: {
      sent: probe.timeOrigin + probe.sent,
      last_rows: probe.timeOrigin + probe.lastRows,
      done: probe.timeOrigin + probe.done,
    },
    first_rows_s: (probe.firstRows - probe.sent) / 1000,
    rows_s: (probe.lastRows - probe.sent) / 1000,
    done_s: (probe.done - probe.sent) / 1000,
    shown: probe.shown,
    main_thread: Object.fromEntries(
      Object.entries(after).map(([name, value]) => [name, value - before[name]]),
    ),
    long_tasks: {
      count: tasks.length,
      total_s: tasks.reduce((total, duration) => total + duration, 0) / 1000,
      longest_s: Math.max(0, ...tasks) / 1000,
    },
    heap_before_mb: heapBefore,
    heap_mb: await pageHeap(page),
  }
  await navigate(page, `http://${ADDRESS}/api/health`)
  return result
}

/**
 * Runs the query in a page of the address of Prisme, by a function of `browser.mjs`, in a new
 * editor; adds the memory of the page before and after, then leaves it.
 *
 * @param {{ send: Function, next: Function }} page Page.
 * @param {number} connection Id of the connection.
 * @param {(editor: number) => Promise<object>} receive Reception, given the id of the editor.
 */
async function inPage(page, connection, receive) {
  const editor = await call('POST', '/editors', { connection_id: connection })
  await navigate(page, `http://${ADDRESS}/api/health`)
  const heapBefore = await pageHeap(page)
  const result = await receive(editor.id)
  result.heap_before_mb = heapBefore
  result.heap_mb = await pageHeap(page)
  await navigate(page, `http://${ADDRESS}/api/health`)
  return result
}

/**
 * Rounds the numbers of a result for its JSON: 3 digits after the point.
 *
 * @param {unknown} value Result.
 */
function rounded(value) {
  return JSON.parse(
    JSON.stringify(value, (key, item) =>
      typeof item === 'number' && !Number.isInteger(item) ? Number(item.toFixed(3)) : item,
    ),
  )
}

/**
 * Summary of the measures of a number of rows, as Markdown tables.
 *
 * @param {object} entry Measures, rounded.
 */
function summary(entry) {
  const columns = [
    ['Interface', entry.interface],
    ['Page, rows thrown away', entry.discard],
    ['Page, rows handled as the interface', entry.page],
    ['Worker, store in columns', entry.worker],
  ].filter(([, step]) => step !== undefined)
  const steps = columns.map(([, step]) => step)
  const cells = (pick) => steps.map((step) => pick(step) ?? '').join(' | ')
  const cpu = (step) => step.prisme_cpu_s && `${step.prisme_cpu_s.rows} / ${step.prisme_cpu_s.save}`
  const megabytes = (bytes) => Math.round(bytes / 1e6)
  const lines = [
    `Store of the results: ${entry.rows.toLocaleString('en')} rows`,
    `${entry.machine.cpu}, ${entry.machine.logical_cores} logical cores, ${entry.machine.memory_gb} GB, ${entry.machine.os}, Node ${entry.machine.node}, ${entry.machine.browser}`,
    `Database alone: ${entry.database.duration_s} s, processor of MariaDB ${entry.database.mariadb_cpu_s} s`,
    `| Step | ${columns.map(([name]) => name).join(' | ')} |`,
    `| --- |${' ---: |'.repeat(columns.length)}`,
    `| Last rows received (s) | ${cells((step) => step.rows_s)} |`,
    `| \`done\` received (s) | ${cells((step) => step.done_s)} |`,
    `| Processor of Prisme, rows / save (s) | ${cells(cpu)} |`,
    `| Processor of MariaDB (s) | ${cells((step) => step.mariadb_cpu_s)} |`,
    `| Handling of the messages (s) | ${cells((step) => step.handling_s ?? (step.parsing_s !== undefined ? `${step.parsing_s} parsing, ${step.storing_s} store` : null))} |`,
    `| Main thread: tasks / scripts / layout (s) | ${cells((step) => step.main_thread && `${step.main_thread.task_s} / ${step.main_thread.script_s} / ${step.main_thread.layout_s}`)} |`,
    `| Long tasks: number, longest (s) | ${cells((step) => `${step.long_tasks.count}, ${step.long_tasks.longest_s}`)} |`,
    `| Memory of the page, before / after (MB) | ${cells((step) => `${step.heap_before_mb} / ${step.heap_mb}`)} |`,
    `| Store in columns, used / reserved (MB) | ${cells((step) => step.store && `${megabytes(step.store.used)} / ${megabytes(step.store.reserved)}`)} |`,
  ]
  if (entry.worker === undefined) {
    return lines.join('\n')
  }
  lines.push(
    '',
    `Windows of visible rows asked to the Worker (${entry.worker.frames} frames):`,
    '| Windows | Number | Median (ms) | 95th percentile (ms) | 99th percentile (ms) | Longest (ms) |',
    '| --- | ---: | ---: | ---: | ---: | ---: |',
  )
  const names = {
    reception: '50 rows, during the reception',
    'idle 50': '50 rows, Worker idle',
    'idle 1000': '1,000 rows, Worker idle',
  }
  for (const [name, label] of Object.entries(names)) {
    const delays = entry.worker.windows[name]
    lines.push(
      `| ${label} | ${delays.windows} | ${delays.median_ms ?? ''} | ${delays.p95_ms ?? ''} | ${delays.p99_ms ?? ''} | ${delays.max_ms ?? ''} |`,
    )
  }
  return lines.join('\n')
}

build()
mkdirSync(workDir, { recursive: true })
const page = await launch(join(workDir, 'chrome'))
const machine = {
  cpu: cpus()[0].model.trim(),
  logical_cores: cpus().length,
  memory_gb: Math.round(totalmem() / 1e8) / 10,
  os: `${type()} ${release()}`,
  node: process.version,
  browser: page.browser,
}
try {
  for (const count of counts) {
    const sql = query(count)
    const prisme = await start(join(workDir, `data-${count}`))
    const cpu = sampleCpu(prisme.pid)
    const entry = { rows: count, machine }
    try {
      console.error(`store: ${count} rows, database alone`)
      entry.database = databaseAlone(sql)
      console.error(`store: ${count} rows, interface`)
      await openInterface(page, prisme.connection, sql)
      entry.interface = await measured(cpu, () => inInterface(page, count))
      if (!interfaceOnly) {
        console.error(`store: ${count} rows, page throwing the rows away`)
        entry.discard = await measured(cpu, () =>
          inPage(page, prisme.connection, (editor) =>
            evaluate(page, receiveInPage, utf8Length, ADDRESS, editor, sql, 'discard'),
          ),
        )
        console.error(`store: ${count} rows, page handling the rows as the interface`)
        entry.page = await measured(cpu, () =>
          inPage(page, prisme.connection, (editor) =>
            evaluate(page, receiveInPage, utf8Length, ADDRESS, editor, sql, 'page'),
          ),
        )
        console.error(`store: ${count} rows, Worker and windows of visible rows`)
        const source = `(${workerMain})(${utf8Length})`
        entry.worker = await measured(cpu, () =>
          inPage(page, prisme.connection, (editor) =>
            evaluate(page, receiveInWorker, source, ADDRESS, editor, sql, IDLE_MS),
          ),
        )
      }
    } finally {
      cpu.stop()
      prisme.stop()
      writeFileSync(join(workDir, `server-${count}.log`), prisme.log())
    }
    const file = join(workDir, `results-${count}${interfaceOnly ? '-interface' : ''}.json`)
    writeFileSync(file, `${JSON.stringify(rounded(entry), null, 2)}\n`)
    console.log(summary(rounded(entry)))
    console.log(`Full results: ${file}`)
  }
} finally {
  page.close()
}
