// @vitest-environment node
import { readFileSync } from 'node:fs'

import enginePath from 'virtual:pivot-engine-path'
import { beforeAll, describe, expect, it } from 'vitest'

import type { Cell, QueryResult } from '@/api/querySession'

import { BLOCK_ROWS, PivotClient, type PivotWorkerPort } from '../pivotClient'
import {
  type PivotAnswer,
  type PivotRequestMessage,
  type WorkerState,
  PivotEngine,
  PivotError,
  answerMessage,
} from '../pivotEngine'

/**
 * Module of the engine, built by Vite before the tests.
 */
let module: WebAssembly.Module

beforeAll(async () => {
  module = await WebAssembly.compile(readFileSync(enginePath))
})

/**
 * Worker that runs the engine in the page, answering each message after the current task, as a
 * Worker would.
 */
class PageWorker implements PivotWorkerPort {
  onmessage: ((event: MessageEvent<PivotAnswer>) => void) | null = null
  /**
   * Messages received, by type.
   */
  readonly received: string[] = []
  /**
   * Has the Worker been stopped?
   */
  terminated = false
  /**
   * Messages handled so far: each waits for the previous one.
   */
  private queue: Promise<WorkerState>

  constructor() {
    this.queue = PivotEngine.create(module).then((engine) => ({ engine, columns: 0 }))
  }

  postMessage(message: PivotRequestMessage): void {
    this.received.push(message.type)
    this.queue = this.queue.then((state) => {
      const answer = answerMessage(state, message)
      if (answer !== null && !this.terminated) {
        this.onmessage?.({ data: answer } as MessageEvent<PivotAnswer>)
      }
      return state
    })
  }

  terminate(): void {
    this.terminated = true
  }
}

/**
 * Received result, with its rows and the family of each column.
 *
 * @param rows Rows.
 * @param columns Names of the columns.
 * @param families Family of each column; none for a result saved before its columns were typed.
 */
function result(
  rows: Cell[][],
  columns: string[],
  families: QueryResult['columnTypes'] = [],
): QueryResult {
  return {
    sql: '',
    limit: 'added',
    expectedRows: null,
    executionUs: 0,
    executedAt: 0,
    columns,
    rightAligned: columns.map(() => false),
    columnTypes: families,
    rows,
    rowCount: rows.length,
    truncated: false,
    affectedRows: 0,
    elapsedMs: 0,
  }
}

describe('PivotClient', () => {
  it('copies the rows in blocks, then computes and reads a table', async () => {
    const count = BLOCK_ROWS * 2 + 1
    const rows: Cell[][] = Array.from({ length: count }, (_, row) => [
      row % 2 === 0 ? 'pair' : 'impair',
      String(row),
    ])
    const types = [
      { family: 'text' as const, name: 'VARCHAR(10)' },
      { family: 'integer' as const, name: 'INT' },
    ]
    const worker = new PageWorker()
    const client = new PivotClient(worker, result(rows, ['parite', 'n'], types))
    await client.loaded
    expect(worker.received).toEqual(['begin', 'rows', 'rows', 'rows', 'finish'])
    const table = await client.compute({
      rows: [0],
      columns: [],
      measures: [{ aggregate: 'sum', column: 1 }],
    })
    expect(table.shape).toMatchObject({ lines: 3, columns: 1, groups: 2, rows: count })
    expect(table.durationMs).toBeGreaterThanOrEqual(0)
    const window = await client.window([0, 3], [0, 1])
    // Sum of the odd numbers below 40,001, of the even ones, of all.
    expect(window.lines.map((line) => line.label)).toEqual(['impair', 'pair', null])
    expect(window.cells).toEqual([['400000000'], ['400020000'], ['800020000']])
  })

  it('takes the families missing from a saved result as other families', async () => {
    const client = new PivotClient(new PageWorker(), result([['1'], ['2']], ['n']))
    await client.loaded
    await expect(
      client.compute({ rows: [], columns: [], measures: [{ aggregate: 'sum', column: 0 }] }),
    ).rejects.toEqual(new PivotError({ kind: 'not_numeric', measure: 0 }))
  })

  it('measures the rows kept by the filter of the grid', async () => {
    const client = new PivotClient(new PageWorker(), result([['a'], ['b'], ['a']], ['x']))
    await client.loaded
    client.setKept([0, 2])
    const table = await client.compute({
      rows: [0],
      columns: [],
      measures: [{ aggregate: 'count_star', column: 0 }],
    })
    expect(table.shape.rows).toBe(2)
  })

  it('refuses the answers still awaited once closed', async () => {
    const worker = new PageWorker()
    const client = new PivotClient(worker, result([['a']], ['x']))
    await client.loaded
    const computing = client.compute({ rows: [], columns: [], measures: [] })
    client.close()
    expect(worker.terminated).toBe(true)
    await expect(computing).rejects.toThrowError('Pivot client closed')
    await expect(client.window([0, 1], [0, 1])).rejects.toThrowError('Pivot client closed')
  })
})
