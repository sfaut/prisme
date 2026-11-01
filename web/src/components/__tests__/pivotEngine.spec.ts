// @vitest-environment node
import { readFileSync } from 'node:fs'

import enginePath from 'virtual:pivot-engine-path'
import { beforeAll, describe, expect, it } from 'vitest'

import type { TypeFamily } from '@/api/generated/TypeFamily'
import type { Cell } from '@/api/querySession'

import {
  type PivotRequest,
  type WorkerState,
  PivotEngine,
  PivotError,
  answerMessage,
  familyCode,
  ranksOf,
  requestWords,
} from '../pivotEngine'
import { utf8Length } from '../pivotTransfer'

/**
 * Module of the engine, built by Vite before the tests (`pivotEngine` in `vite.config.ts`).
 */
let module: WebAssembly.Module

beforeAll(async () => {
  module = await WebAssembly.compile(readFileSync(enginePath))
})

/**
 * Example dataset of the specification: region, ville, annee, trimestre, montant.
 */
const EXAMPLE: Cell[][] = [
  ['Nord', 'Lille', '2025', 'T1', '100'],
  ['Nord', 'Lille', '2025', 'T2', '120'],
  ['Nord', 'Lille', '2026', 'T1', '150'],
  ['Nord', 'Arras', '2025', 'T1', '40'],
  ['Nord', 'Arras', '2026', 'T2', '60'],
  ['Sud', 'Nice', '2025', 'T1', '80'],
  ['Sud', 'Nice', '2025', 'T2', '90'],
  ['Sud', 'Nice', '2026', 'T1', '110'],
  ['Sud', 'Nice', '2026', 'T2', '70'],
  ['Sud', 'Toulon', '2026', 'T1', '30'],
]

const EXAMPLE_FAMILIES: TypeFamily[] = ['text', 'text', 'integer', 'text', 'decimal']

/**
 * Engine holding rows, copied in blocks of `block` rows.
 *
 * @param rows Rows of the result.
 * @param families Family of each column.
 * @param block Rows per block.
 */
async function loaded(rows: Cell[][], families: TypeFamily[], block = 4): Promise<PivotEngine> {
  const engine = await PivotEngine.create(module)
  engine.begin(families)
  for (let start = 0; start < rows.length; start += block) {
    engine.append(rows.slice(start, start + block), families.length)
  }
  engine.finish()
  return engine
}

describe('utf8Length', () => {
  it('counts the bytes of each character as TextEncoder writes them', () => {
    for (const text of ['', 'abc', 'é', '€', '😀', 'a\ud800b']) {
      expect(utf8Length(text)).toBe(new TextEncoder().encode(text).length)
    }
  })
})

describe('familyCode', () => {
  it('tells exact numbers and floats from the other families', () => {
    expect(
      ['integer', 'decimal', 'float', 'text', 'date'].map((family) =>
        familyCode(family as TypeFamily),
      ),
    ).toEqual([1, 1, 2, 0, 0])
  })
})

describe('ranksOf', () => {
  it('ranks the values in the order of the grid', () => {
    // NULL first, numbers by value, texts by the collator, digits included.
    expect(Array.from(ranksOf(['10', null, '9', 'a10', 'a2', 'B']))).toEqual([2, 0, 1, 4, 3, 5])
  })
})

describe('requestWords', () => {
  it('writes the levels then a kind and a column per measure', () => {
    const request: PivotRequest = {
      rows: [0, 1],
      columns: [2],
      measures: [
        { aggregate: 'sum', column: 4 },
        { aggregate: 'count_star', column: 1 },
      ],
    }
    expect(Array.from(requestWords(request))).toEqual([2, 1, 2, 0, 1, 2, 2, 4, 0, 1])
  })
})

describe('PivotEngine', () => {
  it('computes a table from rows copied block by block', async () => {
    const engine = await loaded(EXAMPLE, EXAMPLE_FAMILIES, 3)
    const shape = engine.compute({
      rows: [0, 1],
      columns: [2],
      measures: [{ aggregate: 'sum', column: 4 }],
    })
    expect(shape).toEqual({
      lines: 7,
      columns: 3,
      rowLevels: 2,
      columnLevels: 1,
      groups: 7,
      rows: 10,
    })
    const window = engine.window([0, 7], [0, 3])
    expect(window.lines.map((line) => [line.depth, line.label])).toEqual([
      [1, 'Nord'],
      [2, 'Arras'],
      [2, 'Lille'],
      [1, 'Sud'],
      [2, 'Nice'],
      [2, 'Toulon'],
      [0, null],
    ])
    expect(window.columns).toEqual([
      { node: 1, path: [[1, '2025']], total: false, measure: 0 },
      { node: 2, path: [[2, '2026']], total: false, measure: 0 },
      { node: 0, path: [], total: true, measure: 0 },
    ])
    expect(window.cells).toEqual([
      ['260', '210', '470'],
      ['40', '60', '100'],
      ['220', '150', '370'],
      ['170', '210', '380'],
      ['170', '180', '350'],
      [false, '30', '30'],
      ['430', '420', '850'],
    ])
  })

  it('gives a window bounded by the table', async () => {
    const engine = await loaded(EXAMPLE, EXAMPLE_FAMILIES)
    engine.compute({ rows: [0], columns: [], measures: [{ aggregate: 'count_star', column: 0 }] })
    const window = engine.window([2, 50], [0, 50])
    expect(window.lines).toEqual([{ depth: 0, label: null }])
    expect(window.cells).toEqual([['10']])
  })

  it('ranks numbers by value and keeps texts that are not ASCII', async () => {
    const rows: Cell[][] = [
      ['10', 'Été'],
      ['9', null],
      ['10', 'ça'],
      ['9', '😀'],
    ]
    const engine = await loaded(rows, ['integer', 'text'], 3)
    engine.compute({
      rows: [0, 1],
      columns: [],
      measures: [{ aggregate: 'count', column: 1 }],
    })
    const window = engine.window([0, 10], [0, 1])
    expect(window.lines.map((line) => line.label)).toEqual([
      '9',
      null,
      '😀',
      '10',
      'ça',
      'Été',
      null,
    ])
    expect(window.cells.map((cells) => cells[0])).toEqual(['1', '0', '1', '2', '1', '1', '3'])
  })

  it('measures only the rows kept by the filter of the grid', async () => {
    const engine = await loaded(EXAMPLE, EXAMPLE_FAMILIES)
    engine.setKept(Uint32Array.from([5, 6, 7, 8]))
    const shape = engine.compute({
      rows: [1],
      columns: [],
      measures: [{ aggregate: 'sum', column: 4 }],
    })
    expect(shape.rows).toBe(4)
    expect(engine.window([0, 2], [0, 1]).cells).toEqual([['350'], ['350']])
    engine.setKept(null)
    expect(engine.compute({ rows: [], columns: [], measures: [] }).rows).toBe(10)
  })

  it('refuses a cell that is not a number, and a sum beyond 128 bits', async () => {
    const engine = await loaded([['1'], ['abc']], ['decimal'])
    const sum: PivotRequest = { rows: [], columns: [], measures: [{ aggregate: 'sum', column: 0 }] }
    expect(() => engine.compute(sum)).toThrowError(PivotError)
    expect(() => engine.compute(sum)).toThrowError(
      expect.objectContaining({ failure: { kind: 'invalid_number', column: 0, row: 1 } }),
    )
    const largest = await loaded([['170141183460469231731687303715884105727'], ['1']], ['integer'])
    expect(() => largest.compute(sum)).toThrowError(
      expect.objectContaining({ failure: { kind: 'overflow', measure: 0 } }),
    )
  })

  it('keeps the last table when a computation is refused', async () => {
    const engine = await loaded(EXAMPLE, EXAMPLE_FAMILIES)
    engine.compute({ rows: [0], columns: [], measures: [{ aggregate: 'count_star', column: 0 }] })
    expect(() =>
      engine.compute({ rows: [], columns: [], measures: [{ aggregate: 'sum', column: 0 }] }),
    ).toThrowError(PivotError)
    expect(engine.window([0, 3], [0, 1]).cells).toEqual([['5'], ['5'], ['10']])
  })

  it('refuses a window before any table', async () => {
    const engine = await loaded(EXAMPLE, EXAMPLE_FAMILIES)
    expect(() => engine.window([0, 1], [0, 1])).toThrowError(
      expect.objectContaining({ failure: { kind: 'no_table' } }),
    )
  })
})

describe('answerMessage', () => {
  it('answers the messages of the page in their order', async () => {
    const state: WorkerState = { engine: await PivotEngine.create(module), columns: 0 }
    expect(answerMessage(state, { type: 'begin', families: EXAMPLE_FAMILIES })).toBeNull()
    expect(answerMessage(state, { type: 'rows', rows: EXAMPLE })).toBeNull()
    expect(answerMessage(state, { type: 'finish', id: 1 })).toEqual({ type: 'finished', id: 1 })
    expect(answerMessage(state, { type: 'kept', rows: null })).toBeNull()
    const request: PivotRequest = {
      rows: [],
      columns: [],
      measures: [{ aggregate: 'count_star', column: 0 }],
    }
    const table = answerMessage(state, { type: 'compute', id: 2, request })
    expect(table).toMatchObject({ type: 'table', id: 2, shape: { lines: 1, columns: 1 } })
    expect(answerMessage(state, { type: 'window', id: 3, lines: [0, 1], columns: [0, 1] })).toEqual(
      {
        type: 'window',
        id: 3,
        window: {
          lines: [{ depth: 0, label: null }],
          columns: [{ node: 0, path: [], total: false, measure: 0 }],
          cells: [['10']],
        },
      },
    )
    const sumOfText: PivotRequest = { ...request, measures: [{ aggregate: 'sum', column: 0 }] }
    expect(answerMessage(state, { type: 'compute', id: 4, request: sumOfText })).toEqual({
      type: 'failure',
      id: 4,
      failure: { kind: 'not_numeric', measure: 0 },
    })
  })
})
