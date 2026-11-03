// @vitest-environment node
import { readFileSync } from 'node:fs'

import enginePath from 'virtual:pivot-engine-path'
import { beforeAll, describe, expect, it, vi } from 'vitest'

import type { TypeFamily } from '@/api/generated/TypeFamily'
import type { Cell } from '@/api/querySession'

import {
  type PivotOrder,
  type PivotRequest,
  type PivotRequestFilter,
  type PivotRequestLevel,
  type PivotRequestTest,
  type WorkerState,
  FIRST_LAYOUT,
  LABEL_ORDER,
  PivotEngine,
  PivotError,
  answerMessage,
  familyCode,
  layoutWords,
  orderWords,
  ranksOf,
  requestWords,
  targetWords,
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
 * Level of the values of a column, as they are.
 *
 * @param column Index of the column.
 */
function level(column: number): PivotRequestLevel {
  return { column, precision: 'none' }
}

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
  it('tells numbers, floats, dates, instants and hours from the other families', () => {
    expect(
      ['integer', 'decimal', 'float', 'date', 'datetime', 'time', 'text', 'year'].map((family) =>
        familyCode(family as TypeFamily),
      ),
    ).toEqual([1, 1, 2, 3, 4, 5, 0, 0])
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
      rows: [level(0), level(1)],
      columns: [level(2)],
      measures: [
        { aggregate: 'sum', column: 4 },
        { aggregate: 'count_star', column: 1 },
        { aggregate: 'count_distinct', column: 1 },
      ],
      filters: [],
    }
    expect(Array.from(requestWords(request))).toEqual([2, 1, 3, 0, 0, 1, 0, 2, 0, 2, 4, 0, 1, 3, 1])
  })

  it('has a number per precision', () => {
    const request: PivotRequest = {
      rows: [
        { column: 7, precision: 'year' },
        { column: 7, precision: 'quarter' },
        { column: 7, precision: 'month' },
      ],
      columns: [
        { column: 7, precision: 'week' },
        { column: 7, precision: 'date' },
      ],
      measures: [],
      filters: [],
    }
    expect(Array.from(requestWords(request))).toEqual([3, 2, 0, 7, 1, 7, 2, 7, 3, 7, 4, 7, 5])
  })

  it('has a kind per aggregate', () => {
    const request: PivotRequest = {
      rows: [],
      columns: [],
      measures: [
        { aggregate: 'sum_distinct', column: 0 },
        { aggregate: 'avg', column: 0 },
        { aggregate: 'avg_distinct', column: 0 },
        { aggregate: 'min', column: 0 },
        { aggregate: 'max', column: 0 },
      ],
      filters: [],
    }
    expect(Array.from(requestWords(request))).toEqual([0, 0, 5, 4, 0, 5, 0, 6, 0, 7, 0, 8, 0])
  })
})

describe('orderWords', () => {
  it('writes each axis: its levels, then the values that rank it', () => {
    const order: PivotOrder = {
      rows: { descending: [true, false], values: null },
      columns: {
        descending: [],
        values: { path: ['é', null, 'a'], measure: 2, descending: true },
      },
    }
    expect(Array.from(orderWords(order))).toEqual([
      2, 1, 0, 0, 0, 1, 2, 1, 3, 2, 0xc3, 0xa9, 0xffff_ffff, 1, 0x61,
    ])
  })

  it('writes the order of a table just computed', () => {
    expect(Array.from(orderWords(LABEL_ORDER))).toEqual([0, 0, 0, 0])
  })
})

describe('layoutWords', () => {
  it('writes the form, then the totals of each axis', () => {
    const words = layoutWords({
      form: 'tabular',
      rows: { grandTotal: false, subtotals: [true, false] },
      columns: { grandTotal: true, subtotals: [] },
    })
    expect(Array.from(words)).toEqual([2, 0, 2, 1, 0, 1, 0])
    expect(Array.from(layoutWords({ ...FIRST_LAYOUT, form: 'outline' }))[0]).toBe(1)
  })

  it('writes the layout of a table just computed', () => {
    expect(Array.from(layoutWords(FIRST_LAYOUT))).toEqual([0, 1, 0, 1, 0])
  })
})

describe('PivotEngine', () => {
  it('counts the distinct values of a group again in its total', async () => {
    const engine = await loaded(EXAMPLE, EXAMPLE_FAMILIES, 4)
    engine.compute({
      rows: [level(0)],
      columns: [],
      measures: [{ aggregate: 'count_distinct', column: 1 }],
      filters: [],
    })
    // Nord: Lille and Arras; Sud: Nice and Toulon; the four in the total.
    expect(engine.window([0, 3], [0, 1]).cells).toEqual([['2'], ['2'], ['4']])
  })

  it('computes SUM(DISTINCT), AVG, AVG(DISTINCT), MIN and MAX with their totals', async () => {
    const engine = await loaded(EXAMPLE, EXAMPLE_FAMILIES, 4)
    engine.compute({
      rows: [level(0)],
      columns: [],
      measures: [
        { aggregate: 'sum_distinct', column: 2 },
        { aggregate: 'avg', column: 4 },
        { aggregate: 'avg_distinct', column: 2 },
        { aggregate: 'min', column: 1 },
        { aggregate: 'max', column: 4 },
      ],
      filters: [],
    })
    expect(engine.window([0, 3], [0, 5]).cells).toEqual([
      ['4051', '94.0000', '2025.5000', 'Arras', '150'],
      ['4051', '76.0000', '2025.5000', 'Nice', '110'],
      ['4051', '85.0000', '2025.5000', 'Arras', '150'],
    ])
  })

  it('computes a table from rows copied block by block', async () => {
    const engine = await loaded(EXAMPLE, EXAMPLE_FAMILIES, 3)
    const shape = engine.compute({
      rows: [level(0), level(1)],
      columns: [level(2)],
      measures: [{ aggregate: 'sum', column: 4 }],
      filters: [],
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

  it('groups the dates of a level by their precision, in the order of time', async () => {
    const rows: Cell[][] = [
      ['2027-01-01 12:00:00', '1'],
      ['2026-12-31 10:00:00', '2'],
      [null, '4'],
      ['2027-01-04 09:30:00', '8'],
      ['2026-03-15 08:00:00', '16'],
    ]
    const engine = await loaded(rows, ['datetime', 'integer'], 2)
    const table = (precision: PivotRequestLevel['precision']) => {
      engine.compute({
        rows: [{ column: 0, precision }],
        columns: [],
        measures: [{ aggregate: 'sum', column: 1 }],
        filters: [],
      })
      return engine.window([0, 9], [0, 1])
    }
    const labels = (precision: PivotRequestLevel['precision']) =>
      table(precision).lines.map((line) => line.label)
    expect(labels('year')).toEqual([null, '2026', '2027', null])
    expect(table('year').cells).toEqual([['4'], ['19'], ['8'], ['31']])
    expect(labels('quarter')).toEqual([null, '2026-T1', '2026-T4', '2027-T1', null])
    expect(labels('month')).toEqual([null, '2026-03', '2026-12', '2027-01', null])
    expect(labels('week')).toEqual([null, '2026-S11', '2026-S53', '2027-S01', null])
    expect(table('week').cells).toEqual([['4'], ['16'], ['3'], ['8'], ['31']])
    expect(labels('date')).toEqual([
      null,
      '2026-03-15',
      '2026-12-31',
      '2027-01-01',
      '2027-01-04',
      null,
    ])
  })

  it('refuses a level with a precision over a cell without a date', async () => {
    const engine = await loaded([['2026-01-15'], ['infinity']], ['date'])
    expect(() =>
      engine.compute({
        rows: [{ column: 0, precision: 'month' }],
        columns: [],
        measures: [],
        filters: [],
      }),
    ).toThrowError(
      expect.objectContaining({ failure: { kind: 'invalid_date', column: 0, row: 1 } }),
    )
    expect(engine.compute({ rows: [level(0)], columns: [], measures: [], filters: [] }).lines).toBe(
      3,
    )
  })

  it('gives a window bounded by the table', async () => {
    const engine = await loaded(EXAMPLE, EXAMPLE_FAMILIES)
    engine.compute({
      rows: [level(0)],
      columns: [],
      measures: [{ aggregate: 'count_star', column: 0 }],
      filters: [],
    })
    const window = engine.window([2, 50], [0, 50])
    expect(window.lines).toEqual([{ depth: 0, label: null, path: [], shared: 0 }])
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
      rows: [level(0), level(1)],
      columns: [],
      measures: [{ aggregate: 'count', column: 1 }],
      filters: [],
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
      rows: [level(1)],
      columns: [],
      measures: [{ aggregate: 'sum', column: 4 }],
      filters: [],
    })
    expect(shape.rows).toBe(4)
    expect(engine.window([0, 2], [0, 1]).cells).toEqual([['350'], ['350']])
    engine.setKept(null)
    expect(engine.compute({ rows: [], columns: [], measures: [], filters: [] }).rows).toBe(10)
  })

  it('refuses a cell that is not a number, and a sum beyond 128 bits', async () => {
    const engine = await loaded([['1'], ['abc']], ['decimal'])
    const sum: PivotRequest = {
      rows: [],
      columns: [],
      measures: [{ aggregate: 'sum', column: 0 }],
      filters: [],
    }
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
    engine.compute({
      rows: [level(0)],
      columns: [],
      measures: [{ aggregate: 'count_star', column: 0 }],
      filters: [],
    })
    expect(() =>
      engine.compute({
        rows: [],
        columns: [],
        measures: [{ aggregate: 'sum', column: 0 }],
        filters: [],
      }),
    ).toThrowError(PivotError)
    expect(engine.window([0, 3], [0, 1]).cells).toEqual([['5'], ['5'], ['10']])
  })

  describe('with an order', () => {
    /**
     * Table of the example: regions and villes by years, sum of the amounts.
     */
    const TABLE: PivotRequest = {
      rows: [level(0), level(1)],
      columns: [level(2)],
      measures: [{ aggregate: 'sum', column: 4 }],
      filters: [],
    }

    /**
     * Labels of the lines of a window of the table.
     *
     * @param engine Engine holding the table.
     */
    function labels(engine: PivotEngine): Cell[] {
      return engine.window([0, 7], [0, 3]).lines.map((line) => line.label)
    }

    /**
     * Labels of the columns of a window of the table.
     *
     * @param engine Engine holding the table.
     */
    function columns(engine: PivotEngine): (Cell | undefined)[] {
      return engine.window([0, 1], [0, 3]).columns.map((column) => column.path[0]?.[1])
    }

    it('reverses the labels of a level in descending order', async () => {
      const engine = await loaded(EXAMPLE, EXAMPLE_FAMILIES)
      engine.compute(TABLE, {
        rows: { descending: [true], values: null },
        columns: { descending: [true], values: null },
      })
      expect(labels(engine)).toEqual(['Sud', 'Nice', 'Toulon', 'Nord', 'Arras', 'Lille', null])
      expect(columns(engine)).toEqual(['2026', '2025', undefined])
    })

    it('ranks the lines by the values of a column, within their group', async () => {
      const engine = await loaded(EXAMPLE, EXAMPLE_FAMILIES)
      engine.compute(TABLE, {
        rows: { descending: [], values: { path: ['2025'], measure: 0, descending: true } },
        columns: { descending: [], values: null },
      })
      expect(labels(engine)).toEqual(['Nord', 'Lille', 'Arras', 'Sud', 'Nice', 'Toulon', null])
      const line = engine.window([1, 2], [0, 3])
      expect(line.lines[0]?.path).toEqual(['Nord', 'Lille'])
      expect(line.cells).toEqual([['220', '150', '370']])
    })

    it('ranks the columns by the values of a line, with a text that is not ASCII', async () => {
      const rows: Cell[][] = [
        ['Été', 'a', '1'],
        ['Été', 'b', '2'],
        [null, 'a', '4'],
        [null, 'b', '3'],
      ]
      const engine = await loaded(rows, ['text', 'text', 'integer'])
      const request: PivotRequest = {
        rows: [level(0)],
        columns: [level(1)],
        measures: [{ aggregate: 'sum', column: 2 }],
        filters: [],
      }
      const by = (path: Cell[]) => {
        engine.compute(request, {
          rows: { descending: [], values: null },
          columns: { descending: [], values: { path, measure: 0, descending: true } },
        })
        return columns(engine)
      }
      expect(by(['Été'])).toEqual(['b', 'a', undefined])
      expect(by([null])).toEqual(['a', 'b', undefined])
    })

    it('gives the position of the columns named by their values, in the order of the table', async () => {
      const engine = await loaded(EXAMPLE, EXAMPLE_FAMILIES)
      engine.compute(TABLE)
      const named = [
        { path: ['2026'], measure: 0 },
        { path: [], measure: 0 },
        { path: ['2027'], measure: 0 },
        { path: ['2025'], measure: 1 },
      ]
      expect(engine.columnsAt(named)).toEqual([1, 2, null, null])
      expect(engine.columnsAt([])).toEqual([])
      engine.compute(TABLE, {
        rows: { descending: [], values: null },
        columns: { descending: [true], values: null },
      })
      expect(engine.columnsAt(named.slice(0, 1))).toEqual([0])
      expect([...targetWords([{ path: ['é', null], measure: 2 }])]).toEqual([
        1, 2, 2, 2, 0xc3, 0xa9, 0xffff_ffff,
      ])
    })

    it('lays the table out again without computing it', async () => {
      const engine = await loaded(EXAMPLE, EXAMPLE_FAMILIES)
      // The exports of a module are frozen: the engine reads a copy that counts its computations.
      const held = engine as unknown as { engine: { compute: (...args: number[]) => number } }
      const compute = vi.fn<(...args: number[]) => number>(held.engine.compute)
      held.engine = { ...held.engine, compute }
      const descending = {
        rows: { descending: [true], values: null },
        columns: { descending: [], values: null },
      }
      engine.compute(TABLE)
      expect(compute).toHaveBeenCalledTimes(1)
      const shape = engine.compute(TABLE, descending)
      expect(compute).toHaveBeenCalledTimes(1)
      expect(shape).toMatchObject({ lines: 7, columns: 3, groups: 7, rows: 10 })
      expect(labels(engine)[0]).toBe('Sud')
      // Back to the labels, then a table of the rows kept: a new computation.
      engine.compute(TABLE)
      expect(labels(engine)[0]).toBe('Nord')
      expect(compute).toHaveBeenCalledTimes(1)
      engine.setKept(Uint32Array.from([5, 6]))
      expect(engine.compute(TABLE).rows).toBe(2)
      expect(compute).toHaveBeenCalledTimes(2)
    })

    it('gives the lines and the shape of a layout, without computing the table', async () => {
      const engine = await loaded(EXAMPLE, EXAMPLE_FAMILIES)
      // The exports of a module are frozen: the engine reads a copy that counts its computations.
      const held = engine as unknown as { engine: { compute: (...args: number[]) => number } }
      const compute = vi.fn<(...args: number[]) => number>(held.engine.compute)
      held.engine = { ...held.engine, compute }
      expect(engine.compute(TABLE)).toMatchObject({ lines: 7, columns: 3 })
      const shape = engine.compute(TABLE, LABEL_ORDER, {
        form: 'tabular',
        rows: { grandTotal: false, subtotals: [false, true] },
        columns: { grandTotal: false, subtotals: [] },
      })
      expect(shape).toMatchObject({ lines: 4, columns: 2, groups: 7, rows: 10 })
      const window = engine.window([0, 4], [0, 2])
      expect(window.lines.map(({ path, shared }) => [path, shared])).toEqual([
        [['Nord', 'Arras'], 0],
        [['Nord', 'Lille'], 1],
        [['Sud', 'Nice'], 0],
        [['Sud', 'Toulon'], 1],
      ])
      expect(window.cells[0]).toEqual(['40', '60'])
      expect(engine.compute(TABLE)).toMatchObject({ lines: 7, columns: 3 })
      expect(compute).toHaveBeenCalledTimes(1)
    })

    it('empties the line of a group without its subtotal', async () => {
      const engine = await loaded(EXAMPLE, EXAMPLE_FAMILIES)
      engine.compute(TABLE, LABEL_ORDER, {
        ...FIRST_LAYOUT,
        form: 'outline',
        rows: { grandTotal: true, subtotals: [false, true] },
      })
      const { lines, cells } = engine.window([0, 2], [0, 3])
      expect(lines[0]?.label).toBe('Nord')
      expect(cells).toEqual([
        [false, false, false],
        ['40', '60', '100'],
      ])
    })

    it('starts a table computed again from its labels', async () => {
      const engine = await loaded(EXAMPLE, EXAMPLE_FAMILIES)
      const descending = {
        rows: { descending: [true], values: null },
        columns: { descending: [], values: null },
      }
      engine.compute(TABLE, descending)
      expect(labels(engine)[0]).toBe('Sud')
      engine.compute({ ...TABLE, columns: [] }, descending)
      expect(engine.window([0, 1], [0, 1]).lines[0]?.label).toBe('Sud')
      engine.compute(TABLE)
      expect(labels(engine)[0]).toBe('Nord')
    })
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
    const state: WorkerState = {
      engine: await PivotEngine.create(module),
      columns: 0,
      table: null,
    }
    expect(answerMessage(state, { type: 'begin', families: EXAMPLE_FAMILIES })).toBeNull()
    expect(answerMessage(state, { type: 'rows', rows: EXAMPLE })).toBeNull()
    expect(answerMessage(state, { type: 'finish', id: 1 })).toEqual({ type: 'finished', id: 1 })
    expect(answerMessage(state, { type: 'kept', rows: null })).toBeNull()
    const request: PivotRequest = {
      rows: [],
      columns: [],
      measures: [{ aggregate: 'count_star', column: 0 }],
      filters: [],
    }
    const order = LABEL_ORDER
    const layout = FIRST_LAYOUT
    const targets = [{ path: [], measure: 0 }]
    const table = answerMessage(state, { type: 'compute', id: 2, request, order, layout, targets })
    expect(table).toMatchObject({
      type: 'table',
      id: 2,
      shape: { lines: 1, columns: 1 },
      positions: [0],
    })
    expect(
      answerMessage(state, {
        type: 'window',
        id: 3,
        table: 2,
        lines: [0, 1],
        columns: [0, 1],
      }),
    ).toEqual({
      type: 'window',
      id: 3,
      window: {
        lines: [{ depth: 0, label: null, path: [], shared: 0 }],
        columns: [{ node: 0, path: [], total: false, measure: 0 }],
        cells: [['10']],
      },
    })
    const sumOfText: PivotRequest = {
      ...request,
      measures: [{ aggregate: 'sum', column: 0 }],
      filters: [],
    }
    expect(
      answerMessage(state, {
        type: 'compute',
        id: 4,
        request: sumOfText,
        order,
        layout,
        targets: [],
      }),
    ).toEqual({
      type: 'failure',
      id: 4,
      failure: { kind: 'not_numeric', measure: 0 },
    })
    // The refused table leaves the previous one, whose windows are still read.
    expect(
      answerMessage(state, {
        type: 'window',
        id: 5,
        table: 2,
        lines: [0, 1],
        columns: [0, 1],
      }),
    ).toMatchObject({ type: 'window', id: 5 })
    answerMessage(state, { type: 'compute', id: 6, request, order, layout, targets: [] })
    expect(
      answerMessage(state, {
        type: 'window',
        id: 7,
        table: 2,
        lines: [0, 1],
        columns: [0, 1],
      }),
    ).toEqual({ type: 'failure', id: 7, failure: { kind: 'stale' } })
  })
})

describe('the filters', () => {
  /**
   * Labels of the lines of a table of `SUM(montant)` by ville under some filters, with their value.
   *
   * @param engine Engine.
   * @param filters Filters.
   */
  function lines(engine: PivotEngine, filters: PivotRequest['filters']): [Cell, unknown][] {
    const shape = engine.compute({
      rows: [level(1)],
      columns: [],
      measures: [{ aggregate: 'sum', column: 4 }],
      filters,
    })
    const window = engine.window([0, shape.lines], [0, shape.columns])
    return window.lines.map((line, index) => [line.label, window.cells[index]?.[0]])
  }

  it('writes each kind of filter after the measures', () => {
    const text = (value: string) => [value.length, ...Array.from(value, (c) => c.charCodeAt(0))]
    const words = requestWords({
      rows: [],
      columns: [],
      measures: [],
      filters: [
        {
          kind: 'values',
          level: { column: 2, precision: 'none' },
          values: [null, 'ab'],
          excluded: true,
        },
        {
          kind: 'measure',
          measure: { aggregate: 'sum', column: 4 },
          levels: [{ column: 1, precision: 'month' }],
          test: { condition: 'greater_or_equal', operand: '10', upper: '' },
        },
        {
          kind: 'column',
          level: { column: 3, precision: 'year' },
          test: { condition: 'between', operand: '1', upper: '2' },
        },
        {
          kind: 'column',
          level: { column: 3, precision: 'none' },
          test: { condition: 'not_between', operand: '1', upper: '2' },
        },
        {
          kind: 'column',
          level: { column: 3, precision: 'none' },
          test: { condition: 'is_null', operand: 'x', upper: '' },
        },
        {
          kind: 'column',
          level: { column: 3, precision: 'none' },
          test: { condition: 'is_not_null', operand: '', upper: '' },
        },
      ],
    })
    expect(Array.from(words)).toEqual([
      0,
      0,
      0,
      0,
      2,
      0,
      1,
      2,
      0xffff_ffff,
      ...text('ab'),
      1,
      2,
      4,
      1,
      1,
      3,
      5,
      ...text('10'),
      2,
      3,
      1,
      6,
      ...text('1'),
      ...text('2'),
      2,
      3,
      0,
      7,
      ...text('1'),
      ...text('2'),
      2,
      3,
      0,
      8,
      2,
      3,
      0,
      9,
    ])
  })

  it('gives a table that adds up, a filter on a measure counting the groups that remain', async () => {
    const engine = await loaded(EXAMPLE, EXAMPLE_FAMILIES)
    expect(
      lines(engine, [
        {
          kind: 'measure',
          measure: { aggregate: 'sum', column: 4 },
          levels: [level(1)],
          test: { condition: 'greater', operand: '100', upper: '' },
        },
      ]),
    ).toEqual([
      ['Lille', '370'],
      ['Nice', '350'],
      [null, '720'],
    ])
  })

  it('keeps the rows of some values, NULL included', async () => {
    const engine = await loaded(EXAMPLE, EXAMPLE_FAMILIES)
    const listed = (values: Cell[], excluded = false): PivotRequestFilter => ({
      kind: 'values',
      level: level(0),
      values,
      excluded,
    })
    expect(lines(engine, [listed(['Nord'])])).toEqual([
      ['Arras', '100'],
      ['Lille', '370'],
      [null, '470'],
    ])
    expect(lines(engine, [listed(['Nord', 'Sud'])])).toHaveLength(5)
    expect(lines(engine, [listed(['Nord'], true)])).toEqual([
      ['Nice', '350'],
      ['Toulon', '30'],
      [null, '380'],
    ])
  })

  it('keeps the rows whose value passes a test, numbers by value', async () => {
    const engine = await loaded(EXAMPLE, EXAMPLE_FAMILIES)
    const tested = (test: PivotRequestTest) =>
      lines(engine, [{ kind: 'column', level: level(4), test }])
    expect(tested({ condition: 'between', operand: '30', upper: '40' })).toEqual([
      ['Arras', '40'],
      ['Toulon', '30'],
      [null, '70'],
    ])
    expect(tested({ condition: 'greater', operand: '140', upper: '' })).toEqual([
      ['Lille', '150'],
      [null, '150'],
    ])
    expect(tested({ condition: 'is_null', operand: '', upper: '' })).toEqual([[null, null]])
  })

  it('reads a precision of the dates, for a test and for the list of its values', async () => {
    const dates = [['2026-01-15'], ['2026-03-31'], ['2026-04-01'], ['0000-00-00'], [null]]
    const engine = await loaded(dates, ['date'])
    expect(engine.values(0)).toEqual([null, '0000-00-00', '2026-01-15', '2026-03-31', '2026-04-01'])
    expect(engine.values(0, 'month')).toEqual([null, '2026-01', '2026-03', '2026-04'])
    const shape = (filters: PivotRequestFilter[]) =>
      engine.compute({ rows: [], columns: [], measures: [], filters })
    const months: PivotRequestFilter = {
      kind: 'column',
      level: { column: 0, precision: 'month' },
      test: { condition: 'between', operand: '2026-01', upper: '2026-03' },
    }
    expect(() => shape([months])).toThrowError(
      expect.objectContaining({ failure: { kind: 'invalid_date', column: 0, row: 3 } }),
    )
    const withoutZero: PivotRequestFilter = {
      kind: 'values',
      level: { column: 0, precision: 'none' },
      values: ['0000-00-00'],
      excluded: true,
    }
    expect(shape([months, withoutZero]).rows).toBe(2)
  })

  it('computes again when a filter changes', async () => {
    const engine = await loaded(EXAMPLE, EXAMPLE_FAMILIES)
    const totalOf = (operand: string) =>
      lines(engine, [
        {
          kind: 'measure',
          measure: { aggregate: 'sum', column: 4 },
          levels: [level(1)],
          test: { condition: 'greater', operand, upper: '' },
        },
      ]).pop()?.[1]
    expect(totalOf('100')).toBe('720')
    expect(totalOf('360')).toBe('370')
  })

  it('refuses an operand that cannot compare, naming the filter by its rank', async () => {
    const engine = await loaded(EXAMPLE, EXAMPLE_FAMILIES)
    const refused = () =>
      lines(engine, [
        { kind: 'values', level: level(0), values: ['Nord'], excluded: false },
        {
          kind: 'measure',
          measure: { aggregate: 'count_star', column: 0 },
          levels: [],
          test: { condition: 'equal', operand: 'abc', upper: '' },
        },
      ])
    expect(refused).toThrowError(
      expect.objectContaining({ failure: { kind: 'filter_operand', filter: 1 } }),
    )
  })

  it('gives the distinct values of a column, in the order of the grid', async () => {
    const engine = await loaded([['b'], [null], ['10'], ['9'], ['b']], ['text'])
    expect(engine.values(0)).toEqual([null, '9', '10', 'b'])
    const state: WorkerState = { engine, columns: 1, table: null }
    expect(answerMessage(state, { type: 'values', id: 3, column: 0, precision: 'none' })).toEqual({
      type: 'values',
      id: 3,
      values: [null, '9', '10', 'b'],
    })
    expect(answerMessage(state, { type: 'values', id: 4, column: 7, precision: 'none' })).toEqual({
      type: 'failure',
      id: 4,
      failure: expect.objectContaining({ kind: 'unknown_column' }),
    })
  })
})
