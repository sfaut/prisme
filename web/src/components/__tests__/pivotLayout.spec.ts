import { describe, expect, it } from 'vitest'

import type { Cell } from '@/api/querySession'

import type { PivotColumn } from '../pivotEngine'
import {
  cornerRows,
  headerCells,
  headerRowCount,
  lineLabel,
  sameLayout,
  sortMark,
} from '../pivotLayout'

/**
 * Columns of the years and quarters of the example, each year closed by its subtotal, then the
 * grand total.
 */
const QUARTERS: PivotColumn[] = [
  {
    node: 3,
    path: [
      [1, '2025'],
      [3, 'T1'],
    ],
    total: false,
    measure: 0,
  },
  {
    node: 4,
    path: [
      [1, '2025'],
      [4, 'T2'],
    ],
    total: false,
    measure: 0,
  },
  { node: 1, path: [[1, '2025']], total: true, measure: 0 },
  {
    node: 5,
    path: [
      [2, '2026'],
      [5, 'T1'],
    ],
    total: false,
    measure: 0,
  },
  { node: 0, path: [], total: true, measure: 0 },
]

describe('headerRowCount', () => {
  it('counts a row per level, plus that of the measures', () => {
    expect(headerRowCount(2, 1)).toBe(2)
    expect(headerRowCount(2, 2)).toBe(3)
    expect(headerRowCount(0, 1)).toBe(1)
    expect(headerRowCount(0, 3)).toBe(1)
    expect(headerRowCount(0, 0)).toBe(1)
  })
})

describe('headerCells', () => {
  it('shares the cell of a node, and closes a group by its total', () => {
    expect(headerCells(QUARTERS, 2, ['SUM(montant)'])).toMatchObject([
      { row: 0, rowSpan: 1, column: 0, span: 3, text: '2025', kind: 'value' },
      { row: 0, rowSpan: 1, column: 3, span: 1, text: '2026', kind: 'value' },
      { row: 0, rowSpan: 2, column: 4, span: 1, text: 'Total', kind: 'total' },
      { row: 1, rowSpan: 1, column: 0, span: 1, text: 'T1', kind: 'value' },
      { row: 1, rowSpan: 1, column: 1, span: 1, text: 'T2', kind: 'value' },
      { row: 1, rowSpan: 1, column: 2, span: 1, text: 'Total', kind: 'total' },
      { row: 1, rowSpan: 1, column: 3, span: 1, text: 'T1', kind: 'value' },
    ])
  })

  it('names the values of each single column, a group heading none', () => {
    const targets = headerCells(QUARTERS, 2, ['SUM(montant)']).map(({ text, target }) => [
      text,
      target,
    ])
    expect(targets).toEqual([
      ['2025', null],
      ['2026', null],
      ['Total', { path: [], measure: 0 }],
      ['T1', { path: ['2025', 'T1'], measure: 0 }],
      ['T2', { path: ['2025', 'T2'], measure: 0 }],
      ['Total', { path: ['2025'], measure: 0 }],
      ['T1', { path: ['2026', 'T1'], measure: 0 }],
    ])
  })

  it('names nothing without a measure', () => {
    const columns: PivotColumn[] = [
      { node: 1, path: [[1, '2025']], total: false, measure: null },
      { node: 0, path: [], total: true, measure: null },
    ]
    expect(headerCells(columns, 1, []).map(({ target }) => target)).toEqual([null, null])
  })

  it('heads each measure under its column, a total shared by its measures', () => {
    const columns: PivotColumn[] = [
      { node: 1, path: [[1, null]], total: false, measure: 0 },
      { node: 1, path: [[1, null]], total: false, measure: 1 },
      { node: 0, path: [], total: true, measure: 0 },
      { node: 0, path: [], total: true, measure: 1 },
    ]
    const target = (path: Cell[], measure: number) => ({ path, measure })
    expect(headerCells(columns, 1, ['SUM(montant)', 'COUNT(*)'])).toEqual([
      { row: 0, rowSpan: 1, column: 0, span: 2, text: null, kind: 'value', target: null },
      { row: 0, rowSpan: 1, column: 2, span: 2, text: 'Total', kind: 'total', target: null },
      {
        row: 1,
        rowSpan: 1,
        column: 0,
        span: 1,
        text: 'SUM(montant)',
        kind: 'measure',
        target: target([null], 0),
      },
      {
        row: 1,
        rowSpan: 1,
        column: 1,
        span: 1,
        text: 'COUNT(*)',
        kind: 'measure',
        target: target([null], 1),
      },
      {
        row: 1,
        rowSpan: 1,
        column: 2,
        span: 1,
        text: 'SUM(montant)',
        kind: 'measure',
        target: target([], 0),
      },
      {
        row: 1,
        rowSpan: 1,
        column: 3,
        span: 1,
        text: 'COUNT(*)',
        kind: 'measure',
        target: target([], 1),
      },
    ])
  })

  it('names the single measure of a table without column levels', () => {
    const columns: PivotColumn[] = [{ node: 0, path: [], total: false, measure: 0 }]
    expect(headerCells(columns, 0, ['COUNT(*)'])).toEqual([
      {
        row: 0,
        rowSpan: 1,
        column: 0,
        span: 1,
        text: 'COUNT(*)',
        kind: 'measure',
        target: { path: [], measure: 0 },
      },
    ])
  })

  it('starts a window in the middle of a group', () => {
    expect(headerCells(QUARTERS.slice(1, 3), 2, ['SUM(montant)'])).toMatchObject([
      { row: 0, rowSpan: 1, column: 0, span: 2, text: '2025', kind: 'value' },
      { row: 1, rowSpan: 1, column: 0, span: 1, text: 'T2', kind: 'value' },
      { row: 1, rowSpan: 1, column: 1, span: 1, text: 'Total', kind: 'total' },
    ])
  })
})

describe('cornerRows', () => {
  it('names each level of the columns on its row, and those of the lines on the last row', () => {
    expect(cornerRows(3, ['region', 'ville'], ['annee', 'trimestre'])).toEqual([
      { rows: [], column: { level: 0, name: 'annee' } },
      { rows: [], column: { level: 1, name: 'trimestre' } },
      {
        rows: [
          { level: 0, name: 'region' },
          { level: 1, name: 'ville' },
        ],
        column: null,
      },
    ])
    expect(cornerRows(1, ['Pays'], ['Statut'])).toEqual([
      { rows: [{ level: 0, name: 'Pays' }], column: { level: 0, name: 'Statut' } },
    ])
    expect(cornerRows(1, [], [])).toEqual([{ rows: [], column: null }])
  })
})

describe('sortMark', () => {
  it('reads the direction, and a mark apart when the button does not drive the order', () => {
    expect(sortMark(false)).toBe('▲')
    expect(sortMark(true)).toBe('▼')
    expect(sortMark(null)).toBe('⇅')
  })
})

describe('sameLayout', () => {
  it('compares the room of two tables, whatever their groups and their rows', () => {
    const shape = { lines: 3, columns: 2, rowLevels: 1, columnLevels: 1, groups: 4, rows: 9 }
    expect(sameLayout(shape, { ...shape, groups: 1, rows: 1 })).toBe(true)
    expect(sameLayout(shape, { ...shape, lines: 4 })).toBe(false)
    expect(sameLayout(shape, { ...shape, columns: 3 })).toBe(false)
    expect(sameLayout(shape, { ...shape, rowLevels: 2 })).toBe(false)
    expect(sameLayout(shape, { ...shape, columnLevels: 0 })).toBe(false)
  })
})

describe('lineLabel', () => {
  it('names the grand total, and keeps NULL', () => {
    expect(lineLabel({ depth: 0, label: null, path: [] })).toBe('Total')
    expect(lineLabel({ depth: 1, label: null, path: [null] })).toBeNull()
    expect(lineLabel({ depth: 2, label: 'Lille', path: ['Nord', 'Lille'] })).toBe('Lille')
  })
})
