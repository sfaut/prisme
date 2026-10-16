/**
 * Query session on a saved connection, over a WebSocket.
 *
 * The client sends (`run`) the editor's whole text, with the cursor position
 * to run only the query found there (Ctrl+Enter), or without a cursor to run
 * them all in parallel (Ctrl+Shift+Enter). The server announces the queries
 * of the batch (`batch`), then streams their results, each message carrying
 * the number of its query: `started` when it starts (in parallel, it may wait
 * for its turn), `columns`, batches of `rows`, then `done` (or `error`).
 * `batch_done` closes the batch. `cancel` stops all the running queries,
 * `cancel_query` a single one. The protocol is described in
 * `crates/server/src/query_session/protocol.rs`.
 *
 * Each query has its tab. A new batch replaces the tabs of the previous one,
 * except those the user pinned; a tab can also be renamed or closed (its
 * query is then stopped if it is running).
 *
 * The session is that of an editor: the server saves its results as they
 * run, and gives each tab the id of its record. On opening, the session
 * takes back the saved tabs (`savedTab`), then opens the WebSocket. On the
 * server side, the session outlives the page for the delay set on its
 * connection: the WebSocket reattaches to it, and then receives its last
 * batch where it stands (`resume`, `resumedTab`), which the next messages
 * continue.
 *
 * `applyMessage` updates the state message by message; it depends neither on
 * Vue nor on the WebSocket, which makes it simple to test. `useQuerySession`
 * uses it to expose a reactive state to the components.
 */

import { markRaw, onScopeDispose, reactive } from 'vue'

import type { ColumnTypeInfo } from './generated/ColumnTypeInfo'
import type { LimitReport } from './generated/LimitReport'
import type { ClientMessage } from './generated/ClientMessage'
import type { ResultTab } from './generated/ResultTab'
import type { ResumedQuery } from './generated/ResumedQuery'
import type { ServerInfo } from './generated/ServerInfo'
import type { ServerMessage } from './generated/ServerMessage'

/**
 * Grouping delay of the received messages, in milliseconds: the interface is
 * redrawn at most once per delay, whatever the number of row batches
 * received, which keeps it responsive while running.
 */
const FLUSH_DELAY = 100

/**
 * Result cell: text, or `null` for NULL.
 */
export type Cell = string | null

/**
 * Stage of the session.
 *
 * - `connecting`: opening the WebSocket and connecting to the target server;
 * - `ready`: ready to run queries;
 * - `running`: a batch of queries is running;
 * - `closed`: WebSocket closed, the session must be reopened.
 */
export type SessionStatus = 'connecting' | 'ready' | 'running' | 'closed'

/**
 * Result of a query, completed as the messages arrive.
 */
export interface QueryResult {
  /**
   * Text actually run, with the `LIMIT` added if any.
   */
  sql: string
  /**
   * Handling of the row limit.
   */
  limit: LimitReport
  /**
   * Maximum number of rows, announced by the server (its limit, or the one
   * written by the user); `null` if unknown.
   */
  expectedRows: number | null
  /**
   * Execution time measured by the server, in microseconds.
   */
  executionUs: number
  /**
   * Names of the columns.
   */
  columns: string[]
  /**
   * For each column, are its values right-aligned (numbers, dates and
   * times)?
   */
  rightAligned: boolean[]
  /**
   * Type of each column, announced by the server; empty for a result saved
   * before columns were typed.
   */
  columnTypes: ColumnTypeInfo[]
  /**
   * Rows received so far. In the interface, this array is not reactive
   * (adding thousands of rows one by one would be costly): `rowCount` signals
   * the arrival of new rows.
   */
  rows: Cell[][]
  /**
   * Number of rows received: the reactive value to watch for redrawing.
   */
  rowCount: number
  /**
   * Were rows beyond the limit discarded?
   */
  truncated: boolean
  /**
   * Number of rows changed, for a write.
   */
  affectedRows: number
  /**
   * Execution duration, in milliseconds.
   */
  elapsedMs: number
}

/**
 * Query of a batch, shown in its own tab.
 */
export interface QueryTab {
  /**
   * Id of the tab, unique in the session (two batches each have their query
   * number 0).
   */
  id: number
  /**
   * Number of the query's batch: only the tabs of the current batch receive
   * the server's messages.
   */
  batch: number
  /**
   * Number of the query in its batch.
   */
  query: number
  /**
   * Text of the query, as split in the editor.
   */
  sql: string
  /**
   * Result, `null` as long as the columns are not received.
   */
  result: QueryResult | null
  /**
   * Error of the query.
   */
  error: string | null
  /**
   * Has the query ended (successfully or in error)?
   */
  done: boolean
  /**
   * Instant the query started (`performance.now()`), `null` as long as it waits for its turn.
   */
  startedAt: number | null
  /**
   * Instant the columns were received (`performance.now()`): the server has
   * finished running the query and starts sending the rows. `null` before,
   * or for a query without a result.
   */
  columnsAt: number | null
  /**
   * Instant the query ended, `null` as long as it has not ended.
   */
  endedAt: number | null
  /**
   * Volume received for the query, in bytes: size of the messages of its
   * result (columns, rows, end) as they go over the WebSocket.
   */
  bytes: number
  /**
   * Pinned tab: it stays when a new batch is started.
   */
  pinned: boolean
  /**
   * Name given by the user, `null` for the default label.
   */
  name: string | null
}

/**
 * Whole state of a session.
 */
export interface SessionState {
  /**
   * Stage of the session.
   */
  status: SessionStatus
  /**
   * Identity of the target server, known once the session is ready.
   */
  server: ServerInfo | null
  /**
   * Current database on the target server (it changes with `USE`), `null` if none.
   */
  database: string | null
  /**
   * Tabs: the pinned ones of the previous batches, then the queries of the current batch.
   */
  tabs: QueryTab[]
  /**
   * Number of the current batch (0 before the first one).
   */
  batch: number
  /**
   * Id of the next tab created.
   */
  nextTabId: number
  /**
   * Last error of the session itself (connection, invalid request).
   */
  error: string | null
  /**
   * Are the editor's saved tabs being read? The interface then shows a
   * loading indicator in their place.
   */
  loading: boolean
}

/**
 * Creates the state of a session that is opening.
 */
export function initialState(): SessionState {
  return {
    status: 'connecting',
    server: null,
    database: null,
    tabs: [],
    batch: 0,
    nextTabId: 1,
    error: null,
    loading: false,
  }
}

/**
 * Updates the state with a message of the server.
 *
 * Changes `state` in place: the rows are added to the existing array, without
 * copying a result that can count thousands of rows.
 *
 * @param state State of the session.
 * @param message Message received from the server.
 * @param now Instant of reception, to measure the duration of the queries.
 * @param bytes
 *     Size of the message in bytes, counted in the volume received by its
 *     query (columns, rows and end of a result).
 */
export function applyMessage(
  state: SessionState,
  message: ServerMessage,
  now: number = performance.now(),
  bytes: number = 0,
): void {
  if (message.type === 'columns' || message.type === 'rows' || message.type === 'done') {
    const tab = tabOf(state, message.query)
    if (tab) {
      tab.bytes += bytes
    }
  }
  switch (message.type) {
    case 'ready':
      state.status = 'ready'
      state.server = message.server
      state.database = message.database
      break
    case 'batch':
      state.tabs = [
        ...state.tabs,
        ...message.queries.map((query) => ({
          id: query.tab > 0 ? query.tab : state.nextTabId++,
          batch: state.batch,
          query: query.query,
          sql: query.sql,
          result: null,
          error: null,
          done: false,
          startedAt: null,
          columnsAt: null,
          endedAt: null,
          bytes: 0,
          pinned: false,
          name: null,
        })),
      ]
      break
    case 'started': {
      const tab = tabOf(state, message.query)
      if (tab) {
        tab.startedAt = now
      }
      break
    }
    case 'columns': {
      const tab = tabOf(state, message.query)
      if (tab) {
        tab.columnsAt = now
        tab.result = {
          sql: message.sql,
          limit: message.limit,
          expectedRows: message.expected_rows,
          executionUs: message.execution_us,
          columns: message.columns,
          rightAligned: message.right_aligned,
          columnTypes: message.types,
          rows: [],
          rowCount: 0,
          truncated: false,
          affectedRows: 0,
          elapsedMs: 0,
        }
      }
      break
    }
    case 'rows': {
      const result = tabOf(state, message.query)?.result
      if (result) {
        result.rows.push(...message.rows)
        result.rowCount = result.rows.length
      }
      break
    }
    case 'done': {
      const tab = tabOf(state, message.query)
      if (tab) {
        tab.done = true
        tab.endedAt = now
        if (tab.result) {
          tab.result.truncated = message.truncated
          tab.result.affectedRows = message.affected_rows
          tab.result.elapsedMs = message.elapsed_ms
        }
      }
      state.database = message.database
      break
    }
    case 'error': {
      if (message.query !== null) {
        const tab = tabOf(state, message.query)
        if (tab) {
          tab.error = message.message
          tab.done = true
          tab.endedAt = now
        }
        break
      }
      state.error = message.message
      // Before `ready`, the error is that of the connection to the target
      // server: the server then closes the session.
      state.status = state.status === 'connecting' ? 'closed' : 'ready'
      break
    }
    case 'batch_done':
      state.status = 'ready'
      break
    case 'database':
      state.database = message.database
      break
    case 'resume':
      resumeBatch(state, message.queries, message.running, now)
      break
  }
}

/**
 * Takes back the last batch of the session, received when reattaching to it:
 * it becomes the current batch, and each query finds its tab again (same id:
 * name and pinning are kept), or gets a new one.
 *
 * @param state State of the session.
 * @param queries Queries of the batch, where they stand.
 * @param running Is the batch still running?
 * @param now Instant of reception, to place the start of the queries.
 */
function resumeBatch(
  state: SessionState,
  queries: ResumedQuery[],
  running: boolean,
  now: number,
): void {
  state.batch++
  if (running) {
    state.status = 'running'
  }
  for (const query of queries) {
    const index = state.tabs.findIndex((tab) => tab.id === query.tab)
    const tab = resumedTab(query, state.batch, now, index < 0 ? undefined : state.tabs[index])
    if (index < 0) {
      state.tabs.push(tab)
    } else {
      state.tabs[index] = tab
    }
  }
}

/**
 * Tab of the query number `query` of the current batch.
 *
 * @param state State of the session.
 * @param query Number of the query.
 */
function tabOf(state: SessionState, query: number): QueryTab | undefined {
  return state.tabs.find((tab) => tab.batch === state.batch && tab.query === query)
}

/**
 * Progress of a query, shown by the dot of its tab.
 *
 * - `waiting`: waiting for its turn (limited parallel run);
 * - `executing`: started, no result received;
 * - `loading`: columns received, rows being received;
 * - `done`: complete result;
 * - `error`: failure.
 */
export type TabProgress = 'waiting' | 'executing' | 'loading' | 'done' | 'error'

/**
 * Progress of the query of a tab.
 *
 * @param tab Tab of the query.
 */
export function tabProgress(tab: QueryTab): TabProgress {
  if (tab.error !== null) {
    return 'error'
  }
  if (tab.done) {
    return 'done'
  }
  if (tab.startedAt === null) {
    return 'waiting'
  }
  return tab.result === null ? 'executing' : 'loading'
}

/**
 * Execution time of a query, in milliseconds: the one measured by the server,
 * to the microsecond, once the columns are received; otherwise, since its
 * start (until its end for a query without a result, until `now` while it
 * runs); `null` as long as it waits for its turn.
 *
 * @param tab Tab of the query.
 * @param now Present instant (`performance.now()`).
 */
export function executionDuration(tab: QueryTab, now: number): number | null {
  if (tab.result) {
    return tab.result.executionUs / 1000
  }
  if (tab.startedAt === null) {
    return null
  }
  return (tab.endedAt ?? now) - tab.startedAt
}

/**
 * Transfer and display time of the rows, in milliseconds: from the reception
 * of the columns to the end of the query. It runs until `now` during the
 * reception; `null` without a result.
 *
 * @param tab Tab of the query.
 * @param now Present instant (`performance.now()`).
 */
export function transferDuration(tab: QueryTab, now: number): number | null {
  return tab.columnsAt === null ? null : (tab.endedAt ?? now) - tab.columnsAt
}

/**
 * Completed share of a query, from 0 to 1, for the progress bar of its tab;
 * `null` when it is unknown (indeterminate bar).
 *
 * - waiting: 0;
 * - executing: unknown;
 * - receiving: rows received out of the maximum number of rows announced by
 *   the server (Prisme's limit or the user's); unknown without this number;
 * - ended, successfully or in error: 1.
 *
 * @param tab Tab of the query.
 */
export function tabFraction(tab: QueryTab): number | null {
  switch (tabProgress(tab)) {
    case 'waiting':
      return 0
    case 'done':
    case 'error':
      return 1
    case 'loading': {
      const expected = tab.result?.expectedRows ?? 0
      return tab.result && expected > 0 ? Math.min(1, tab.result.rowCount / expected) : null
    }
    default:
      return null
  }
}

/**
 * Parses a message received from the server.
 *
 * A message that is not JSON returns `null` instead of throwing: thrown in the
 * WebSocket listener, the error would leave the session silent.
 *
 * @param data Text of the message.
 */
export function parseMessage(data: string): ServerMessage | null {
  try {
    return JSON.parse(data) as ServerMessage
  } catch {
    return null
  }
}

/**
 * Size in bytes of a text encoded in UTF-8, as it goes over the WebSocket,
 * without copying it (a message can hold thousands of rows).
 *
 * @param text Text of the message.
 */
export function utf8Length(text: string): number {
  let bytes = 0
  for (let index = 0; index < text.length; index++) {
    const code = text.charCodeAt(index)
    if (code < 0x80) {
      bytes += 1
    } else if (code < 0x800) {
      bytes += 2
    } else if (code >= 0xd800 && code < 0xdc00) {
      // Surrogate pair: a character outside the basic plane, 4 bytes.
      bytes += 4
      index++
    } else {
      bytes += 3
    }
  }
  return bytes
}

/**
 * Formats a data volume in the unit that suits it: bytes, then kB, MB, GB and
 * TB (by 1,000), with one decimal beyond the byte. Units are displayed in
 * French (o, ko, Mo, Go, To), like the rest of the interface.
 *
 * @param bytes Volume in bytes.
 */
export function formatBytes(bytes: number): string {
  const units = ['ko', 'Mo', 'Go', 'To']
  if (bytes < 1000) {
    return `${Math.round(Math.max(0, bytes))} o`
  }
  let value = bytes / 1000
  let unit = 0
  while (value >= 1000 && unit < units.length - 1) {
    value /= 1000
    unit++
  }
  return `${value.toLocaleString('fr-FR', { minimumFractionDigits: 1, maximumFractionDigits: 1 })} ${units[unit]}`
}

/**
 * Formats a duration in the unit that suits it: microseconds under the
 * millisecond, milliseconds under the second, seconds (one decimal) under the
 * minute, minutes and seconds under the hour, then hours and minutes.
 *
 * @param milliseconds Duration.
 */
export function formatDuration(milliseconds: number): string {
  const duration = Math.max(0, milliseconds)
  if (duration < 1) {
    return `${Math.round(duration * 1000)} µs`
  }
  if (duration < 1000) {
    return `${Math.round(duration)} ms`
  }
  if (duration < 60_000) {
    const seconds = Math.floor(duration / 100) / 10
    return `${seconds.toLocaleString('fr-FR', { minimumFractionDigits: 1 })} s`
  }
  const pad = (value: number) => String(value).padStart(2, '0')
  const totalSeconds = Math.floor(duration / 1000)
  if (totalSeconds < 3600) {
    return `${Math.floor(totalSeconds / 60)} min ${pad(totalSeconds % 60)} s`
  }
  return `${Math.floor(totalSeconds / 3600)} h ${pad(Math.floor(totalSeconds / 60) % 60)} min`
}

/**
 * Records the start of a batch in the state: the tabs of the previous batch
 * disappear, except the pinned ones.
 *
 * @param state State of the session.
 */
export function startBatch(state: SessionState): void {
  state.status = 'running'
  state.batch++
  state.tabs = state.tabs.filter((tab) => tab.pinned)
  state.error = null
}

/**
 * Removes a tab from the state.
 *
 * @param state State of the session.
 * @param id Id of the tab.
 * @returns The removed tab, absent if it did not exist.
 */
export function removeTab(state: SessionState, id: number): QueryTab | undefined {
  const tab = state.tabs.find((item) => item.id === id)
  state.tabs = state.tabs.filter((item) => item.id !== id)
  return tab
}

/**
 * Interface tab built from a result tab saved by the server. It belongs to no
 * batch of the session (batch 0): the server's messages no longer touch it.
 * Its durations are those measured by the server; the rows stay outside Vue's
 * reactivity.
 *
 * @param saved Saved tab.
 */
export function savedTab(saved: ResultTab): QueryTab {
  const { result } = saved
  const columns = result.columns
  return {
    id: saved.id,
    batch: 0,
    query: saved.query,
    sql: saved.sql,
    result:
      columns === null
        ? null
        : {
            sql: columns.sql,
            limit: columns.limit,
            expectedRows: columns.expected_rows,
            executionUs: columns.execution_us,
            columns: columns.columns,
            rightAligned: columns.right_aligned,
            columnTypes: columns.types,
            rows: markRaw(saved.rows),
            rowCount: result.row_count,
            truncated: result.truncated,
            affectedRows: result.affected_rows,
            elapsedMs: 0,
          },
    error: result.error,
    done: result.done,
    startedAt: result.started ? 0 : null,
    columnsAt: columns === null ? null : 0,
    endedAt: result.done ? result.transfer_us / 1000 : null,
    bytes: result.bytes,
    pinned: saved.pinned,
    name: saved.name === '' ? null : saved.name,
  }
}

/**
 * Interface tab of a query of the batch taken back when reattaching to the
 * session. It belongs to the current batch: the next messages complete it.
 * For an unfinished query, its start and the reception of its columns are
 * placed from the elapsed time measured by the server.
 *
 * @param query Resumed query.
 * @param batch Number of the current batch.
 * @param now Instant of reception (`performance.now()`).
 * @param existing Tab already shown for this query, whose name and pinning it keeps.
 */
export function resumedTab(
  query: ResumedQuery,
  batch: number,
  now: number,
  existing?: QueryTab,
): QueryTab {
  const tab = savedTab({
    id: query.tab,
    query: query.query,
    sql: query.sql,
    name: existing?.name ?? '',
    pinned: existing?.pinned ?? false,
    result: query.result,
    rows: query.rows,
  })
  tab.batch = batch
  if (!query.result.done) {
    const since = (microseconds: number | null) =>
      microseconds === null ? null : now - microseconds / 1000
    tab.startedAt = since(query.started_us_ago)
    tab.columnsAt = since(query.columns_us_ago)
  }
  return tab
}

/**
 * Builds the WebSocket address of an editor's session, on the page's host.
 *
 * @param editorId Id of the editor.
 * @param location Address of the page.
 */
export function sessionUrl(editorId: number, location: Location): string {
  const protocol = location.protocol === 'https:' ? 'wss:' : 'ws:'
  return `${protocol}//${location.host}/api/editors/${editorId}/session`
}

/**
 * Opens the query session of an editor, on its connection: the saved tabs
 * first, then the WebSocket, which reattaches to the server's session if it
 * is still open.
 *
 * The WebSocket closes when the component that opened it disappears; the
 * server's session stays open for the delay set on the connection.
 *
 * @param editorId Id of the editor.
 * @param loadSaved Reading of the editor's saved result tabs.
 * @returns The reactive state of the session, and the functions to run queries and close the session.
 */
export function useQuerySession(editorId: number, loadSaved: () => Promise<ResultTab[]>) {
  const state = reactive(initialState())

  /**
   * WebSocket of the session, opened once the saved tabs are read.
   */
  let socket: WebSocket | null = null

  /**
   * Has the session been closed (component gone)?
   */
  let closed = false

  /**
   * Messages received, not applied yet, with their instant of reception and
   * their size in bytes.
   */
  let pending: [ServerMessage, number, number][] = []

  /**
   * Deferred application of the pending messages, absent if nothing waits.
   */
  let flushTimer: ReturnType<typeof setTimeout> | undefined

  /**
   * Applies all the pending messages at once: the interface is redrawn only
   * once for all the row batches received in the meantime.
   */
  function flush(): void {
    flushTimer = undefined
    const messages = pending
    pending = []
    for (const [message, receivedAt, bytes] of messages) {
      applyMessage(state, message, receivedAt, bytes)
      if (message.type === 'columns') {
        const result = tabOf(state, message.query)?.result
        if (result) {
          // Rows outside Vue's reactivity: only `rowCount` signals their arrival.
          result.rows = markRaw([])
        }
      }
    }
  }

  /**
   * Opens the WebSocket of the session.
   */
  function connect(): void {
    if (closed) {
      return
    }
    socket = new WebSocket(sessionUrl(editorId, window.location))
    socket.addEventListener('message', (event: MessageEvent<string>) => {
      const message = parseMessage(event.data)
      if (message === null) {
        state.error = 'Message illisible reçu du serveur'
        return
      }
      pending.push([message, performance.now(), utf8Length(event.data)])
      flushTimer ??= setTimeout(flush, FLUSH_DELAY)
    })
    socket.addEventListener('close', () => {
      clearTimeout(flushTimer)
      flush()
      state.status = 'closed'
    })
  }

  // The saved tabs first: the batch resumed by the WebSocket finds its tabs among them.
  state.loading = true
  loadSaved()
    .then((saved) => {
      state.tabs = saved.map(savedTab)
    })
    .catch((failure: unknown) => {
      state.error = failure instanceof Error ? failure.message : String(failure)
    })
    .finally(() => {
      state.loading = false
      connect()
    })

  /**
   * Sends the editor's text, if the session is ready.
   *
   * @param sql Whole text of the editor.
   * @param cursor
   *     Position of the cursor (UTF-16 units): only the query found there
   *     runs. When absent: all of them, in parallel.
   * @param rowLimit Row limit; when absent, the one of the settings applies.
   */
  function run(sql: string, cursor?: number, rowLimit?: number): void {
    if (state.status !== 'ready') {
      return
    }
    startBatch(state)
    send({ type: 'run', sql, cursor, row_limit: rowLimit })
  }

  /**
   * Changes the session's current database, like `USE`, without a result
   * tab. During a batch, the change waits for its end. A failure arrives as a
   * session error.
   *
   * @param database Name of the database.
   */
  function changeDatabase(database: string): void {
    if (state.status === 'ready' || state.status === 'running') {
      send({ type: 'use', database })
    }
  }

  /**
   * Stops all the running queries: they end with a "query interrupted"
   * error, and those waiting for their turn do not start.
   */
  function cancel(): void {
    if (state.status === 'running') {
      send({ type: 'cancel' })
    }
  }

  /**
   * Closes a tab. If its query is still running (current batch), it is
   * stopped; its last messages no longer find a tab and are ignored.
   *
   * @param id Id of the tab.
   */
  function closeTab(id: number): void {
    const tab = removeTab(state, id)
    if (tab && !tab.done && tab.batch === state.batch && state.status === 'running') {
      send({ type: 'cancel_query', query: tab.query })
    }
  }

  /**
   * Sends a message to the server.
   *
   * @param message Message of the client.
   */
  function send(message: ClientMessage): void {
    socket?.send(JSON.stringify(message))
  }

  /**
   * Closes the WebSocket of the session.
   */
  function close(): void {
    closed = true
    socket?.close()
  }

  onScopeDispose(close)

  return { state, run, changeDatabase, cancel, closeTab, close }
}
