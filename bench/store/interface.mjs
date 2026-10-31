/**
 * The interface of Prisme driven in Chrome (`cdp.mjs`), for the benchmarks of the store of the
 * results (`run.mjs`, `trace.mjs`): opening on an editor that holds a query, run by Ctrl+Enter,
 * and the waits on the probe of the WebSocket (`interfaceProbe` of `browser.mjs`).
 */

import { performance } from 'node:perf_hooks'
import { ADDRESS, call } from '../prisme.mjs'
import { interfaceProbe } from './browser.mjs'
import { evaluate, navigate, pause } from './cdp.mjs'

/**
 * Durations of the main thread of the page so far, in seconds (`Performance.getMetrics`).
 *
 * @param {{ send: Function }} page Page.
 */
export async function mainThread(page) {
  const { metrics } = await page.send('Performance.getMetrics')
  const value = (name) => metrics.find((metric) => metric.name === name)?.value ?? 0
  return {
    task_s: value('TaskDuration'),
    script_s: value('ScriptDuration'),
    layout_s: value('LayoutDuration') + value('RecalcStyleDuration'),
  }
}

/**
 * Waits until a function run in the page returns true.
 *
 * @param {{ send: Function }} page Page.
 * @param {Function} test Function run in the page.
 * @param {number} timeoutMs Longest wait, in milliseconds.
 * @param {string} what What is waited for, for the error.
 */
async function until(page, test, timeoutMs, what) {
  const limit = performance.now() + timeoutMs
  while (!(await evaluate(page, test))) {
    if (performance.now() > limit) {
      throw new Error(`still waiting for ${what}`)
    }
    await pause(250)
  }
}

/**
 * Waits until the main thread of the page has worked less than 25 ms in half a second.
 *
 * @param {{ send: Function }} page Page.
 */
async function quiet(page) {
  let last = (await mainThread(page)).task_s
  for (let attempt = 0; attempt < 60; attempt++) {
    await pause(500)
    const now = (await mainThread(page)).task_s
    if (now - last < 0.025) {
      return
    }
    last = now
  }
  throw new Error('the page stays busy')
}

/**
 * Opens the interface of Prisme on a new editor that holds the query, with its probe, and waits
 * until its session is ready and its main thread quiet (the catalog of the connection loaded).
 *
 * @param {{ send: Function, next: Function }} page Page.
 * @param {number} connection Id of the connection.
 * @param {string} sql Query.
 */
export async function openInterface(page, connection, sql) {
  const editor = await call('POST', '/editors', { connection_id: connection })
  await call('PUT', `/editors/${editor.id}`, { name: '', sql, connection_id: connection })
  const { identifier } = await page.send('Page.addScriptToEvaluateOnNewDocument', {
    source: `(${interfaceProbe})()`,
  })
  await navigate(page, `http://${ADDRESS}/`)
  await page.send('Page.removeScriptToEvaluateOnNewDocument', { identifier })
  await until(
    page,
    () => window.benchProbe.ready !== null && document.querySelector('.cm-content') !== null,
    30_000,
    'the session of the editor',
  )
  await quiet(page)
}

/**
 * Runs the query of the interface opened by `openInterface`: Ctrl+Enter in the editor, then the
 * wait for `done`.
 *
 * @param {{ send: Function }} page Page.
 */
export async function runInInterface(page) {
  await evaluate(page, () => document.querySelector('.cm-content').focus())
  for (const type of ['rawKeyDown', 'keyUp']) {
    await page.send('Input.dispatchKeyEvent', {
      type,
      modifiers: 2,
      key: 'Enter',
      code: 'Enter',
      windowsVirtualKeyCode: 13,
    })
  }
  await until(page, () => window.benchProbe.sent !== null, 5_000, 'the query to leave')
  await until(page, () => window.benchProbe.done !== null, 600_000, 'the end of the query')
}
