// Small driver of a Chrome started with a remote debugging port, through
// the DevTools protocol: it plays the steps of a scenario on the page
// (clicks, keys, color scheme) and takes its screenshots. Node 22 or later
// (global `fetch` and `WebSocket`), without dependencies.

import { writeFileSync } from 'node:fs'

/**
 * Opens a DevTools session on a target.
 *
 * @param {string} url WebSocket address of the target.
 * @returns {Promise<{ send: (method: string, params?: object) => Promise<any>, close: () => void }>}
 */
async function session(url) {
  const socket = new WebSocket(url)
  await new Promise((resolve, reject) => {
    socket.onopen = resolve
    socket.onerror = reject
  })
  let next = 0
  const pending = new Map()
  socket.onmessage = (event) => {
    const message = JSON.parse(event.data)
    if (message.id === undefined || !pending.has(message.id)) {
      return
    }
    const { resolve, reject } = pending.get(message.id)
    pending.delete(message.id)
    if (message.error) {
      reject(new Error(`${message.error.message} (${message.error.code})`))
    } else {
      resolve(message.result)
    }
  }
  const send = (method, params = {}) =>
    new Promise((resolve, reject) => {
      const id = ++next
      pending.set(id, { resolve, reject })
      socket.send(JSON.stringify({ id, method, params }))
    })
  return { send, close: () => socket.close() }
}

/**
 * Closes the whole browser.
 *
 * @param {number} port Remote debugging port of the browser.
 */
export async function closeBrowser(port) {
  const { webSocketDebuggerUrl } = await (
    await fetch(`http://127.0.0.1:${port}/json/version`)
  ).json()
  const browser = await session(webSocketDebuggerUrl)
  await browser.send('Browser.close').catch(() => {
    // The browser may close before it answers.
  })
  browser.close()
}

/**
 * Connects to the first page of the browser and returns the helpers of a
 * scenario.
 *
 * @param {number} port Remote debugging port of the browser.
 */
export async function connect(port) {
  const targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json()
  const target = targets.find((item) => item.type === 'page')
  const { send, close } = await session(target.webSocketDebuggerUrl)

  /**
   * Waits a number of milliseconds.
   *
   * @param {number} ms Duration.
   */
  const wait = (ms) => new Promise((resolve) => setTimeout(resolve, ms))

  /**
   * Evaluates an expression in the page and returns its value.
   *
   * @param {string} expression JavaScript expression.
   */
  async function evaluate(expression) {
    const result = await send('Runtime.evaluate', {
      expression,
      awaitPromise: true,
      returnByValue: true,
    })
    if (result.exceptionDetails) {
      throw new Error(result.exceptionDetails.exception?.description ?? expression)
    }
    return result.result.value
  }

  /**
   * Center of the first visible element matching `selector` whose text or
   * accessible name (`aria-label`) contains `text`, or whose text is
   * `text` with `exact`.
   *
   * @param {string} selector CSS selector.
   * @param {string} text Text looked for.
   * @param {{ exact?: boolean }} options
   * @returns {Promise<{ x: number, y: number }>}
   */
  async function find(selector, text = '', { exact = false } = {}) {
    const point = await evaluate(`(() => {
      const wanted = ${JSON.stringify(text)}
      for (const element of document.querySelectorAll(${JSON.stringify(selector)})) {
        const content = (element.textContent ?? '').trim()
        const matches = ${exact}
          ? content === wanted
          : (content + ' ' + (element.getAttribute('aria-label') ?? '')).includes(wanted)
        const rect = element.getBoundingClientRect()
        if (matches && rect.width > 0 && rect.height > 0) {
          return { x: rect.x + rect.width / 2, y: rect.y + rect.height / 2 }
        }
      }
      return null
    })()`)
    if (point === null) {
      throw new Error(`not found: ${selector} "${text}"`)
    }
    return point
  }

  /**
   * Moves the mouse.
   *
   * @param {number} x Horizontal position, in CSS pixels.
   * @param {number} y Vertical position, in CSS pixels.
   */
  async function move(x, y) {
    await send('Input.dispatchMouseEvent', { type: 'mouseMoved', x, y })
  }

  /**
   * Clicks with the left button.
   *
   * @param {number} x Horizontal position, in CSS pixels.
   * @param {number} y Vertical position, in CSS pixels.
   * @param {{ modifiers?: number }} options
   *     Keys held: 1 Alt, 2 Ctrl, 4 Meta, 8 Shift, added up.
   */
  async function click(x, y, { modifiers = 0 } = {}) {
    await move(x, y)
    const common = { x, y, button: 'left', modifiers, clickCount: 1 }
    await send('Input.dispatchMouseEvent', { type: 'mousePressed', buttons: 1, ...common })
    await send('Input.dispatchMouseEvent', { type: 'mouseReleased', buttons: 0, ...common })
  }

  /**
   * Clicks on an element found by `find`.
   *
   * @param {string} selector CSS selector.
   * @param {string} text Text looked for.
   * @param {{ exact?: boolean }} options
   */
  async function clickOn(selector, text = '', options = {}) {
    const { x, y } = await find(selector, text, options)
    await click(x, y)
  }

  /**
   * Waits until an expression of the page is true.
   *
   * @param {string} expression JavaScript expression.
   * @param {number} timeout Longest wait, in milliseconds.
   */
  async function until(expression, timeout = 15000) {
    const end = Date.now() + timeout
    while (!(await evaluate(expression))) {
      if (Date.now() > end) {
        throw new Error(`timeout: ${expression}`)
      }
      await wait(200)
    }
  }

  /**
   * Gives the window of the page a size.
   *
   * @param {number} width Width, in CSS pixels.
   * @param {number} height Height, in CSS pixels.
   */
  async function resize(width, height) {
    const { windowId } = await send('Browser.getWindowForTarget')
    await send('Browser.setWindowBounds', { windowId, bounds: { width, height } })
  }

  /**
   * Emulates the color scheme of the system.
   *
   * @param {'light' | 'dark'} value Color scheme.
   */
  async function scheme(value) {
    await send('Emulation.setEmulatedMedia', {
      features: [{ name: 'prefers-color-scheme', value }],
    })
  }

  /**
   * Saves a screenshot of the page as PNG.
   *
   * @param {string} path File written.
   */
  async function screenshot(path) {
    const { data } = await send('Page.captureScreenshot', { format: 'png' })
    writeFileSync(path, Buffer.from(data, 'base64'))
  }

  return {
    send,
    wait,
    evaluate,
    find,
    move,
    click,
    clickOn,
    until,
    resize,
    scheme,
    screenshot,
    close,
  }
}
