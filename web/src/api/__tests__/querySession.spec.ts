import { describe, expect, it } from 'vitest'

import {
  applyMessage,
  executionDuration,
  parseMessage,
  refreshState,
  startRefresh,
  formatBytes,
  formatDuration,
  formatInstant,
  initialState,
  removeTab,
  savedTab,
  sessionUrl,
  startBatch,
  tabFraction,
  tabProgress,
  transferDuration,
  utf8Length,
} from '../querySession'
import type { ErrorDetail } from '../generated/ErrorDetail'
import type { ServerMessage } from '../generated/ServerMessage'
import { describe as describeError } from '../messages'

/**
 * Error message of the server, for the query `query` (`null`: the session).
 */
function failed(query: number | null, detail: ErrorDetail): ServerMessage {
  return { type: 'error', query, message: '', detail }
}

/**
 * Failure of a query whose connection broke.
 */
const LOST: ErrorDetail = {
  code: 'query_failed',
  flavor: 'mariadb',
  cause: { source: 'driver', message: 'Connection to the server is closed.' },
}

/**
 * State of a ready session, on the `xlist` database.
 */
function readyState() {
  const state = initialState()
  applyMessage(state, {
    type: 'ready',
    server: { flavor: 'mariadb', version: '11.3.2', ansi_quotes: false },
    database: 'xlist',
  })
  return state
}

describe('applyMessage', () => {
  it('follows a batch of two queries, each in its tab', () => {
    const state = readyState()
    expect(state.status).toBe('ready')
    expect(state.database).toBe('xlist')

    startBatch(state)
    expect(state.status).toBe('running')
    applyMessage(state, {
      type: 'batch',
      queries: [
        { query: 0, tab: 0, sql: 'select * from video', start: 0, end: 20 },
        { query: 1, tab: 0, sql: 'select * from person', start: 21, end: 42 },
      ],
    })
    expect(state.tabs.map((tab) => tab.sql)).toEqual([
      'select * from video',
      'select * from person',
    ])

    // The messages of the two queries interleave.
    applyMessage(state, {
      type: 'columns',
      query: 1,
      columns: ['id'],
      right_aligned: [true],
      types: [],
      sql: 'x',
      limit: 'added',
      expected_rows: null,
      execution_us: 0,
      executed_at: 0,
    })
    applyMessage(state, {
      type: 'columns',
      query: 0,
      columns: ['id', 'title'],
      right_aligned: [true, false],
      types: [],
      sql: 'y',
      limit: 'added',
      expected_rows: null,
      execution_us: 0,
      executed_at: 0,
    })
    applyMessage(state, { type: 'rows', query: 0, rows: [['1', 'a']] })
    applyMessage(state, { type: 'rows', query: 1, rows: [['7'], ['8']] })
    applyMessage(state, {
      type: 'done',
      query: 1,
      row_count: 2,
      truncated: false,
      affected_rows: 0,
      elapsed_ms: 5,
      database: 'xlist',
    })
    applyMessage(state, failed(0, { code: 'query_interrupted' }))
    applyMessage(state, { type: 'batch_done' })

    const [video, person] = state.tabs
    expect(video?.result?.rows).toEqual([['1', 'a']])
    expect(video?.error).toBe('Requête interrompue par l’utilisateur.')
    expect(video?.done).toBe(true)
    expect(person?.result?.rows).toEqual([['7'], ['8']])
    expect(person?.result?.elapsedMs).toBe(5)
    expect(person?.done).toBe(true)
    expect(state.status).toBe('ready')
  })

  it('follows the current database at the end of each query', () => {
    const state = readyState()
    startBatch(state)
    applyMessage(state, {
      type: 'batch',
      queries: [{ query: 0, tab: 0, sql: 'USE shop', start: 0, end: 8 }],
    })
    applyMessage(state, {
      type: 'done',
      query: 0,
      row_count: 0,
      truncated: false,
      affected_rows: 0,
      elapsed_ms: 1,
      database: 'shop',
    })
    expect(state.database).toBe('shop')
  })

  it('follows the database chosen by a use request', () => {
    const state = readyState()
    applyMessage(state, { type: 'database', database: 'shop' })
    expect(state.database).toBe('shop')
    expect(state.status).toBe('ready')
  })

  it('keeps the session open after a session error', () => {
    const state = readyState()
    startBatch(state)
    applyMessage(state, failed(null, { code: 'no_query' }))
    expect(state.status).toBe('ready')
    expect(state.error).toBe('Aucune requête à exécuter.')
  })

  it('closes the session if the connection to the target server fails', () => {
    const state = initialState()
    applyMessage(state, failed(null, { code: 'missing_version' }))
    expect(state.status).toBe('closed')
  })

  it('clears the previous tabs and error at the start of a batch, except the pinned tabs', () => {
    const state = readyState()
    startBatch(state)
    applyMessage(state, {
      type: 'batch',
      queries: [
        { query: 0, tab: 0, sql: 'a', start: 0, end: 1 },
        { query: 1, tab: 0, sql: 'b', start: 2, end: 3 },
      ],
    })
    state.tabs[1]!.pinned = true
    state.error = 'ancienne erreur'

    startBatch(state)
    expect(state.error).toBeNull()
    expect(state.tabs.map((tab) => tab.sql)).toEqual(['b'])

    // The new batch follows the pinned tab; its messages do not touch the latter,
    // which yet carried the same query number in the old batch.
    applyMessage(state, {
      type: 'batch',
      queries: [{ query: 1, tab: 0, sql: 'c', start: 0, end: 1 }],
    })
    applyMessage(state, { type: 'started', query: 1 }, 1000)
    expect(state.tabs.map((tab) => [tab.sql, tab.startedAt])).toEqual([
      ['b', null],
      ['c', 1000],
    ])
    expect(new Set(state.tabs.map((tab) => tab.id)).size).toBe(2)
  })

  it('removes a closed tab', () => {
    const state = readyState()
    startBatch(state)
    applyMessage(state, {
      type: 'batch',
      queries: [{ query: 0, tab: 0, sql: 'a', start: 0, end: 1 }],
    })
    const id = state.tabs[0]!.id
    expect(removeTab(state, id)?.sql).toBe('a')
    expect(state.tabs).toEqual([])
    expect(removeTab(state, id)).toBeUndefined()
  })
})

describe('tabProgress', () => {
  it('goes from waiting to executing, then loading, then done or error', () => {
    const state = readyState()
    startBatch(state)
    applyMessage(state, {
      type: 'batch',
      queries: [
        { query: 0, tab: 0, sql: 'a', start: 0, end: 1 },
        { query: 1, tab: 0, sql: 'b', start: 2, end: 3 },
      ],
    })
    const [first, second] = state.tabs
    expect(tabProgress(first!)).toBe('waiting')

    applyMessage(state, { type: 'started', query: 0 }, 1000)
    expect(tabProgress(first!)).toBe('executing')
    expect(first?.startedAt).toBe(1000)

    applyMessage(state, {
      type: 'columns',
      query: 0,
      columns: ['x'],
      right_aligned: [false],
      types: [],
      sql: 'a',
      limit: 'added',
      expected_rows: null,
      execution_us: 0,
      executed_at: 0,
    })
    expect(tabProgress(first!)).toBe('loading')

    applyMessage(state, {
      type: 'done',
      query: 0,
      row_count: 0,
      truncated: false,
      affected_rows: 0,
      elapsed_ms: 1,
      database: null,
    })
    expect(tabProgress(first!)).toBe('done')

    applyMessage(state, failed(1, { code: 'write_read_only' }))
    expect(tabProgress(second!)).toBe('error')
  })
})

describe('durations', () => {
  it('separates the execution from the transfer of the rows', () => {
    const state = readyState()
    startBatch(state)
    applyMessage(state, {
      type: 'batch',
      queries: [
        { query: 0, tab: 0, sql: 'a', start: 0, end: 1 },
        { query: 1, tab: 0, sql: 'b', start: 2, end: 3 },
      ],
    })
    const tab = state.tabs[0]!
    expect(executionDuration(tab, 0)).toBeNull()

    applyMessage(state, { type: 'started', query: 0 }, 1000)
    expect(executionDuration(tab, 1300)).toBe(300)
    expect(transferDuration(tab, 1300)).toBeNull()

    applyMessage(
      state,
      {
        type: 'columns',
        query: 0,
        columns: ['x'],
        right_aligned: [false],
        types: [],
        sql: 'a',
        limit: 'added',
        expected_rows: null,
        execution_us: 0,
        executed_at: 0,
      },
      1400,
    )
    // Columns received: the time measured by the server (here 0 µs in the message).
    tab.result!.executionUs = 380_500
    expect(executionDuration(tab, 2000)).toBe(380.5)
    expect(transferDuration(tab, 2000)).toBe(600)

    applyMessage(
      state,
      {
        type: 'done',
        query: 0,
        row_count: 0,
        truncated: false,
        affected_rows: 0,
        elapsed_ms: 500,
        database: null,
      },
      2500,
    )
    expect(executionDuration(tab, 9999)).toBe(380.5)
    expect(transferDuration(tab, 9999)).toBe(1100)
  })

  it('measures the completed share of a query, when it is known', () => {
    const state = readyState()
    startBatch(state)
    applyMessage(state, {
      type: 'batch',
      queries: [{ query: 0, tab: 0, sql: 'a', start: 0, end: 1 }],
    })
    const tab = state.tabs[0]!
    expect(tabFraction(tab)).toBe(0)
    applyMessage(state, { type: 'started', query: 0 })
    expect(tabFraction(tab)).toBeNull()
    applyMessage(state, {
      type: 'columns',
      query: 0,
      columns: ['x'],
      right_aligned: [false],
      types: [],
      sql: 'a LIMIT 101',
      limit: 'added',
      expected_rows: 100,
      execution_us: 0,
      executed_at: 0,
    })
    applyMessage(state, { type: 'rows', query: 0, rows: [['1'], ['2'], ['3'], ['4']] })
    expect(tabFraction(tab)).toBe(0.04)
    tab.result!.expectedRows = null
    expect(tabFraction(tab)).toBeNull()
    applyMessage(state, failed(0, LOST))
    expect(tabFraction(tab)).toBe(1)
  })

  it('counts the volume received per query', () => {
    const state = readyState()
    startBatch(state)
    applyMessage(
      state,
      { type: 'batch', queries: [{ query: 0, tab: 0, sql: 'a', start: 0, end: 1 }] },
      0,
      50,
    )
    applyMessage(
      state,
      {
        type: 'columns',
        query: 0,
        columns: ['x'],
        right_aligned: [false],
        types: [],
        sql: 'a',
        limit: 'added',
        expected_rows: null,
        execution_us: 0,
        executed_at: 0,
      },
      0,
      120,
    )
    applyMessage(state, { type: 'rows', query: 0, rows: [['1']] }, 0, 30)
    expect(state.tabs[0]!.bytes).toBe(150)
  })

  it('measures a UTF-8 text in bytes', () => {
    expect(utf8Length('abc')).toBe(3)
    expect(utf8Length('é')).toBe(2)
    expect(utf8Length('€')).toBe(3)
    expect(utf8Length('😀')).toBe(4)
  })

  it('formats the volumes', () => {
    expect(formatBytes(0)).toBe('0 o')
    expect(formatBytes(999)).toBe('999 o')
    expect(formatBytes(1000)).toBe('1,0 ko')
    expect(formatBytes(1_234_567)).toBe('1,2 Mo')
    expect(formatBytes(5_600_000_000)).toBe('5,6 Go')
  })

  it('formats the durations', () => {
    expect(formatDuration(0.25)).toBe('250 µs')
    expect(formatDuration(0.9994)).toBe('999 µs')
    expect(formatDuration(42)).toBe('42 ms')
    expect(formatDuration(999)).toBe('999 ms')
    expect(formatDuration(1000)).toBe('1,0 s')
    expect(formatDuration(12345)).toBe('12,3 s')
    expect(formatDuration(59_999)).toBe('59,9 s')
    expect(formatDuration(60_000)).toBe('1 min 00 s')
    expect(formatDuration(187_400)).toBe('3 min 07 s')
    expect(formatDuration(3_600_000)).toBe('1 h 00 min')
    expect(formatDuration(3_725_000)).toBe('1 h 02 min')
  })

  it('formats an instant to the second, in local time', () => {
    expect(formatInstant(new Date(2026, 9, 20, 14, 32, 5, 900).getTime())).toBe(
      '20/10/2026 14:32:05',
    )
  })
})

describe('savedTab', () => {
  it('takes back a saved tab, outside the batches of the session', () => {
    const tab = savedTab({
      id: 42,
      query: 1,
      sql: 'SELECT x FROM t',
      name: 'Gardé',
      pinned: true,
      result: {
        started: true,
        done: true,
        error: null,
        error_detail: null,
        columns: {
          columns: ['x'],
          right_aligned: [true],
          types: [],
          sql: 'SELECT x FROM t LIMIT 1001',
          limit: 'added',
          expected_rows: 1000,
          execution_us: 1500,
          executed_at: 1_792_500_000_000,
        },
        row_count: 2,
        truncated: false,
        affected_rows: 0,
        transfer_us: 4000,
        bytes: 120,
      },
      rows: [['1'], ['2']],
    })
    expect([tab.id, tab.batch, tab.pinned, tab.name]).toEqual([42, 0, true, 'Gardé'])
    expect(tab.result?.rows).toEqual([['1'], ['2']])
    expect(tab.result?.executedAt).toBe(1_792_500_000_000)
    expect(executionDuration(tab, 0)).toBe(1.5)
    expect(transferDuration(tab, 0)).toBe(4)
    expect(tabProgress(tab)).toBe('done')
  })
})

describe('resume', () => {
  it('takes back the last batch in its tabs, which the next messages continue', () => {
    const state = readyState()
    state.tabs = [
      savedTab({
        id: 7,
        query: 0,
        sql: 'SELECT 1',
        name: 'Mon onglet',
        pinned: false,
        result: {
          started: false,
          done: false,
          error: null,
          error_detail: null,
          columns: null,
          row_count: 0,
          truncated: false,
          affected_rows: 0,
          transfer_us: 0,
          bytes: 0,
        },
        rows: [],
      }),
    ]
    const running = {
      started: true,
      done: false,
      error: null,
      error_detail: null,
      columns: {
        columns: ['x'],
        right_aligned: [true],
        types: [],
        sql: 'SELECT x FROM t LIMIT 1001',
        limit: 'added' as const,
        expected_rows: 1000,
        execution_us: 2000,
        executed_at: 0,
      },
      row_count: 1,
      truncated: false,
      affected_rows: 0,
      transfer_us: 0,
      bytes: 50,
    }
    applyMessage(
      state,
      {
        type: 'resume',
        running: true,
        queries: [
          {
            query: 0,
            tab: 7,
            sql: 'SELECT x FROM t',
            result: running,
            rows: [['1']],
            started_us_ago: 5_000_000,
            columns_us_ago: 3_000_000,
            refresh: false,
          },
          {
            query: 1,
            tab: 8,
            sql: 'SELECT 2',
            result: { ...running, columns: null, started: false, row_count: 0 },
            rows: [],
            started_us_ago: null,
            columns_us_ago: null,
            refresh: false,
          },
        ],
      },
      10_000,
    )
    expect(state.status).toBe('running')
    expect(state.tabs.map((tab) => [tab.id, tab.name, tab.batch])).toEqual([
      [7, 'Mon onglet', 1],
      [8, null, 1],
    ])
    const [first, second] = state.tabs
    expect([first!.startedAt, first!.columnsAt]).toEqual([5_000, 7_000])
    expect(tabProgress(second!)).toBe('waiting')

    applyMessage(state, { type: 'rows', query: 0, rows: [['2']] }, 10_100)
    expect(first!.result?.rowCount).toBe(2)
    expect(transferDuration(first!, 10_100)).toBe(3_100)
  })
})

/**
 * Columns message of the query number `query`, executed at `executedAt`.
 */
function columns(query: number, executedAt: number): ServerMessage {
  return {
    type: 'columns',
    query,
    columns: ['x'],
    right_aligned: [true],
    types: [],
    sql: 'SELECT x FROM t',
    limit: 'added',
    expected_rows: null,
    execution_us: 0,
    executed_at: executedAt,
  }
}

/**
 * Done message of the query number `query`.
 */
function done(query: number): ServerMessage {
  return {
    type: 'done',
    query,
    row_count: 1,
    truncated: false,
    affected_rows: 0,
    elapsed_ms: 1,
    database: null,
  }
}

/**
 * Ready session with two ended tabs, ids 7 and 8: its second query's tab,
 * with a row executed at 1000.
 */
function stateWithResult() {
  const state = readyState()
  startBatch(state)
  applyMessage(state, {
    type: 'batch',
    queries: [
      { query: 0, tab: 7, sql: 'SELECT 1', start: 0, end: 8 },
      { query: 1, tab: 8, sql: 'SELECT x FROM t', start: 9, end: 24 },
    ],
  })
  for (const query of [0, 1]) {
    applyMessage(state, columns(query, 1000))
    applyMessage(state, { type: 'rows', query, rows: [['old']] })
    applyMessage(state, done(query))
  }
  applyMessage(state, { type: 'batch_done' })
  return { state, tab: state.tabs[1]! }
}

describe('refresh', () => {
  it('runs the query again in its tab, the other tabs and its number kept', () => {
    const { state, tab } = stateWithResult()
    expect(refreshState(state, tab)).toBe('idle')

    startRefresh(state, tab)
    expect(state.status).toBe('running')
    expect(refreshState(state, tab)).toBe('running')
    expect(refreshState(state, state.tabs[0]!)).toBe('idle')
    // The previous result stays until the new columns arrive.
    expect(tab.result).toBeNull()
    expect(tab.previous?.result?.rows).toEqual([['old']])

    applyMessage(state, {
      type: 'batch',
      queries: [{ query: 0, tab: 8, sql: 'SELECT x FROM t', start: 0, end: 15 }],
    })
    expect(state.tabs.map((item) => item.id)).toEqual([7, 8])
    applyMessage(state, columns(0, 2000))
    applyMessage(state, { type: 'rows', query: 0, rows: [['new']] })
    applyMessage(state, done(0))
    applyMessage(state, { type: 'batch_done' })

    expect(tab.result?.rows).toEqual([['new']])
    expect(tab.result?.executedAt).toBe(2000)
    expect([tab.previous, tab.number]).toEqual([null, 1])
    expect(refreshState(state, tab)).toBe('idle')
  })

  it('gives the tab back as it was when stopped, with the error when it fails', () => {
    const { state, tab } = stateWithResult()
    const before = { ...tab }

    startRefresh(state, tab)
    applyMessage(state, columns(0, 2000))
    applyMessage(state, { type: 'rows', query: 0, rows: [['new']] })
    tab.previous!.stopped = true
    applyMessage(state, failed(0, { code: 'query_interrupted' }))
    expect(tab.result?.rows).toEqual([['old']])
    expect([tab.error, tab.done, tab.previous]).toEqual([null, true, null])
    expect({ ...tab, batch: before.batch, query: before.query }).toEqual(before)
    applyMessage(state, { type: 'batch_done' })

    startRefresh(state, tab)
    applyMessage(state, failed(0, LOST))
    expect(tab.result?.rows).toEqual([['old']])
    expect([tab.error, tab.previous]).toEqual([describeError(LOST), null])
    applyMessage(state, { type: 'batch_done' })
    expect(refreshState(state, tab)).toBe('idle')
  })

  it('waits in the queue during a batch, and is not offered for a running query', () => {
    const { state, tab } = stateWithResult()
    state.refreshQueue.push(tab.id)
    expect(refreshState(state, tab)).toBe('queued')
    removeTab(state, tab.id)
    expect(state.refreshQueue).toEqual([])

    startBatch(state)
    applyMessage(state, {
      type: 'batch',
      queries: [{ query: 0, tab: 9, sql: 'SELECT 1', start: 0, end: 8 }],
    })
    const running = state.tabs[0]!
    expect(refreshState(state, running)).toBe('unavailable')
    applyMessage(state, done(0))
    expect(refreshState(state, running)).toBe('idle')
    state.status = 'closed'
    expect(refreshState(state, running)).toBe('unavailable')
  })

  it('keeps the previous result of a refresh resumed after a reconnection', () => {
    const { state, tab } = stateWithResult()
    const running = {
      started: true,
      done: false,
      error: null,
      error_detail: null,
      columns: null,
      row_count: 0,
      truncated: false,
      affected_rows: 0,
      transfer_us: 0,
      bytes: 0,
    }
    applyMessage(state, {
      type: 'resume',
      running: true,
      queries: [
        {
          query: 0,
          tab: 8,
          sql: 'SELECT x FROM t',
          result: running,
          rows: [],
          started_us_ago: 1000,
          columns_us_ago: null,
          refresh: true,
        },
      ],
    })
    const resumed = state.tabs[1]!
    expect([resumed.id, resumed.number, resumed.result]).toEqual([8, 1, null])
    expect(resumed.previous?.result?.rows).toEqual(tab.result?.rows)

    applyMessage(state, failed(0, { code: 'query_interrupted' }))
    expect(resumed.result?.rows).toEqual([['old']])
  })
})

describe('sessionUrl', () => {
  it('follows the protocol of the page', () => {
    expect(sessionUrl(3, { protocol: 'http:', host: 'localhost:5173' } as Location)).toBe(
      'ws://localhost:5173/api/editors/3/session',
    )
    expect(sessionUrl(3, { protocol: 'https:', host: 'prisme.example' } as Location)).toBe(
      'wss://prisme.example/api/editors/3/session',
    )
  })
})

describe('parseMessage', () => {
  it('reads a message of the server', () => {
    expect(parseMessage('{"type":"batch_done"}')).toEqual({ type: 'batch_done' })
  })

  it('returns null for a text that is not JSON, instead of throwing', () => {
    expect(parseMessage('{"type":')).toBeNull()
    expect(parseMessage('')).toBeNull()
  })
})
