/**
 * Trace of the interface of Prisme while it receives a result: on what the main thread of the
 * page spends its time (scripts, microtasks of Vue, layout, painting, layers...). Complements
 * `run.mjs`, whose measure of the interface says how long, not on what.
 *
 * Starts Prisme and Chrome as `run.mjs` does, opens the interface on an editor that holds the
 * query of that many rows, records a trace of Chrome from Ctrl+Enter to `done` and the drawing
 * that follows, then sums, on the main thread of the page, the own time of each kind of event:
 * its duration less that of the events it holds.
 *
 * From the root of the repository, as for `run.mjs`: `node bench/store/trace.mjs [rows]`
 * (200,000 rows by default: the trace slows the page down, and grows with the rows). The trace
 * goes to `target/store-bench/trace-<rows>.json`, which the Performance panel of DevTools opens.
 */

import { writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { ROOT, build, query, start } from '../prisme.mjs'
import { evaluate, launch, pause } from './cdp.mjs'
import { openInterface, runInInterface } from './interface.mjs'

/**
 * Categories of the trace: tasks, work of the page, scripts, garbage collection.
 */
const CATEGORIES = [
  'toplevel',
  'devtools.timeline',
  'disabled-by-default-devtools.timeline',
  'v8.execute',
  'disabled-by-default-v8.gc',
]

/**
 * Kinds of events shown, by own time.
 */
const SHOWN = 15

const workDir = join(ROOT, 'target', 'store-bench')
const count = Number(process.argv[2] ?? 200_000)

/**
 * Records a trace of the page until `done`, plus two seconds for the drawing.
 *
 * @param {{ send: Function, next: Function }} page Page.
 * @returns {Promise<object[]>} Events of the trace.
 */
async function trace(page) {
  await page.send('Tracing.start', {
    transferMode: 'ReturnAsStream',
    traceConfig: { includedCategories: CATEGORIES },
  })
  await runInInterface(page)
  await pause(2_000)
  const complete = page.next('Tracing.tracingComplete')
  await page.send('Tracing.end')
  const { stream } = await complete
  const chunks = []
  for (;;) {
    const { data, eof, base64Encoded } = await page.send('IO.read', { handle: stream })
    chunks.push(base64Encoded ? Buffer.from(data, 'base64').toString('utf8') : data)
    if (eof) {
      break
    }
  }
  await page.send('IO.close', { handle: stream })
  const events = JSON.parse(chunks.join(''))
  return Array.isArray(events) ? events : events.traceEvents
}

/**
 * Own time and number of each kind of event of the main thread of the page, longest first.
 *
 * @param {object[]} events Events of the trace.
 * @returns {[string, { own: number, count: number }][]} Kinds, with their own time in
 *   microseconds.
 */
function ownTimes(events) {
  // The main thread of the renderer that worked the most: that of the page.
  const threads = new Map()
  for (const event of events) {
    if (event.ph === 'M' && event.args?.name === 'CrRendererMain') {
      threads.set(`${event.pid}:${event.tid}`, [])
    }
  }
  for (const event of events) {
    if (event.ph === 'X' && event.dur > 0) {
      threads.get(`${event.pid}:${event.tid}`)?.push(event)
    }
  }
  const list = [...threads.values()]
    .reduce((longest, thread) => (thread.length > longest.length ? thread : longest))
    .sort((a, b) => a.ts - b.ts || b.dur - a.dur)
  // Each event holds those that start before its end; its own time leaves out their duration.
  const held = new Map()
  const stack = []
  for (const event of list) {
    while (stack.length > 0 && stack.at(-1).ts + stack.at(-1).dur <= event.ts) {
      stack.pop()
    }
    if (stack.length > 0) {
      held.set(stack.at(-1), (held.get(stack.at(-1)) ?? 0) + event.dur)
    }
    stack.push(event)
  }
  const kinds = new Map()
  for (const event of list) {
    const kind = kinds.get(event.name) ?? { own: 0, count: 0 }
    kind.own += event.dur - (held.get(event) ?? 0)
    kind.count++
    kinds.set(event.name, kind)
  }
  return [...kinds].sort((a, b) => b[1].own - a[1].own)
}

build()
const prisme = await start(join(workDir, `data-trace-${count}`))
const page = await launch(join(workDir, 'chrome'))
try {
  await openInterface(page, prisme.connection, query(count))
  const events = await trace(page)
  const probe = await evaluate(page, () => window.benchProbe)
  const file = join(workDir, `trace-${count}.json`)
  writeFileSync(file, JSON.stringify({ traceEvents: events }))
  const kinds = ownTimes(events)
  const total = kinds.reduce((sum, [, kind]) => sum + kind.own, 0)
  console.log(`Trace of the interface: ${count.toLocaleString('en')} rows, ${page.browser}`)
  console.log(
    `Last rows received after ${((probe.lastRows - probe.sent) / 1000).toFixed(2)} s, done after ${((probe.done - probe.sent) / 1000).toFixed(2)} s; main thread busy ${(total / 1e6).toFixed(2)} s`,
  )
  console.log('| Event | Number | Own time (s) | Share |')
  console.log('| --- | ---: | ---: | ---: |')
  for (const [name, kind] of kinds.slice(0, SHOWN)) {
    console.log(
      `| ${name} | ${kind.count} | ${(kind.own / 1e6).toFixed(3)} | ${Math.round((100 * kind.own) / total)} % |`,
    )
  }
  console.log(`Trace: ${file}`)
} finally {
  page.close()
  prisme.stop()
}
