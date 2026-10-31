/**
 * Chrome without a window, driven over the DevTools protocol, for the benchmarks that measure the
 * page: nothing to install but the browser. The browser is the one named by the `CHROME`
 * variable when set, otherwise Chrome or Edge at their usual place on Windows, otherwise
 * `google-chrome` from the path.
 */

import { spawn } from 'node:child_process'
import { existsSync, mkdirSync, readFileSync, rmSync } from 'node:fs'
import { join } from 'node:path'

/**
 * Usual places of the browsers on Windows, in order of preference.
 */
const BROWSERS = [
  'C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe',
  'C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe',
]

/**
 * Executable of the browser.
 */
function browserPath() {
  return process.env.CHROME ?? BROWSERS.find((path) => existsSync(path)) ?? 'google-chrome'
}

/**
 * Waits a little.
 *
 * @param {number} ms Delay, in milliseconds.
 */
export function pause(ms) {
  return new Promise((done) => setTimeout(done, ms))
}

/**
 * Connection to the browser: commands, with their answers, and events.
 */
class Connection {
  /**
   * Opens the connection on a WebSocket of the DevTools protocol.
   *
   * @param {WebSocket} socket WebSocket of the browser.
   */
  constructor(socket) {
    this.socket = socket
    this.lastId = 0
    this.pending = new Map()
    this.waiters = []
    socket.addEventListener('message', (event) => {
      const message = JSON.parse(event.data)
      if (message.id !== undefined) {
        const { resolve, reject } = this.pending.get(message.id)
        this.pending.delete(message.id)
        if (message.error) {
          reject(new Error(`${message.error.message} (${message.error.code})`))
        } else {
          resolve(message.result)
        }
        return
      }
      this.waiters = this.waiters.filter((waiter) => {
        const match = waiter.method === message.method && waiter.session === message.sessionId
        if (match) {
          waiter.resolve(message.params)
        }
        return !match
      })
    })
  }

  /**
   * Sends a command and returns its result.
   *
   * @param {string} method Command, such as `Page.navigate`.
   * @param {object} params Parameters.
   * @param {string} [session] Session of a page, absent for the browser itself.
   */
  send(method, params = {}, session = undefined) {
    const id = ++this.lastId
    this.socket.send(JSON.stringify({ id, method, params, sessionId: session }))
    return new Promise((resolve, reject) => this.pending.set(id, { resolve, reject }))
  }

  /**
   * Waits for the next event of a kind.
   *
   * @param {string} method Event, such as `Page.loadEventFired`.
   * @param {string} [session] Session of a page.
   */
  next(method, session = undefined) {
    return new Promise((resolve) => this.waiters.push({ method, session, resolve }))
  }
}

/**
 * Starts the browser without a window, on an emptied profile, and opens a page of 1,600 × 1,000.
 *
 * @param {string} profileDir Folder of the profile.
 * @returns {Promise<{ send: (method: string, params?: object) => Promise<object>,
 *   next: (method: string) => Promise<object>, close: () => void, browser: string }>}
 *   Commands and events of the page, and the stop of the browser.
 */
export async function launch(profileDir) {
  rmSync(profileDir, { recursive: true, force: true })
  mkdirSync(profileDir, { recursive: true })
  const executable = browserPath()
  const browser = spawn(
    executable,
    [
      '--headless=new',
      '--remote-debugging-port=0',
      `--user-data-dir=${profileDir}`,
      '--no-first-run',
      '--no-default-browser-check',
      '--window-size=1600,1000',
      // A page without a window keeps its timers and its frames.
      '--disable-background-timer-throttling',
      '--disable-renderer-backgrounding',
      '--disable-backgrounding-occluded-windows',
      // A page left does not stay in memory, counted in that of the next one.
      '--disable-features=BackForwardCache',
      'about:blank',
    ],
    { stdio: 'ignore' },
  )
  // The browser writes its port, then the path of its WebSocket, once it listens.
  const portFile = join(profileDir, 'DevToolsActivePort')
  let lines = []
  for (let attempt = 0; attempt < 100 && lines.length < 2; attempt++) {
    await pause(100)
    lines = existsSync(portFile) ? readFileSync(portFile, 'utf8').split('\n').filter(Boolean) : []
  }
  if (lines.length < 2) {
    browser.kill()
    throw new Error(`the browser does not answer: ${executable}`)
  }
  const socket = new WebSocket(`ws://127.0.0.1:${lines[0]}${lines[1]}`)
  await new Promise((resolve, reject) => {
    socket.addEventListener('open', resolve)
    socket.addEventListener('error', reject)
  })
  const connection = new Connection(socket)
  const { targetId } = await connection.send('Target.createTarget', { url: 'about:blank' })
  const { sessionId } = await connection.send('Target.attachToTarget', { targetId, flatten: true })
  const { product } = await connection.send('Browser.getVersion')
  await connection.send('Page.enable', {}, sessionId)
  await connection.send('Performance.enable', {}, sessionId)
  return {
    send: (method, params) => connection.send(method, params, sessionId),
    next: (method) => connection.next(method, sessionId),
    close: () => {
      socket.close()
      browser.kill()
    },
    browser: product,
  }
}

/**
 * Runs a function in the page and returns its result, awaited when it is a promise. The function
 * is sent as its text: it uses nothing from the module that defines it.
 *
 * @param {{ send: Function }} page Page.
 * @param {Function} fn Function.
 * @param {...unknown} args Arguments: a function is passed as its text, any other value as JSON.
 */
export async function evaluate(page, fn, ...args) {
  const list = args.map((arg) => (typeof arg === 'function' ? `(${arg})` : JSON.stringify(arg)))
  const answer = await page.send('Runtime.evaluate', {
    expression: `(${fn})(${list.join(', ')})`,
    awaitPromise: true,
    returnByValue: true,
  })
  if (answer.exceptionDetails) {
    const { exception, text } = answer.exceptionDetails
    throw new Error(exception?.description ?? text)
  }
  return answer.result.value
}

/**
 * Loads an address in the page and waits for its load.
 *
 * @param {{ send: Function, next: Function }} page Page.
 * @param {string} url Address.
 */
export async function navigate(page, url) {
  const loaded = page.next('Page.loadEventFired')
  await page.send('Page.navigate', { url })
  await loaded
}
