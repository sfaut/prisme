/**
 * Profile of the interface of Prisme while it receives a result: the functions of the page that
 * hold its main thread, and the composited layers it draws. Complements `trace.mjs`, which tells
 * the kinds of work (scripts, microtasks of Vue, layers, painting), with the functions behind
 * them.
 *
 * Builds the interface without minification, to keep the names of its functions, into
 * `target/store-bench/web-plain`, and gives its scripts to Chrome in place of those embedded in
 * Prisme (`Fetch` domain of the DevTools protocol): the page asks for the entry script named by
 * the document of Prisme, and receives that of the plain build, which loads its own modules. The
 * document, its styles, the API and the WebSocket stay those of the server: a document given by
 * the protocol opens no WebSocket.
 *
 * Starts Prisme and Chrome as `run.mjs` does, opens the interface on an editor that holds the
 * query of that many rows, then samples the JavaScript of the page from Ctrl+Enter to `done` and
 * the drawing that follows; two seconds after the first rows, it reads the composited layers of
 * the page once.
 *
 * From the root of the repository, as for `run.mjs`: `node bench/store/profile.mjs [rows]`
 * (1,000,000 rows by default). The profile goes to `target/store-bench/profile-<rows>.cpuprofile`,
 * which the Performance panel of DevTools opens.
 */

import { spawnSync } from 'node:child_process'
import { existsSync, readFileSync, writeFileSync } from 'node:fs'
import { basename, extname, join } from 'node:path'
import { ADDRESS, ROOT, build, query, start } from '../prisme.mjs'
import { evaluate, launch, pause } from './cdp.mjs'
import { openInterface, runInInterface } from './interface.mjs'

/**
 * Interval between two samples of the JavaScript of the page, in microseconds.
 */
const SAMPLING_US = 200

/**
 * Time left to the interface after `done`, to apply its last batch and draw.
 */
const SETTLE_MS = 2_000

/**
 * Functions shown in each table.
 */
const SHOWN = 25

/**
 * Types of the files of the interface, by extension.
 */
const TYPES = {
  '.html': 'text/html',
  '.js': 'text/javascript',
  '.css': 'text/css',
  '.ico': 'image/x-icon',
  '.svg': 'image/svg+xml',
  '.woff2': 'font/woff2',
}

const workDir = join(ROOT, 'target', 'store-bench')
const plainDir = join(workDir, 'web-plain')
const count = Number(process.argv[2] ?? 1_000_000)

/**
 * Builds the interface without minification into `plainDir`. Exits when the build fails.
 */
function buildPlain() {
  const vite = spawnSync(
    process.execPath,
    [
      join(ROOT, 'web', 'node_modules', 'vite', 'bin', 'vite.js'),
      'build',
      '--minify',
      'false',
      '--outDir',
      plainDir,
      '--emptyOutDir',
    ],
    { cwd: join(ROOT, 'web'), stdio: 'inherit' },
  )
  if (vite.status !== 0) {
    process.exit(vite.status ?? 1)
  }
}

/**
 * Path of the entry script named by the document of a build, such as `assets/index-B3uHSGgs.js`.
 *
 * @param {string} dir Folder of the build.
 */
function entryScript(dir) {
  return readFileSync(join(dir, 'index.html'), 'utf8').match(/src="\/(assets\/[^"]+\.js)"/)[1]
}

/**
 * Gives the page the scripts of `plainDir` in place of those of Prisme: the entry script of
 * Prisme is answered with that of `plainDir`; the other files of `assets/` come from `plainDir`
 * when it holds them, from the server otherwise.
 *
 * @param {{ send: Function, on: Function }} page Page.
 */
async function servePlain(page) {
  const embedded = entryScript(join(ROOT, 'web', 'dist'))
  const entry = entryScript(plainDir)
  page.on('Fetch.requestPaused', ({ requestId, request }) => {
    const path = new URL(request.url).pathname.slice(1)
    const file = join(plainDir, path === embedded ? entry : path)
    const type = TYPES[extname(file)] ?? 'application/octet-stream'
    const answer = existsSync(file)
      ? page.send('Fetch.fulfillRequest', {
          requestId,
          responseCode: 200,
          responseHeaders: [{ name: 'Content-Type', value: type }],
          body: readFileSync(file).toString('base64'),
        })
      : page.send('Fetch.continueRequest', { requestId })
    answer.catch((error) => console.error(`profile: ${path}: ${error.message}`))
  })
  await page.send('Fetch.enable', {
    patterns: [{ urlPattern: `http://${ADDRESS}/assets/*`, requestStage: 'Request' }],
  })
}

/**
 * Composited layers of the page, by element that owns them: tag and first class.
 *
 * @param {{ send: Function, next: Function }} page Page.
 * @returns {Promise<[string, { layers: number, drawing: number }][]>} Owners, the most layers
 *   first; `drawing` counts the layers that paint content.
 */
async function layersByOwner(page) {
  await page.send('DOM.enable')
  const changed = page.next('LayerTree.layerTreeDidChange')
  await page.send('LayerTree.enable')
  // A scroll of the grid makes a frame, whose layers the event gives.
  await evaluate(page, () => {
    document.querySelector('.grid-scroller').scrollTop += 220
  })
  const { layers = [] } = await Promise.race([changed, pause(5_000).then(() => ({}))])
  await page.send('LayerTree.disable')
  const owners = new Map()
  for (const layer of layers) {
    let owner = '(no element)'
    if (layer.backendNodeId !== undefined) {
      try {
        const { node } = await page.send('DOM.describeNode', { backendNodeId: layer.backendNodeId })
        // Attributes come as a flat list: name, value, name, value...
        const attributes = node.attributes ?? []
        const at = attributes.indexOf('class')
        const first = at < 0 ? '' : attributes[at + 1].split(' ')[0]
        owner = `${node.nodeName.toLowerCase()}${first === '' ? '' : `.${first}`}`
      } catch {
        owner = '(element gone)'
      }
    }
    const entry = owners.get(owner) ?? { layers: 0, drawing: 0 }
    entry.layers++
    entry.drawing += layer.drawsContent ? 1 : 0
    owners.set(owner, entry)
  }
  await page.send('DOM.disable')
  return [...owners].sort((a, b) => b[1].layers - a[1].layers)
}

/**
 * Own time and total time of each function of a profile, in microseconds, idle time left out:
 * the busy time of the main thread, the work of the browser outside JavaScript (`(program)`) and
 * the garbage collection included.
 * The total time of a function holds that of the functions it calls, once per sample even when
 * it calls itself.
 *
 * @param {{ nodes: object[], samples: number[], timeDeltas: number[] }} profile Profile.
 * @returns {{ busy: number, own: Map<string, number>, total: Map<string, number> }}
 */
function functionTimes(profile) {
  const labels = new Map()
  const parents = new Map()
  for (const node of profile.nodes) {
    const { functionName, url, lineNumber } = node.callFrame
    const place = url === '' ? '' : ` ${basename(new URL(url).pathname)}:${lineNumber + 1}`
    labels.set(node.id, `${functionName || '(anonymous)'}${place}`)
    for (const child of node.children ?? []) {
      parents.set(child, node.id)
    }
  }
  const own = new Map()
  const total = new Map()
  let busy = 0
  profile.samples.forEach((id, index) => {
    // A sample lasts until the next one.
    const duration = profile.timeDeltas[index + 1] ?? 0
    const label = labels.get(id)
    if (label === '(idle)') {
      return
    }
    busy += duration
    own.set(label, (own.get(label) ?? 0) + duration)
    const seen = new Set()
    for (let at = id; at !== undefined; at = parents.get(at)) {
      const caller = labels.get(at)
      if (caller !== '(root)' && !seen.has(caller)) {
        seen.add(caller)
        total.set(caller, (total.get(caller) ?? 0) + duration)
      }
    }
  })
  return { busy, own, total }
}

/**
 * Prints the functions that take the most time, as a Markdown table.
 *
 * @param {string} title Title of the column of times.
 * @param {Map<string, number>} times Time of each function, in microseconds.
 * @param {number} busy Busy time of the main thread, in microseconds.
 */
function printTimes(title, times, busy) {
  console.log(`| Function | ${title} (s) | Share |`)
  console.log('| --- | ---: | ---: |')
  for (const [name, time] of [...times].sort((a, b) => b[1] - a[1]).slice(0, SHOWN)) {
    console.log(`| ${name} | ${(time / 1e6).toFixed(3)} | ${Math.round((100 * time) / busy)} % |`)
  }
}

buildPlain()
build()
const prisme = await start(join(workDir, `data-profile-${count}`))
const page = await launch(join(workDir, 'chrome'))
try {
  await servePlain(page)
  await openInterface(page, prisme.connection, query(count))
  await page.send('Profiler.enable')
  await page.send('Profiler.setSamplingInterval', { interval: SAMPLING_US })
  await page.send('Profiler.start')
  const running = runInInterface(page)
  while ((await evaluate(page, () => window.benchProbe.firstRows)) === null) {
    await pause(100)
  }
  await pause(2_000)
  const owners = await layersByOwner(page)
  await running
  await pause(SETTLE_MS)
  const { profile } = await page.send('Profiler.stop')
  const probe = await evaluate(page, () => window.benchProbe)
  const file = join(workDir, `profile-${count}.cpuprofile`)
  writeFileSync(file, JSON.stringify(profile))
  const { busy, own, total } = functionTimes(profile)
  console.log(`Profile of the interface: ${count.toLocaleString('en')} rows, ${page.browser}`)
  console.log(
    `Last rows received after ${((probe.lastRows - probe.sent) / 1000).toFixed(2)} s, done after ${((probe.done - probe.sent) / 1000).toFixed(2)} s; main thread busy ${(busy / 1e6).toFixed(2)} s`,
  )
  console.log()
  printTimes('Own time', own, busy)
  console.log()
  printTimes('Total time', total, busy)
  console.log()
  console.log('Composited layers, two seconds after the first rows:')
  console.log('| Element | Layers | Painting content |')
  console.log('| --- | ---: | ---: |')
  for (const [owner, { layers, drawing }] of owners) {
    console.log(`| ${owner} | ${layers} | ${drawing} |`)
  }
  console.log(`Profile: ${file}`)
} finally {
  page.close()
  prisme.stop()
}
