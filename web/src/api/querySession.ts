/**
 * Query session on a saved connection, over a WebSocket.
 *
 * The client sends (`run`) the editor's whole text, with the cursor position
 * to run only the query found there (Ctrl+Enter), or without a cursor to run
 * in parallel the queries touched by the selected passages, or all of them
 * (Ctrl+Shift+Enter), or only the selected text (`exact`, "Exécuter
 * précisément"). The server announces the queries
 * of the batch (`batch`), then streams their results, each message carrying
 * the number of its query: `started` when it starts (in parallel, it may wait
 * for its turn), `columns`, batches of `rows`, then `done` (or `error`).
 * `batch_done` closes the batch. `cancel` stops all the running queries,
 * `cancel_query` a single one. The protocol is described in
 * `crates/server/src/query_session/protocol.rs`.
 *
 * Each query has its tab. A new batch replaces the tabs of the previous one,
 * except those the user pinned; a tab can also be renamed or closed (its
 * query is then stopped if it is running). A tab also carries the user filter
 * of its grid (`filter`), which a refresh and a resumed session keep.
 *
 * A tab can be refreshed: its query runs again alone, as a batch of its own
 * (`run` with the tab's id), and the other tabs stay. While a batch runs, the
 * refresh waits for its end (`refreshQueue`). The previous result stays
 * displayed until the new columns arrive; if the refresh is stopped, the tab
 * gets its previous result back as it was, and if it fails, with the error
 * above it (`previous`).
 *
 * The session is that of an editor: the server saves its results as they
 * run, and gives each tab the id of its record. On opening, the session
 * reads the list of the saved tabs and shows them at once, with their
 * columns (`awaitingTab`), then opens the WebSocket without waiting for their
 * rows. These are read in the background, chunk by chunk, the tab shown
 * first (`nextReading`), and received as those of a query: through the same
 * grouping of the messages (`saved_rows`), the tab counting as running until
 * its last row (`awaitedRows`), so that the grid and the Pivoteur follow
 * them, and the editor stays usable. On the server side, the session outlives
 * the page for the delay set on its connection: the WebSocket reattaches to
 * it, and then receives its last batch where it stands (`resume`,
 * `resumedTab`), the rows received so far (`resumed_rows`), then the next
 * messages, which continue it. A tab taken back this way is no longer read
 * from its chunks.
 *
 * `applyMessage` updates the state message by message; it depends neither on
 * Vue nor on the WebSocket, which makes it simple to test. `useQuerySession`
 * uses it to expose a reactive state to the components.
 */

import { errorMessage } from './errorMessage'
import { describe } from './messages'
import { markRaw, onScopeDispose, reactive } from 'vue'

import type { ColumnTypeInfo } from './generated/ColumnTypeInfo'
import type { LimitReport } from './generated/LimitReport'
import type { ClientMessage } from './generated/ClientMessage'
import type { ResultTab } from './generated/ResultTab'
import type { ResumedQuery } from './generated/ResumedQuery'
import type { SelectionRange } from './generated/SelectionRange'
import type { ServerInfo } from './generated/ServerInfo'
import type { ServerMessage } from './generated/ServerMessage'
import type { NoticeLevel } from './generated/NoticeLevel'
import type { OutcomeCounts } from './generated/OutcomeCounts'
import type { StatementOutcome } from './generated/StatementOutcome'
import type { StatementVerb } from './generated/StatementVerb'
import type { TabPivot } from './generated/TabPivot'

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
 * Queries of the text to run: the one under the cursor; without a cursor,
 * those touched by the selected passages, or only the selected text; without
 * either, all of them.
 */
export interface RunTarget {
  /**
   * Position of the cursor, in UTF-16 units: only the query found there runs.
   */
  cursor?: number
  /**
   * Passages selected in the text: the queries they touch run, in parallel.
   */
  selection?: SelectionRange[]
  /**
   * With `selection`, runs only the selected text, cut out of the queries it
   * touches; without a selection, nothing runs.
   */
  exact?: boolean
}

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
   * End of the execution (columns received), in milliseconds since the Unix
   * epoch, by the server's clock: the instant the result reflects. 0 if
   * unknown (result saved before it was recorded).
   */
  executedAt: number
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
   * What the statement does to the data, from its AST.
   */
  verb: StatementVerb
  /**
   * What the statement did, as its engine reports it; its fields absent
   * until the end of the query, and when the engine does not report them.
   */
  outcome: StatementOutcome
  /**
   * Execution duration, in milliseconds.
   */
  elapsedMs: number
}

/**
 * User filter of a result tab's grid. Two forms: the global filter searches
 * the text in all the columns (`column` is `null`); the targeted filter
 * searches it in the column at the index `column` of the `SELECT`.
 */
export interface UserFilter {
  /**
   * Searched text, case-insensitive. Empty: no text to search.
   */
  text: string
  /**
   * Index of the targeted column, or `null` for the global filter.
   */
  column: number | null
}

/**
 * User filter that keeps every row.
 */
export const NO_FILTER: Readonly<UserFilter> = Object.freeze({ text: '', column: null })

/**
 * Outcome of a statement whose engine has reported nothing yet.
 */
export const NO_OUTCOME: Readonly<StatementOutcome> = Object.freeze({
  affected: null,
  counts: Object.freeze({
    matched: null,
    changed: null,
    unchanged: null,
    inserted: null,
    replaced: null,
    deleted: null,
    ignored: null,
  }),
  inserted_id: null,
  warnings: null,
  notices: Object.freeze([]) as unknown as StatementOutcome['notices'],
})

/**
 * Configuration of the Pivoteur without any element.
 */
export const NO_PIVOT: Readonly<TabPivot> = Object.freeze({
  fields: [],
  config: { rows: [], columns: [], measures: [], filters: [] },
})

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
   * Number of the query in the batch that runs it, repeated by the server's
   * messages (0 for a refresh).
   */
  query: number
  /**
   * Number of the query in the batch that created the tab, shown in its
   * label: a refresh keeps it.
   */
  number: number
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
   * Did the user stop the query (error `query_interrupted`), or did the
   * reading of its saved rows fail (`saved_rows`)? The rows
   * received before the stop stay a result, cut short: no error banner, a
   * "Résultat tronqué" badge instead, and the Pivoteur and the exports take
   * them.
   */
  interrupted: boolean
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
  /**
   * State of the tab before its refresh, while the refresh runs: given back
   * if it fails or is stopped. `null` outside a refresh.
   */
  previous: PreviousRun | null
  /**
   * User filter of the tab's grid, replaced as a whole on each change.
   */
  filter: UserFilter
  /**
   * Is the form of the user filter expanded? Collapsed, the filter stays
   * applied.
   */
  filterOpen: boolean
  /**
   * Configuration of the Pivoteur of the tab, saved with it and replaced as a
   * whole on each change (`PivotPanel.vue`).
   */
  pivot: TabPivot
  /**
   * Rows of an ended query still to receive: after `resume`
   * (`resumed_rows`), or to read from the chunks of a saved tab
   * (`saved_rows`); `null` otherwise. Until they are all there, the query
   * counts as running: the grid and the Pivoteur follow them, the exports
   * wait for them.
   */
  awaitedRows: number | null
}

/**
 * State of a tab's run: what a refresh replaces.
 */
export type TabRun = Pick<
  QueryTab,
  'result' | 'error' | 'done' | 'interrupted' | 'startedAt' | 'columnsAt' | 'endedAt' | 'bytes'
>

/**
 * State of a tab before its refresh.
 */
export interface PreviousRun extends TabRun {
  /**
   * Has the user stopped the refresh? Its failure then leaves the tab
   * exactly as it was, without an error.
   */
  stopped: boolean
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
   * Is a transaction open on the session's connection? Told by the server
   * at the end of each batch; false once the WebSocket closes, until a
   * `ready` tells it again.
   */
  transaction: boolean
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
   * Is the list of the editor's saved tabs being read? The interface then
   * shows a loading indicator in their place; their rows are read after,
   * in the background.
   */
  loading: boolean
  /**
   * Ids of the tabs whose refresh waits for the end of the running batch, in
   * the order they were asked for.
   */
  refreshQueue: number[]
}

/**
 * Rows of a saved tab, read from one of its chunks: received as those of a
 * query, through the same grouping as the server's messages.
 */
export interface SavedRows {
  type: 'saved_rows'
  /**
   * Tab read. A tab taken back by `resume` meanwhile is another object, with
   * the same id: these rows no longer find it.
   */
  tab: QueryTab
  /**
   * Rows of the chunk, in their order.
   */
  rows: Cell[][]
  /**
   * Is it the last chunk of the tab? Its rows are then all there.
   */
  last: boolean
  /**
   * Did the reading fail? The rows read so far stay, as a result cut short
   * (`interrupted`).
   */
  cut: boolean
}

/**
 * Message the state of a session receives: from the server, or rows of a
 * saved tab.
 */
export type SessionMessage = ServerMessage | SavedRows

/**
 * Creates the state of a session that is opening.
 */
export function initialState(): SessionState {
  return {
    status: 'connecting',
    server: null,
    database: null,
    transaction: false,
    tabs: [],
    batch: 0,
    nextTabId: 1,
    error: null,
    loading: false,
    refreshQueue: [],
  }
}

/**
 * Updates the state with a message of the server, or with rows of a saved
 * tab.
 *
 * Changes `state` in place: the rows are added to the existing array, without
 * copying a result that can count thousands of rows.
 *
 * @param state State of the session.
 * @param message Message received from the server, or rows read from a chunk.
 * @param now Instant of reception, to measure the duration of the queries.
 * @param bytes
 *     Size of the message in bytes, counted in the volume received by its
 *     query (columns, rows and end of a result).
 */
export function applyMessage(
  state: SessionState,
  message: SessionMessage,
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
      state.transaction = message.transaction
      break
    case 'batch': {
      // A refreshed tab is already in the batch.
      const fresh = message.queries.filter((query) => !tabOf(state, query.query))
      if (fresh.length > 0) {
        state.tabs = [
          ...state.tabs,
          ...fresh.map((query) => ({
            id: query.tab > 0 ? query.tab : state.nextTabId++,
            batch: state.batch,
            query: query.query,
            number: query.query,
            sql: query.sql,
            result: null,
            error: null,
            done: false,
            interrupted: false,
            startedAt: null,
            columnsAt: null,
            endedAt: null,
            bytes: 0,
            pinned: false,
            name: null,
            previous: null,
            filter: NO_FILTER,
            filterOpen: false,
            pivot: query.pivot ?? NO_PIVOT,
            awaitedRows: null,
          })),
        ]
      }
      break
    }
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
          executedAt: message.executed_at,
          columns: message.columns,
          rightAligned: message.right_aligned,
          columnTypes: message.types,
          rows: [],
          rowCount: 0,
          truncated: false,
          verb: 'other',
          outcome: NO_OUTCOME,
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
        tab.previous = null
        if (tab.result) {
          tab.result.truncated = message.truncated
          tab.result.verb = message.verb
          tab.result.outcome = message.outcome
          tab.result.elapsedMs = message.elapsed_ms
        }
      }
      state.database = message.database
      break
    }
    case 'error': {
      const text = describe(message.detail)
      if (message.query !== null) {
        const tab = tabOf(state, message.query)
        if (tab?.previous) {
          endRefresh(tab, text)
        } else if (tab) {
          tab.error = text
          tab.done = true
          tab.interrupted = message.detail.code === 'query_interrupted'
          tab.endedAt = now
        }
        break
      }
      state.error = text
      // The session refused the request: a refresh in it did not run.
      for (const tab of state.tabs) {
        if (tab.batch === state.batch) {
          endRefresh(tab, null)
        }
      }
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
    case 'transaction':
      state.transaction = message.open
      break
    case 'resume':
      resumeBatch(state, message.queries, message.running, now)
      break
    case 'resumed_rows': {
      const tab = tabOf(state, message.query)
      if (tab) {
        receiveRows(tab, message.rows, false)
      }
      break
    }
    case 'saved_rows':
      if (state.tabs.includes(message.tab)) {
        // Cut short, the rows read stay a result, as those of a query stopped.
        message.tab.interrupted ||= message.cut
        receiveRows(message.tab, message.rows, message.last)
      }
      break
  }
}

/**
 * Adds rows to the result of an ended query whose rows are still arriving
 * (`awaitedRows`): taken back by `resume`, or read from its chunks. Once they
 * are all there, the query counts as ended.
 *
 * @param tab Tab of the query.
 * @param rows Rows received, in their order.
 * @param last Are they the last ones, whatever their number?
 */
function receiveRows(tab: QueryTab, rows: Cell[][], last: boolean): void {
  const result = tab.result
  if (!result) {
    return
  }
  result.rows.push(...rows)
  result.rowCount = result.rows.length
  if (tab.awaitedRows !== null && (last || result.rowCount >= tab.awaitedRows)) {
    tab.awaitedRows = null
    tab.done = true
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
 * Ends a failed refresh: the tab gets back its state from before the refresh,
 * with the error above its result, unless the user stopped it. Without effect
 * outside a refresh.
 *
 * @param tab Refreshed tab.
 * @param error Failure message, `null` for none.
 */
function endRefresh(tab: QueryTab, error: string | null): void {
  if (tab.previous === null) {
    return
  }
  const { stopped, ...run } = tab.previous
  Object.assign(tab, run)
  if (!stopped && error !== null) {
    tab.error = error
  }
  tab.previous = null
}

/**
 * State of a tab's run, to keep before its refresh.
 *
 * @param tab Tab of the query.
 */
function runOf(tab: QueryTab): TabRun {
  const { result, error, done, interrupted, startedAt, columnsAt, endedAt, bytes } = tab
  return { result, error, done, interrupted, startedAt, columnsAt, endedAt, bytes }
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
 * - `stopped`: stopped by the user, the rows received kept;
 * - `error`: failure.
 */
export type TabProgress = 'waiting' | 'executing' | 'loading' | 'done' | 'stopped' | 'error'

/**
 * Progress of the query of a tab.
 *
 * @param tab Tab of the query.
 */
export function tabProgress(tab: QueryTab): TabProgress {
  if (tab.interrupted) {
    return 'stopped'
  }
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
 *   the server (Prisme's limit or the user's); unknown without this number.
 *   After `resume`, the rows of an ended query out of those it received;
 * - ended, successfully, stopped or in error: 1.
 *
 * @param tab Tab of the query.
 */
export function tabFraction(tab: QueryTab): number | null {
  switch (tabProgress(tab)) {
    case 'waiting':
      return 0
    case 'done':
    case 'stopped':
    case 'error':
      return 1
    case 'loading': {
      const expected = tab.awaitedRows ?? tab.result?.expectedRows ?? 0
      return tab.result && expected > 0 ? Math.min(1, tab.result.rowCount / expected) : null
    }
    default:
      return null
  }
}

/**
 * Has the query of a tab failed? A query stopped by the user has not: the rows
 * it received stay a result, which the grid, the Pivoteur and the exports
 * take.
 *
 * @param tab Tab of the query.
 */
export function failed(tab: QueryTab): boolean {
  return tab.error !== null && !tab.interrupted
}

/**
 * Can the query of a tab be stopped? It belongs to the batch running, and has
 * not ended: running, or waiting for its turn. A query taken back ended, whose
 * rows are still arriving (`awaitedRows`), has nothing left to stop.
 *
 * @param state State of the session.
 * @param tab Tab of the query.
 */
export function stoppable(state: SessionState, tab: QueryTab): boolean {
  return (
    !tab.done && tab.awaitedRows === null && tab.batch === state.batch && state.status === 'running'
  )
}

/**
 * Progress of a batch of queries, for the tab of its editor.
 */
export interface BatchProgress {
  /**
   * Stage of the batch, as that of a query: running while one of its queries
   * runs, then ended in error if one failed, stopped if the user stopped
   * one, done otherwise.
   */
  progress: TabProgress
  /**
   * Completed share of the batch, from 0 to 1, `null` when unknown.
   */
  fraction: number | null
}

/**
 * Progress of the last batch of a session, from the tabs of its queries:
 * while one runs, the most advanced stage among those still running
 * (receiving, then executing, then waiting), and the mean of their completed
 * shares, an unknown share counting as nothing; unknown when no share is
 * known. Once all have ended: `error` if one failed, otherwise `stopped` if
 * the user stopped one, `done` otherwise. `null`
 * without any query in the batch.
 *
 * @param state Tabs of the session and number of its last batch.
 */
export function batchProgress(state: Pick<SessionState, 'tabs' | 'batch'>): BatchProgress | null {
  const tabs = state.tabs.filter((tab) => tab.batch === state.batch)
  if (tabs.length === 0) {
    return null
  }
  const stages = tabs.map(tabProgress)
  const running = (['loading', 'executing', 'waiting'] as const).find((stage) =>
    stages.includes(stage),
  )
  if (running === undefined) {
    const ended = (['error', 'stopped'] as const).find((stage) => stages.includes(stage))
    return { progress: ended ?? 'done', fraction: 1 }
  }
  const known = tabs.map(tabFraction).filter((fraction) => fraction !== null)
  const fraction =
    known.length === 0 ? null : known.reduce((sum, value) => sum + value, 0) / tabs.length
  return { progress: running, fraction }
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
 * Encoder of `utf8Length`.
 */
const UTF8_ENCODER = new TextEncoder()

/**
 * Buffer that `utf8Length` encodes into, a piece of the text at a time: it
 * keeps its size, whatever the size of a message.
 */
const UTF8_BUFFER = new Uint8Array(64 * 1024)

/**
 * Code units of a piece of text: each one takes at most 3 bytes in UTF-8 (a
 * surrogate pair, 2 units, takes 4), so that a piece always fits in the buffer.
 */
const UTF8_PIECE = Math.floor(UTF8_BUFFER.length / 3)

/**
 * Size in bytes of a text encoded in UTF-8, as it goes over the WebSocket.
 *
 * The browser encodes the text (`TextEncoder.encodeInto`), piece by piece,
 * into a buffer of a fixed size. Over a reception of 1 million rows, a loop
 * over the characters in JavaScript took 1.7 s of the main thread, the
 * encoder 0.7 s, and the scripts of the page run 0.35 to 0.70 s less
 * (`docs/PERFORMANCES.md`, chapter 12).
 *
 * @param text Text of the message.
 */
export function utf8Length(text: string): number {
  let bytes = 0
  for (let start = 0; start < text.length;) {
    let end = Math.min(text.length, start + UTF8_PIECE)
    const last = text.charCodeAt(end - 1)
    // A piece does not end between the two halves of a surrogate pair: apart,
    // each half would be encoded as a replacement character, of 3 bytes.
    if (end < text.length && last >= 0xd800 && last < 0xdc00) {
      end--
    }
    bytes += UTF8_ENCODER.encodeInto(text.substring(start, end), UTF8_BUFFER).written
    start = end
  }
  return bytes
}

/**
 * Formats the number of rows of a result, for its status bar: "5 000
 * lignes", or, when a user filter keeps only some of them, "120 / 5 000
 * lignes".
 *
 * @param loaded Number of rows received.
 * @param kept Number of rows the user filter keeps, `null` without a filter.
 */
export function formatRowCount(loaded: number, kept: number | null): string {
  const total = `${loaded.toLocaleString('fr-FR')} ${loaded > 1 ? 'lignes' : 'ligne'}`
  return kept === null ? total : `${kept.toLocaleString('fr-FR')} / ${total}`
}

/**
 * Participle of each verb whose count names the rows it changed: singular,
 * plural.
 */
const PARTICIPLES: Partial<Record<StatementVerb, [string, string]>> = {
  insert: ['insérée', 'insérées'],
  update: ['modifiée', 'modifiées'],
  delete: ['supprimée', 'supprimées'],
}

/**
 * Formats a number of rows with its participle: "aucune ligne insérée",
 * "1 ligne insérée", "3 lignes insérées".
 *
 * @param count Number of rows.
 * @param forms Participle, singular and plural.
 */
function countedRows(count: number, [one, many]: [string, string]): string {
  if (count === 0) {
    return `aucune ligne ${one}`
  }
  const rows = count > 1 ? 'lignes' : 'ligne'
  return `${count.toLocaleString('fr-FR')} ${rows} ${count > 1 ? many : one}`
}

/**
 * Counts of the rows, in the order the summary gives them, with their
 * participle: singular, plural. `matched` stands apart: it holds the rows
 * changed and unchanged.
 */
const COUNTED: [Exclude<keyof OutcomeCounts, 'matched'>, [string, string]][] = [
  ['inserted', ['insérée', 'insérées']],
  ['replaced', ['remplacée', 'remplacées']],
  ['changed', ['modifiée', 'modifiées']],
  ['unchanged', ['inchangée', 'inchangées']],
  ['deleted', ['supprimée', 'supprimées']],
  ['ignored', ['ignorée', 'ignorées']],
]

/**
 * Summary of the rows counted by what happened to them: "1 ligne insérée,
 * 1 modifiée, 1 inchangée", "2 lignes modifiées sur 3 trouvées"; `null`
 * when the engine counted none. The counts at 0 are left out, unless all
 * are: "aucune ligne modifiée".
 *
 * @param counts Counts the engine reported.
 */
function countsSummary(counts: OutcomeCounts): string | null {
  if (counts.matched !== null && counts.changed !== null) {
    const found = `${counts.matched.toLocaleString('fr-FR')} ${counts.matched > 1 ? 'trouvées' : 'trouvée'}`
    return `${countedRows(counts.changed, ['modifiée', 'modifiées'])} sur ${found}`
  }
  const known = COUNTED.flatMap(([key, forms]) => {
    const count = counts[key]
    return count === null ? [] : [{ count, forms }]
  })
  const [first] = known
  if (first === undefined) {
    return null
  }
  const shown = known.filter(({ count }) => count > 0)
  const [lead, ...rest] = shown.length > 0 ? shown : [first]
  return [
    countedRows(lead!.count, lead!.forms),
    ...rest.map(
      ({ count, forms: [one, many] }) =>
        `${count.toLocaleString('fr-FR')} ${count > 1 ? many : one}`,
    ),
  ].join(', ')
}

/**
 * Summary of what a statement without rows did, from its verb and from the
 * fields its engine reported, without knowing the engine: the counts of the
 * rows when the engine gives them ("1 ligne insérée, 1 ignorée"), else its
 * number of affected rows named by the verb ("3 lignes insérées"). A number
 * whose meaning depends on the engine (a `REPLACE`, an upsert, a change of
 * the schema that copies rows) says "concernées"; without any count,
 * "Requête exécutée".
 *
 * @param verb What the statement does to the data.
 * @param outcome What its engine reported.
 */
export function outcomeSummary(verb: StatementVerb, outcome: StatementOutcome): string {
  const affected = outcome.affected
  if (verb === 'truncate') {
    return 'Table vidée'
  }
  const counted = countsSummary(outcome.counts)
  if (counted !== null) {
    return capitalized(counted)
  }
  if (affected === null) {
    return 'Requête exécutée'
  }
  const participle = PARTICIPLES[verb]
  if (participle !== undefined) {
    return capitalized(countedRows(affected, participle))
  }
  if (affected === 0) {
    return 'Requête exécutée'
  }
  return capitalized(countedRows(affected, ['concernée', 'concernées']))
}

/**
 * Auto-increment id of the row the statement inserted, for the summary: "Id
 * inséré : 42" for one row, "Premier id inséré : 42" for several; `null`
 * without an id.
 *
 * @param outcome What the engine reported.
 */
export function insertedIdLabel(outcome: StatementOutcome): string | null {
  if (outcome.inserted_id === null) {
    return null
  }
  const several = (outcome.counts.inserted ?? 1) > 1
  return `${several ? 'Premier id inséré' : 'Id inséré'} : ${outcome.inserted_id}`
}

/**
 * Name of each level of a notice of the engine, before its code.
 */
export const NOTICE_LABELS: Record<NoticeLevel, string> = {
  note: 'Note',
  warning: 'Avertissement',
  error: 'Erreur',
}

/**
 * Number of warnings the engine counted beyond those it kept and sent: 0
 * when it sent them all, or none.
 *
 * @param outcome What the engine reported.
 */
export function droppedNotices(outcome: StatementOutcome): number {
  const counted = outcome.warnings ?? 0
  return outcome.notices.length === 0 ? 0 : Math.max(counted - outcome.notices.length, 0)
}

/**
 * Formats a number of warnings: "1 avertissement", "3 avertissements".
 *
 * @param count Number of warnings, notes included.
 */
export function formatWarnings(count: number): string {
  return `${count.toLocaleString('fr-FR')} ${count > 1 ? 'avertissements' : 'avertissement'}`
}

/**
 * Text with its first letter in upper case.
 *
 * @param text Text, not empty.
 */
function capitalized(text: string): string {
  return `${text.charAt(0).toUpperCase()}${text.slice(1)}`
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
 * Formats an instant to the second, in local time and in French, like the rest
 * of the interface: "20/10/2026 14:32:05".
 *
 * @param milliseconds Instant, in milliseconds since the Unix epoch.
 */
export function formatInstant(milliseconds: number): string {
  return new Date(milliseconds).toLocaleString('fr-FR')
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
 * Records the start of a tab's refresh in the state: a batch of its own, in
 * which the tab, kept, runs its query again. Its current state is kept until
 * the refresh succeeds.
 *
 * @param state State of the session.
 * @param tab Refreshed tab.
 */
export function startRefresh(state: SessionState, tab: QueryTab): void {
  state.status = 'running'
  state.batch++
  state.error = null
  tab.previous = { ...(tab.previous ?? runOf(tab)), stopped: false }
  Object.assign(tab, {
    batch: state.batch,
    query: 0,
    result: null,
    error: null,
    done: false,
    interrupted: false,
    startedAt: null,
    columnsAt: null,
    endedAt: null,
    bytes: 0,
  })
}

/**
 * Refresh of a tab, for its button:
 *
 * - `running`: the refresh runs, it can be stopped;
 * - `queued`: it waits for the end of the running batch, it can be cancelled;
 * - `idle`: the tab can be refreshed;
 * - `unavailable`: no refresh (query still running, session closed).
 */
export type RefreshState = 'running' | 'queued' | 'idle' | 'unavailable'

/**
 * Refresh state of a tab.
 *
 * @param state State of the session.
 * @param tab Tab of the query.
 */
export function refreshState(state: SessionState, tab: QueryTab): RefreshState {
  if (tab.previous !== null) {
    return 'running'
  }
  if (state.refreshQueue.includes(tab.id)) {
    return 'queued'
  }
  const open = state.status === 'ready' || state.status === 'running'
  return open && tab.done ? 'idle' : 'unavailable'
}

/**
 * Result shown by a tab: that of its query, or, during a refresh, the
 * previous one until the new columns arrive.
 *
 * @param tab Tab of the query.
 */
export function shownResult(tab: QueryTab): QueryResult | null {
  return tab.result ?? tab.previous?.result ?? null
}

/**
 * Does a tab show a grid, that is a result with columns?
 *
 * @param tab Tab of the query.
 */
export function showsGrid(tab: QueryTab): boolean {
  return (shownResult(tab)?.columns.length ?? 0) > 0
}

/**
 * Removes a tab from the state, and from the refresh queue.
 *
 * @param state State of the session.
 * @param id Id of the tab.
 * @returns The removed tab, absent if it did not exist.
 */
export function removeTab(state: SessionState, id: number): QueryTab | undefined {
  const tab = state.tabs.find((item) => item.id === id)
  state.tabs = state.tabs.filter((item) => item.id !== id)
  state.refreshQueue = state.refreshQueue.filter((item) => item !== id)
  return tab
}

/**
 * Interface tab built from a result tab saved by the server. It belongs to no
 * batch of the session (batch 0): the server's messages no longer touch it.
 * Its durations are those measured by the server; the rows stay outside Vue's
 * reactivity.
 *
 * @param saved Saved tab.
 * @param rows Its rows, read from its chunks.
 */
export function savedTab(saved: ResultTab, rows: Cell[][]): QueryTab {
  const { result } = saved
  const columns = result.columns
  return {
    id: saved.id,
    batch: 0,
    query: saved.query,
    number: saved.query,
    sql: saved.sql,
    result:
      columns === null
        ? null
        : {
            sql: columns.sql,
            limit: columns.limit,
            expectedRows: columns.expected_rows,
            executionUs: columns.execution_us,
            executedAt: columns.executed_at,
            columns: columns.columns,
            rightAligned: columns.right_aligned,
            columnTypes: columns.types,
            rows: markRaw(rows),
            rowCount: rows.length,
            truncated: result.truncated,
            verb: result.verb,
            outcome: result.outcome,
            elapsedMs: 0,
          },
    // A result saved before the details existed shows its English text.
    error: result.error_detail === null ? result.error : describe(result.error_detail),
    done: result.done,
    interrupted: result.error_detail?.code === 'query_interrupted',
    startedAt: result.started ? 0 : null,
    columnsAt: columns === null ? null : 0,
    endedAt: result.done ? result.transfer_us / 1000 : null,
    bytes: result.bytes,
    pinned: saved.pinned,
    name: saved.name === '' ? null : saved.name,
    previous: null,
    filter: NO_FILTER,
    filterOpen: false,
    pivot: saved.pivot,
    awaitedRows: null,
  }
}

/**
 * Interface tab of a saved tab whose rows are still to read from its chunks:
 * shown at once, with its columns. An ended query counts as running until
 * its last row (`awaitedRows`), as a query being received.
 *
 * @param saved Saved tab.
 */
export function awaitingTab(saved: ResultTab): QueryTab {
  const tab = savedTab(saved, [])
  if (tab.done && tab.result !== null && saved.chunks > 0) {
    tab.done = false
    tab.awaitedRows = saved.result.row_count
  }
  return tab
}

/**
 * Reading of the chunks of a saved tab.
 */
export interface ChunkReading {
  /**
   * Tab read, as it stands in the state of the session.
   */
  tab: QueryTab
  /**
   * Number of its chunks.
   */
  chunks: number
  /**
   * Rank of the next chunk to read.
   */
  next: number
}

/**
 * Next reading to advance by a chunk: that of the tab shown, if it still
 * has some, otherwise the first in the order of the tabs. A tab closed, or
 * taken back by `resume` (another object), is no longer read. `undefined`
 * once there is nothing left to read.
 *
 * @param tabs Tabs of the session.
 * @param readings Readings of the saved tabs.
 * @param shown Id of the tab shown, if any.
 */
export function nextReading(
  tabs: QueryTab[],
  readings: ChunkReading[],
  shown: number | undefined,
): ChunkReading | undefined {
  const open = readings.filter((reading) => reading.next < reading.chunks)
  const first = tabs.find((tab) => tab.id === shown && open.some((reading) => reading.tab === tab))
  const chosen = first ?? tabs.find((tab) => open.some((reading) => reading.tab === tab))
  return open.find((reading) => reading.tab === chosen)
}

/**
 * Interface tab of a query of the batch taken back when reattaching to the
 * session. It belongs to the current batch: the next messages complete it,
 * starting with the rows received so far (`resumed_rows`); an ended query
 * waits for them before counting as ended. For an unfinished query, its
 * start and the reception of its columns are placed from the elapsed time
 * measured by the server. A refresh keeps the state of the tab already shown,
 * until it succeeds.
 *
 * @param query Resumed query.
 * @param batch Number of the current batch.
 * @param now Instant of reception (`performance.now()`).
 * @param existing
 *     Tab already shown for this query, whose name, pinning, number, user
 *     filter and configuration of the Pivoteur it keeps.
 */
export function resumedTab(
  query: ResumedQuery,
  batch: number,
  now: number,
  existing?: QueryTab,
): QueryTab {
  const tab = savedTab(
    {
      id: query.tab,
      query: query.query,
      sql: query.sql,
      name: existing?.name ?? '',
      pinned: existing?.pinned ?? false,
      pivot: existing?.pivot ?? NO_PIVOT,
      result: query.result,
      chunks: 0,
    },
    [],
  )
  tab.batch = batch
  tab.number = existing?.number ?? query.query
  tab.filter = existing?.filter ?? NO_FILTER
  tab.filterOpen = existing?.filterOpen ?? false
  if (!query.result.done) {
    const since = (microseconds: number | null) =>
      microseconds === null ? null : now - microseconds / 1000
    tab.startedAt = since(query.started_us_ago)
    tab.columnsAt = since(query.columns_us_ago)
  }
  if (query.refresh && existing) {
    tab.previous = existing.previous ?? { ...runOf(existing), stopped: false }
    if (tab.done) {
      if (tab.error === null) {
        tab.previous = null
      } else {
        endRefresh(tab, tab.error)
      }
    }
  }
  if (tab.done && tab.previous === null && tab.result !== null && query.resumed_rows > 0) {
    tab.done = false
    tab.awaitedRows = query.resumed_rows
  }
  return tab
}

/**
 * Builds the WebSocket address of an editor's session, on the page's host.
 *
 * @param editorId Id of the editor.
 * @param location Address of the page.
 * @param database
 *     Database to open a new session on, in place of the connection's;
 *     ignored by a session still open, which the WebSocket reattaches to.
 */
export function sessionUrl(editorId: number, location: Location, database?: string): string {
  const protocol = location.protocol === 'https:' ? 'wss:' : 'ws:'
  const query = database === undefined ? '' : `?${new URLSearchParams({ database })}`
  return `${protocol}//${location.host}/api/editors/${editorId}/session${query}`
}

/**
 * Opens the query session of an editor, on its connection: the list of the
 * saved tabs first, then the WebSocket, which reattaches to the server's
 * session if it is still open, while the rows of the saved tabs are read in
 * the background, chunk by chunk.
 *
 * The WebSocket closes when the component that opened it disappears; the
 * server's session stays open for the delay set on the connection.
 *
 * @param editorId Id of the editor.
 * @param loadSaved Reading of the editor's saved result tabs, without their rows.
 * @param loadChunk Reading of a chunk of the saved rows of a tab, by its id and its rank.
 * @param shownTab Id of the tab shown, whose rows are read first.
 * @param database
 *     Database to open a new session on, in place of the connection's (see
 *     `sessionUrl`).
 * @returns The reactive state of the session, and the functions to run queries and close the session.
 */
export function useQuerySession(
  editorId: number,
  loadSaved: () => Promise<ResultTab[]>,
  loadChunk: (tab: number, position: number) => Promise<Cell[][]>,
  shownTab: () => number | undefined,
  database?: string,
) {
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
  let pending: [SessionMessage, number, number][] = []

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
    startQueuedRefresh()
  }

  /**
   * Keeps a message for the next application of the pending messages.
   *
   * @param message Message received.
   * @param bytes Its size in bytes, counted in the volume of its query.
   */
  function receive(message: SessionMessage, bytes: number): void {
    pending.push([message, performance.now(), bytes])
    flushTimer ??= setTimeout(flush, FLUSH_DELAY)
  }

  /**
   * Opens the WebSocket of the session.
   */
  function connect(): void {
    if (closed) {
      return
    }
    socket = new WebSocket(sessionUrl(editorId, window.location, database))
    socket.addEventListener('message', (event: MessageEvent<string>) => {
      const message = parseMessage(event.data)
      if (message === null) {
        state.error = 'Message illisible reçu du serveur.'
        return
      }
      receive(message, utf8Length(event.data))
    })
    socket.addEventListener('close', () => {
      clearTimeout(flushTimer)
      flush()
      state.status = 'closed'
      state.transaction = false
      // The refreshes will not end: the tabs get their result back.
      state.refreshQueue = []
      for (const tab of state.tabs) {
        endRefresh(tab, null)
      }
    })
  }

  /**
   * Reads the list of the saved tabs, and shows them, without their rows.
   *
   * @returns The readings of their chunks, in the order of the tabs.
   */
  async function readSaved(): Promise<ChunkReading[]> {
    const saved = await loadSaved()
    if (closed) {
      return []
    }
    state.tabs = saved.map(awaitingTab)
    // The tabs as the state holds them: the rows read find them there.
    return saved.flatMap((tab, index) =>
      tab.chunks > 0 ? [{ tab: state.tabs[index]!, chunks: tab.chunks, next: 0 }] : [],
    )
  }

  /**
   * Reads the rows of the saved tabs, a chunk at a time, the tab shown first
   * (`nextReading`), until they are all read or the session is closed. A
   * failure ends the reading of its tab, cut short; the others go on.
   *
   * @param readings Readings of the saved tabs.
   */
  async function readChunks(readings: ChunkReading[]): Promise<void> {
    for (
      let reading = nextReading(state.tabs, readings, shownTab());
      reading !== undefined && !closed;
      reading = nextReading(state.tabs, readings, shownTab())
    ) {
      const { tab } = reading
      const position = reading.next++
      try {
        const rows = await loadChunk(tab.id, position)
        receive(
          { type: 'saved_rows', tab, rows, last: reading.next === reading.chunks, cut: false },
          0,
        )
      } catch (failure) {
        reading.next = reading.chunks
        // A tab closed meanwhile may no longer exist on the server.
        if (state.tabs.includes(tab)) {
          state.error = errorMessage(failure)
          receive({ type: 'saved_rows', tab, rows: [], last: true, cut: true }, 0)
        }
      }
    }
  }

  // The list of the saved tabs first: the batch resumed by the WebSocket
  // finds its tabs among them. Their rows follow, while the session opens.
  state.loading = true
  readSaved()
    .catch((failure: unknown) => {
      state.error = errorMessage(failure)
      return []
    })
    .then((readings) => {
      state.loading = false
      connect()
      return readChunks(readings)
    })

  /**
   * Sends the editor's text, if the session is ready.
   *
   * @param sql Whole text of the editor.
   * @param target
   *     Queries to run: the one under the cursor, those of the selection (or
   *     only its text), or, by default, all of them, in parallel.
   * @param rowLimit Row limit; when absent, the one of the settings applies.
   */
  function run(sql: string, target: RunTarget = {}, rowLimit?: number): void {
    if (state.status !== 'ready') {
      return
    }
    startBatch(state)
    send({
      type: 'run',
      sql,
      cursor: target.cursor,
      selection: target.selection,
      exact: target.exact,
      row_limit: rowLimit,
    })
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
   * error, and those waiting for their turn do not start. A stopped refresh
   * gives its tab back as it was; the waiting refreshes are cancelled.
   */
  function cancel(): void {
    if (state.status === 'running') {
      state.refreshQueue = []
      for (const tab of state.tabs) {
        if (tab.previous) {
          tab.previous.stopped = true
        }
      }
      send({ type: 'cancel' })
    }
  }

  /**
   * Refreshes a tab: its query runs again, right away if the session is
   * ready, otherwise at the end of the running batch.
   *
   * @param tab Tab to refresh.
   */
  function refresh(tab: QueryTab): void {
    if (refreshState(state, tab) !== 'idle') {
      return
    }
    if (state.status === 'ready') {
      runRefresh(tab)
    } else {
      state.refreshQueue.push(tab.id)
    }
  }

  /**
   * Stops a tab's refresh, or cancels it while it waits: the tab keeps its
   * result as it was.
   *
   * @param tab Refreshed tab.
   */
  function stopRefresh(tab: QueryTab): void {
    state.refreshQueue = state.refreshQueue.filter((id) => id !== tab.id)
    if (tab.previous && tab.batch === state.batch && state.status === 'running') {
      tab.previous.stopped = true
      send({ type: 'cancel_query', query: tab.query })
    }
  }

  /**
   * Sends the refresh of a tab: its query alone, in the tab.
   *
   * @param tab Tab to refresh.
   */
  function runRefresh(tab: QueryTab): void {
    startRefresh(state, tab)
    send({ type: 'run', sql: tab.sql, tab: tab.id })
  }

  /**
   * Starts the next waiting refresh, once the session is ready again.
   */
  function startQueuedRefresh(): void {
    while (state.status === 'ready' && state.refreshQueue.length > 0) {
      const id = state.refreshQueue.shift()
      const tab = state.tabs.find((item) => item.id === id)
      if (tab) {
        runRefresh(tab)
      }
    }
  }

  /**
   * Stops the query of a tab, while it runs or waits for its turn: it ends
   * stopped (`interrupted`), with the rows received so far; the other
   * queries of a read batch go on. A refreshed tab takes back its result,
   * as `stopRefresh` does.
   *
   * @param tab Tab of the query.
   */
  function stopQuery(tab: QueryTab): void {
    if (tab.previous) {
      stopRefresh(tab)
    } else if (stoppable(state, tab)) {
      send({ type: 'cancel_query', query: tab.query })
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
    if (tab && stoppable(state, tab)) {
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

  return {
    state,
    run,
    changeDatabase,
    cancel,
    refresh,
    stopRefresh,
    stopQuery,
    closeTab,
    close,
  }
}
