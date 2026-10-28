// Retakes the screenshots of the design brief (`design/screenshots/`).
// Starts Prisme on a throwaway data folder, then a headless Chrome at a
// scale of 150 % on a throwaway profile; prepares two connections and two
// editors through the API, runs their queries, opens the tree down to the
// columns of `customer`, then takes the main screen and the data explorer,
// each in light and dark. Stops both programs and removes their folders at
// the end, even after a failure. Prerequisites in `design/README.md`.
//
// From the root of the repository: node design/capture/capture.mjs [folder]

import { spawn } from 'node:child_process'
import { mkdirSync, mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { closeBrowser, connect } from './devtools.mjs'

/**
 * Root of the repository.
 */
const ROOT = fileURLToPath(new URL('../..', import.meta.url))

/**
 * Folder of the screenshots: the first argument, or `design/screenshots`.
 */
const OUTPUT = resolve(process.argv[2] ?? join(ROOT, 'design', 'screenshots'))

/**
 * Executables of Prisme (debug build, which serves `web/dist` from the disk)
 * and of Chrome, unless `PRISME_BIN` or `CHROME_BIN` name others.
 */
const WINDOWS = process.platform === 'win32'
const PRISME_BIN =
  process.env.PRISME_BIN ?? join(ROOT, 'target', 'debug', WINDOWS ? 'prisme.exe' : 'prisme')
const CHROME_BIN =
  process.env.CHROME_BIN ??
  (WINDOWS ? 'C:/Program Files/Google/Chrome/Application/chrome.exe' : 'google-chrome')

/**
 * Ports of Prisme and of Chrome's DevTools, away from the usual 8484.
 */
const PRISME_PORT = 8485
const DEVTOOLS_PORT = 9333
const PRISME = `http://127.0.0.1:${PRISME_PORT}`

/**
 * Window of a 2560 × 1440 screen at 150 %, in full screen, in CSS pixels.
 */
const WINDOW = { width: 1707, height: 1060 }

/**
 * Servers of the test databases (`design/README.md`).
 */
const MARIADB_PORT = 3400
const MYSQL_PORT = 3401
const PASSWORD = 'design'

/**
 * Starts a program.
 *
 * @param {string} bin Executable.
 * @param {string[]} args Arguments.
 * @param {import('node:child_process').StdioOptions} stdio Standard streams.
 * @returns {Promise<import('node:child_process').ChildProcess>}
 * @throws {Error} The executable cannot be started.
 */
function start(bin, args, stdio) {
  const program = spawn(bin, args, { stdio })
  return new Promise((resolve, reject) => {
    program.once('spawn', () => resolve(program))
    program.once('error', reject)
  })
}

/**
 * Waits until an address answers.
 *
 * @param {string} url Address polled.
 * @param {import('node:child_process').ChildProcess} program Program that should answer.
 * @param {number} timeout Longest wait, in milliseconds.
 */
async function answering(url, program, timeout = 20000) {
  const end = Date.now() + timeout
  while (Date.now() < end) {
    if (program.exitCode !== null) {
      throw new Error(`${program.spawnfile} stopped, code ${program.exitCode}`)
    }
    try {
      if ((await fetch(url)).ok) {
        return
      }
    } catch {
      // Not listening yet.
    }
    await new Promise((resolve) => setTimeout(resolve, 200))
  }
  throw new Error(`no answer from ${url}`)
}

/**
 * Calls the API of Prisme.
 *
 * @param {string} method HTTP method.
 * @param {string} path Path of the API.
 * @param {object} [body] Body, sent as JSON.
 */
async function api(method, path, body) {
  const response = await fetch(`${PRISME}${path}`, {
    method,
    headers: body === undefined ? {} : { 'Content-Type': 'application/json' },
    body: body === undefined ? undefined : JSON.stringify(body),
  })
  const text = await response.text()
  if (!response.ok) {
    throw new Error(`${method} ${path}: ${response.status} ${text}`)
  }
  return text === '' ? null : JSON.parse(text)
}

/**
 * Prepares the state of the screenshots: the connections "local" (MariaDB)
 * and "prod" (MySQL), the editor "boutique" with two queries, and an editor
 * without a name on "prod", whose query names a table that does not exist.
 */
async function prepare() {
  const local = await api('POST', '/api/connections', {
    name: 'local',
    host: '127.0.0.1',
    port: MARIADB_PORT,
    username: 'root',
    password: PASSWORD,
    database_name: 'shop',
    init_sql: 'SET NAMES utf8mb4',
    show_system_schemas: false,
  })
  const prod = await api('POST', '/api/connections', {
    name: 'prod',
    host: '127.0.0.1',
    port: MYSQL_PORT,
    username: 'root',
    password: PASSWORD,
    database_name: 'billing',
    show_system_schemas: false,
  })
  const boutique = await api('POST', '/api/editors', { connection_id: local.id })
  await api('PUT', `/api/editors/${boutique.id}`, {
    name: 'boutique',
    connection_id: local.id,
    sql: [
      'SELECT c.id, c.name, c.email, c.country, c.website, c.attributes, c.created_at',
      'FROM customer AS c',
      "WHERE c.created_at >= '2026-01-01'",
      'ORDER BY c.created_at DESC;',
      '',
      'SELECT country, COUNT(*) AS customers',
      'FROM customer',
      'GROUP BY country;',
      '',
    ].join('\n'),
  })
  const failing = await api('POST', '/api/editors', { connection_id: prod.id })
  await api('PUT', `/api/editors/${failing.id}`, {
    name: '',
    connection_id: prod.id,
    sql: "SELECT number, issued_on, amount, status\nFROM invoices\nWHERE status = 'late';\n",
  })
}

/**
 * Clicks on a button of the editor shown, once it can be pressed: the
 * buttons of an editor wait for its session to be connected.
 *
 * @param {Awaited<ReturnType<typeof connect>>} page Page of Prisme.
 * @param {string} text Text of the button.
 */
async function press(page, text) {
  await page.until(`[...document.querySelectorAll('button')].some(
    (button) =>
      button.textContent.trim() === ${JSON.stringify(text)} &&
      button.getBoundingClientRect().width > 0 &&
      !button.disabled,
  )`)
  await page.clickOn('button', text, { exact: true })
}

/**
 * Brings the page to the main screen: the query of "prod" failed, both
 * queries of "boutique" run, the tree open down to the columns of
 * `customer`, the first result shown.
 *
 * @param {Awaited<ReturnType<typeof connect>>} page Page of Prisme.
 */
async function mainScreen(page) {
  await page.resize(WINDOW.width, WINDOW.height)
  await page.send('Page.reload')
  await page.wait(1500)
  await page.until(`document.querySelector('[role="treeitem"]') !== null`)

  await page.clickOn('[role="tab"]', 'prod', { exact: true })
  await press(page, 'Exécuter')
  await page.until(`document.querySelector('.progress-dot.error') !== null`)

  await page.clickOn('[role="tab"]', 'boutique', { exact: true })
  await press(page, 'Exécuter tout')
  await page.until(`document.querySelectorAll('.progress-dot.done').length >= 2`)

  for (const label of ['shop', 'Tables (4)', 'customer', 'Colonnes (7)']) {
    await page.clickOn('[role="treeitem"]', label, { exact: true })
    await page.wait(700)
  }
  await page.clickOn('[role="tab"]', '1 · SELECT c.id')
  await page.wait(500)
}

/**
 * Opens the data explorer on the `attributes` of the first four rows of the
 * grid: a click on the first cell, then Shift + click on the fourth.
 *
 * @param {Awaited<ReturnType<typeof connect>>} page Page of Prisme.
 */
async function explorerScreen(page) {
  const [first, last] = await page.evaluate(`(() => {
    const scroller = [...document.querySelectorAll('.grid-scroller')].find(
      (element) => element.getBoundingClientRect().width > 0,
    )
    const names = [...scroller.querySelectorAll('.grid-column-label')].map((label) => label.textContent.trim())
    const column = names.indexOf('attributes')
    const rows = [...scroller.querySelectorAll('.grid-body .grid-row')]
    const center = (number) => {
      const row = rows.find((item) => item.querySelector('.grid-row-header').textContent.trim() === String(number))
      const rect = row.children[1 + column].getBoundingClientRect()
      return { x: rect.x + rect.width / 2, y: rect.y + rect.height / 2 }
    }
    return [center(1), center(4)]
  })()`)
  await page.click(first.x, first.y)
  await page.click(last.x, last.y, { modifiers: 8 })
  await page.wait(300)
  await page.clickOn('button', 'Déplier : Explorateur de données')
  await page.wait(1500)
}

/**
 * Takes the screen in light and dark, the mouse out of the way.
 *
 * @param {Awaited<ReturnType<typeof connect>>} page Page of Prisme.
 * @param {string} name Start of the files' names.
 */
async function shots(page, name) {
  for (const scheme of ['light', 'dark']) {
    await page.scheme(scheme)
    await page.move(1000, 8)
    await page.wait(800)
    const path = join(OUTPUT, `${name}-${scheme}.png`)
    await page.screenshot(path)
    console.log(`written: ${path}`)
  }
  await page.scheme('light')
}

/**
 * Stops a program and waits for its end.
 *
 * @param {import('node:child_process').ChildProcess} program Program.
 */
async function stop(program) {
  if (program.exitCode !== null || program.signalCode !== null) {
    return
  }
  const ended = new Promise((resolve) => program.once('exit', resolve))
  program.kill()
  await ended
}

mkdirSync(OUTPUT, { recursive: true })
const dataDir = mkdtempSync(join(tmpdir(), 'prisme-design-data-'))
const profileDir = mkdtempSync(join(tmpdir(), 'prisme-design-chrome-'))
let prisme = null
let chrome = null
try {
  // Without a terminal, Prisme takes its first-launch defaults: the user
  // me@local, hence the avatar "M".
  prisme = await start(
    PRISME_BIN,
    ['--listen', `127.0.0.1:${PRISME_PORT}`, '--data-dir', dataDir],
    ['ignore', 'inherit', 'inherit'],
  )
  await answering(`${PRISME}/api/editors`, prisme)
  await prepare()

  chrome = await start(
    CHROME_BIN,
    [
      '--headless=new',
      `--remote-debugging-port=${DEVTOOLS_PORT}`,
      `--user-data-dir=${profileDir}`,
      '--force-device-scale-factor=1.5',
      `--window-size=${WINDOW.width},${WINDOW.height}`,
      '--no-first-run',
      '--no-default-browser-check',
      '--lang=fr-FR',
      PRISME,
    ],
    'ignore',
  )
  await answering(`http://127.0.0.1:${DEVTOOLS_PORT}/json/version`, chrome)
  const page = await connect(DEVTOOLS_PORT)
  try {
    await mainScreen(page)
    await shots(page, 'main')
    await explorerScreen(page)
    await shots(page, 'explorer')
  } finally {
    page.close()
  }
} finally {
  if (chrome !== null) {
    await closeBrowser(DEVTOOLS_PORT).catch(() => {
      // Chrome did not start: nothing to close.
    })
    await stop(chrome)
  }
  if (prisme !== null) {
    await stop(prisme)
  }
  rmSync(profileDir, { recursive: true, force: true, maxRetries: 10, retryDelay: 300 })
  rmSync(dataDir, { recursive: true, force: true, maxRetries: 10, retryDelay: 300 })
}
