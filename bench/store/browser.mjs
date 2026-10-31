/**
 * Code run in the browser by the benchmark of the store of the results (`run.mjs`). Each function
 * is sent as its text, over the DevTools protocol or as the source of a Worker: it uses nothing
 * from this module, and receives what it needs as arguments. Instants are given in milliseconds
 * since 1970 (`performance.timeOrigin + performance.now()`), to compare those of the page, of the
 * Worker and of the samples of the processor time of Prisme.
 */

/**
 * Receives a result in the page, over the WebSocket of a session, and measures its way. Two
 * handlings:
 *
 * - `discard`: a message of rows is only recognised by its start, never parsed, and its rows are
 *   thrown away: the cost of the server and of the transport alone;
 * - `page`: as `web/src/api/querySession.ts` does without the interface, each message is parsed
 *   and its size counted, then the messages waiting are applied every 100 ms, the rows pushed
 *   into one array.
 *
 * The rows stay in `window.benchRows`, for the measure of the memory of the page.
 *
 * @param {(text: string) => number} utf8Length Length in UTF-8 of a text.
 * @param {string} host Host of Prisme, such as `127.0.0.1:18484`.
 * @param {number} editor Id of the editor.
 * @param {string} sql Query.
 * @param {'discard' | 'page'} mode Handling of the messages.
 */
export function receiveInPage(utf8Length, host, editor, sql, mode) {
  const clock = () => performance.timeOrigin + performance.now()
  const longTasks = []
  const observer = new PerformanceObserver((list) => {
    for (const entry of list.getEntries()) {
      longTasks.push(entry.duration)
    }
  })
  observer.observe({ type: 'longtask' })
  return new Promise((resolve, reject) => {
    const socket = new WebSocket(`ws://${host}/api/editors/${editor}/session`)
    const rows = []
    window.benchRows = rows
    let waiting = []
    let timer
    let sent = 0
    let firstRows = 0
    let lastRows = 0
    let handling = 0
    let batches = 0
    let bytes = 0

    function flush() {
      const started = clock()
      timer = undefined
      for (const message of waiting) {
        if (message.type === 'rows') {
          rows.push(...message.rows)
        }
      }
      waiting = []
      handling += clock() - started
    }

    function end(message, now) {
      clearTimeout(timer)
      flush()
      socket.close()
      observer.disconnect()
      resolve({
        epoch: { sent, last_rows: lastRows, done: now },
        first_rows_s: (firstRows - sent) / 1000,
        rows_s: (lastRows - sent) / 1000,
        done_s: (now - sent) / 1000,
        rows: mode === 'page' ? rows.length : message.row_count,
        batches,
        bytes,
        handling_s: handling / 1000,
        long_tasks: {
          count: longTasks.length,
          total_s: longTasks.reduce((total, duration) => total + duration, 0) / 1000,
          longest_s: Math.max(0, ...longTasks) / 1000,
        },
      })
    }

    socket.addEventListener('error', () => reject(new Error('WebSocket error')))
    socket.addEventListener('message', (event) => {
      const now = clock()
      const data = event.data
      if (mode === 'discard' && data.startsWith('{"type":"rows"')) {
        batches++
        firstRows ||= now
        lastRows = now
        handling += clock() - now
        return
      }
      const message = JSON.parse(data)
      if (message.type === 'rows') {
        batches++
        firstRows ||= now
        lastRows = now
      }
      if (mode === 'page') {
        bytes += utf8Length(data)
        waiting.push(message)
        timer ??= setTimeout(flush, 100)
      }
      handling += clock() - now
      switch (message.type) {
        case 'ready':
          sent = clock()
          socket.send(JSON.stringify({ type: 'run', sql }))
          break
        case 'error':
          socket.close()
          reject(new Error(JSON.stringify(message.detail)))
          break
        case 'done':
          end(message, now)
          break
      }
    })
  })
}

/**
 * Body of the Worker that holds the rows: it receives the result over the WebSocket of a session
 * and keeps its cells in columns, as the memory of a WebAssembly module would hold them (the
 * layout of `bench/pivot/js/transfer.mjs`: the UTF-8 bytes of the cells end to end, an offset per
 * cell, a validity byte per row). It tells the page the number of rows received after each batch,
 * and answers the windows of visible rows that the page asks for.
 *
 * Messages of the page: `{ type: 'start', host, editor, sql }`, then
 * `{ type: 'window', start, count }`. Messages to the page: `{ type: 'rows', received }`,
 * `{ type: 'window', rows }`, `{ type: 'done', ... }` with the measures, or
 * `{ type: 'error', message }`.
 *
 * @param {(text: string) => number} utf8Length Length in UTF-8 of a text.
 */
export function workerMain(utf8Length) {
  const clock = () => performance.timeOrigin + performance.now()
  const encoder = new TextEncoder()
  const decoder = new TextDecoder()
  // Columns of the store: bytes, length of the bytes used, offsets, validities.
  let columns = []
  let count = 0
  let capacity = 0

  // Room for `needed` more bytes in a column, the bytes copied into a larger area if needed.
  function reserve(column, needed) {
    if (column.length + needed <= column.bytes.length) {
      return
    }
    const bytes = new Uint8Array(Math.max(column.bytes.length * 2, column.length + needed))
    bytes.set(column.bytes.subarray(0, column.length))
    column.bytes = bytes
  }

  // Room for the offsets and validities of `rows` more rows.
  function reserveRows(rows) {
    if (count + rows + 1 <= capacity) {
      return
    }
    capacity = Math.max(capacity * 2, count + rows + 1)
    for (const column of columns) {
      const offsets = new Uint32Array(capacity)
      offsets.set(column.offsets.subarray(0, count + 1))
      column.offsets = offsets
      const valid = new Uint8Array(capacity)
      valid.set(column.valid.subarray(0, count))
      column.valid = valid
    }
  }

  // Appends a batch of rows: the cells of each column joined and encoded in one call.
  function append(rows) {
    reserveRows(rows.length)
    const parts = []
    for (let index = 0; index < columns.length; index++) {
      const column = columns[index]
      const { offsets, valid } = column
      const start = column.length
      let position = start
      parts.length = 0
      for (let row = 0; row < rows.length; row++) {
        const text = rows[row][index]
        offsets[count + row] = position
        if (text === null) {
          valid[count + row] = 0
        } else {
          valid[count + row] = 1
          parts.push(text)
          position += text.length
        }
      }
      let rest = parts.join('')
      const units = rest.length
      reserve(column, units)
      for (;;) {
        const { read, written } = encoder.encodeInto(rest, column.bytes.subarray(column.length))
        column.length += written
        if (read === rest.length) {
          break
        }
        rest = rest.slice(read)
        reserve(column, rest.length * 3)
      }
      if (column.length - start !== units) {
        // Not ASCII: the offsets in bytes are counted again, cell by cell.
        position = start
        for (let row = 0; row < rows.length; row++) {
          const text = rows[row][index]
          offsets[count + row] = position
          if (text !== null) {
            position += utf8Length(text)
          }
        }
      }
      offsets[count + rows.length] = column.length
    }
    count += rows.length
  }

  // Rows `start` to `start + length` (excluded), decoded column by column.
  function visible(start, length) {
    const end = Math.min(count, start + length)
    const rows = Array.from({ length: end - start }, () => new Array(columns.length))
    for (let index = 0; index < columns.length; index++) {
      const { bytes, offsets, valid } = columns[index]
      const from = offsets[start]
      const text = decoder.decode(bytes.subarray(from, offsets[end]))
      // In ASCII, the offsets in bytes are those in UTF-16 units.
      const ascii = text.length === offsets[end] - from
      for (let row = start; row < end; row++) {
        rows[row - start][index] =
          valid[row] === 0
            ? null
            : ascii
              ? text.slice(offsets[row] - from, offsets[row + 1] - from)
              : decoder.decode(bytes.subarray(offsets[row], offsets[row + 1]))
      }
    }
    return rows
  }

  function receive({ host, editor, sql }) {
    const socket = new WebSocket(`ws://${host}/api/editors/${editor}/session`)
    let sent = 0
    let firstRows = 0
    let lastRows = 0
    let parsing = 0
    let storing = 0
    socket.addEventListener('error', () =>
      postMessage({ type: 'error', message: 'WebSocket error' }),
    )
    socket.addEventListener('message', (event) => {
      const now = clock()
      const message = JSON.parse(event.data)
      parsing += clock() - now
      switch (message.type) {
        case 'ready':
          sent = clock()
          socket.send(JSON.stringify({ type: 'run', sql }))
          break
        case 'columns':
          columns = message.columns.map(() => ({
            bytes: new Uint8Array(65_536),
            length: 0,
            offsets: new Uint32Array(1),
            valid: new Uint8Array(0),
          }))
          capacity = 1
          break
        case 'rows': {
          firstRows ||= now
          lastRows = now
          const started = clock()
          append(message.rows)
          storing += clock() - started
          postMessage({ type: 'rows', received: count })
          break
        }
        case 'error':
          postMessage({ type: 'error', message: JSON.stringify(message.detail) })
          break
        case 'done': {
          socket.close()
          const used = columns.reduce((total, column) => total + column.length, 0)
          const reserved = columns.reduce(
            (total, column) =>
              total + column.bytes.length + column.offsets.byteLength + column.valid.byteLength,
            0,
          )
          postMessage({
            type: 'done',
            epoch: { sent, last_rows: lastRows, done: now },
            first_rows_s: (firstRows - sent) / 1000,
            rows_s: (lastRows - sent) / 1000,
            done_s: (now - sent) / 1000,
            rows: count,
            parsing_s: parsing / 1000,
            storing_s: storing / 1000,
            store: { used: used + columns.length * (count * 5 + 4), reserved },
          })
          break
        }
      }
    })
  }

  addEventListener('message', ({ data }) => {
    if (data.type === 'start') {
      receive(data)
    } else {
      postMessage({ type: 'window', rows: visible(data.start, data.count) })
    }
  })
}

/**
 * Starts a Worker that receives a result (`workerMain`), and asks it, at each frame, for a window
 * of visible rows at a random place among those received, one window at a time: 50 rows during
 * the reception, then, once it is done, 50 rows then 1,000 rows for `idleMs` each. Measures the
 * delay between the request and the rows received by the page.
 *
 * @param {string} workerSource Source of the Worker.
 * @param {string} host Host of Prisme, such as `127.0.0.1:18484`.
 * @param {number} editor Id of the editor.
 * @param {string} sql Query.
 * @param {number} idleMs Duration of each phase once the result is received, in milliseconds.
 */
export function receiveInWorker(workerSource, host, editor, sql, idleMs) {
  const clock = () => performance.timeOrigin + performance.now()
  const longTasks = []
  const observer = new PerformanceObserver((list) => {
    for (const entry of list.getEntries()) {
      longTasks.push(entry.duration)
    }
  })
  observer.observe({ type: 'longtask' })
  const url = URL.createObjectURL(new Blob([workerSource], { type: 'text/javascript' }))
  const worker = new Worker(url)
  return new Promise((resolve, reject) => {
    let received = 0
    let result = null
    let request = null
    const samples = []
    let phase = 'reception'
    let phaseEnd = 0
    let frames = 0
    // Places of the windows drawn from a fixed seed, the same from one run to the next.
    let seed = 42
    const random = () => (seed = (seed * 1_103_515_245 + 12_345) % 2_147_483_648) / 2_147_483_648

    function statistics(delays) {
      const sorted = [...delays].sort((a, b) => a - b)
      const at = (share) => sorted[Math.min(sorted.length - 1, Math.floor(share * sorted.length))]
      return sorted.length === 0
        ? { windows: 0 }
        : {
            windows: sorted.length,
            median_ms: at(0.5),
            p95_ms: at(0.95),
            p99_ms: at(0.99),
            max_ms: sorted[sorted.length - 1],
          }
    }

    function finish() {
      observer.disconnect()
      worker.terminate()
      // A window asked after the last rows waits for no batch: it goes with the idle Worker.
      const phases = { reception: [], 'idle 50': [], 'idle 1000': [] }
      for (const [name, sent, delay] of samples) {
        phases[name === 'reception' && sent > result.epoch.last_rows ? 'idle 50' : name].push(delay)
      }
      resolve({
        ...result,
        frames,
        windows: Object.fromEntries(
          Object.entries(phases).map(([name, delays]) => [name, statistics(delays)]),
        ),
        long_tasks: {
          count: longTasks.length,
          total_s: longTasks.reduce((total, duration) => total + duration, 0) / 1000,
          longest_s: Math.max(0, ...longTasks) / 1000,
        },
      })
    }

    function frame() {
      frames++
      const now = performance.now()
      if (phase !== 'reception' && now > phaseEnd) {
        if (phase === 'idle 1000') {
          finish()
          return
        }
        phase = 'idle 1000'
        phaseEnd = now + idleMs
      }
      const size = phase === 'idle 1000' ? 1000 : 50
      if (request === null && received >= size) {
        const start = Math.floor(random() * (received - size + 1))
        request = { phase, size, sent: clock() }
        worker.postMessage({ type: 'window', start, count: size })
      }
      requestAnimationFrame(frame)
    }

    worker.addEventListener('error', (event) => reject(new Error(event.message)))
    worker.addEventListener('message', ({ data }) => {
      switch (data.type) {
        case 'rows':
          received = data.received
          break
        case 'window':
          if (data.rows.length !== request.size) {
            reject(new Error(`window of ${data.rows.length} rows, ${request.size} asked`))
          }
          samples.push([request.phase, request.sent, clock() - request.sent])
          request = null
          break
        case 'done':
          result = data
          phase = 'idle 50'
          phaseEnd = performance.now() + idleMs
          break
        case 'error':
          reject(new Error(data.message))
          break
      }
    })
    worker.postMessage({ type: 'start', host, editor, sql })
    requestAnimationFrame(frame)
  })
}

/**
 * Probe of the interface, installed before its scripts: notes when the session is ready, when
 * the query leaves, when the first and the last rows arrive and when `done` arrives, from the
 * start of each message, and collects the long tasks. The probe is `window.benchProbe`.
 */
export function interfaceProbe() {
  const probe = { ready: null, sent: null, firstRows: null, lastRows: null, done: null }
  probe.longTasks = []
  window.benchProbe = probe
  new PerformanceObserver((list) => {
    for (const entry of list.getEntries()) {
      probe.longTasks.push([entry.startTime, entry.duration])
    }
  }).observe({ type: 'longtask' })
  const Native = window.WebSocket
  window.WebSocket = class extends Native {
    constructor(...args) {
      super(...args)
      this.addEventListener('message', (event) => {
        const now = performance.now()
        const data = event.data
        if (typeof data !== 'string') {
          return
        }
        if (data.startsWith('{"type":"rows"')) {
          probe.firstRows ??= now
          probe.lastRows = now
        } else if (data.startsWith('{"type":"ready"')) {
          probe.ready ??= now
        } else if (data.startsWith('{"type":"done"')) {
          probe.done = now
        }
      })
    }

    send(data) {
      if (typeof data === 'string' && data.startsWith('{"type":"run"')) {
        probe.sent ??= performance.now()
      }
      super.send(data)
    }
  }
}
