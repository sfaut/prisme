import { describe, expect, it } from 'vitest'

import type { PivotColumn } from '../pivotEngine'
import { headerCells, headerRowCount, lineLabel } from '../pivotLayout'

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
    expect(headerCells(QUARTERS, 2, ['SUM(montant)'])).toEqual([
      { row: 0, rowSpan: 1, column: 0, span: 3, text: '2025', kind: 'value' },
      { row: 0, rowSpan: 1, column: 3, span: 1, text: '2026', kind: 'value' },
      { row: 0, rowSpan: 2, column: 4, span: 1, text: 'Total', kind: 'total' },
      { row: 1, rowSpan: 1, column: 0, span: 1, text: 'T1', kind: 'value' },
      { row: 1, rowSpan: 1, column: 1, span: 1, text: 'T2', kind: 'value' },
      { row: 1, rowSpan: 1, column: 2, span: 1, text: 'Total', kind: 'total' },
      { row: 1, rowSpan: 1, column: 3, span: 1, text: 'T1', kind: 'value' },
    ])
  })

  it('heads each measure under its column, a total shared by its measures', () => {
    const columns: PivotColumn[] = [
      { node: 1, path: [[1, null]], total: false, measure: 0 },
      { node: 1, path: [[1, null]], total: false, measure: 1 },
      { node: 0, path: [], total: true, measure: 0 },
      { node: 0, path: [], total: true, measure: 1 },
    ]
    expect(headerCells(columns, 1, ['SUM(montant)', 'COUNT(*)'])).toEqual([
      { row: 0, rowSpan: 1, column: 0, span: 2, text: null, kind: 'value' },
      { row: 0, rowSpan: 1, column: 2, span: 2, text: 'Total', kind: 'total' },
      { row: 1, rowSpan: 1, column: 0, span: 1, text: 'SUM(montant)', kind: 'measure' },
      { row: 1, rowSpan: 1, column: 1, span: 1, text: 'COUNT(*)', kind: 'measure' },
      { row: 1, rowSpan: 1, column: 2, span: 1, text: 'SUM(montant)', kind: 'measure' },
      { row: 1, rowSpan: 1, column: 3, span: 1, text: 'COUNT(*)', kind: 'measure' },
    ])
  })

  it('names the single measure of a table without column levels', () => {
    const columns: PivotColumn[] = [{ node: 0, path: [], total: false, measure: 0 }]
    expect(headerCells(columns, 0, ['COUNT(*)'])).toEqual([
      { row: 0, rowSpan: 1, column: 0, span: 1, text: 'COUNT(*)', kind: 'measure' },
    ])
  })

  it('starts a window in the middle of a group', () => {
    expect(headerCells(QUARTERS.slice(1, 3), 2, ['SUM(montant)'])).toEqual([
      { row: 0, rowSpan: 1, column: 0, span: 2, text: '2025', kind: 'value' },
      { row: 1, rowSpan: 1, column: 0, span: 1, text: 'T2', kind: 'value' },
      { row: 1, rowSpan: 1, column: 1, span: 1, text: 'Total', kind: 'total' },
    ])
  })
})

describe('lineLabel', () => {
  it('names the grand total, and keeps NULL', () => {
    expect(lineLabel({ depth: 0, label: null })).toBe('Total')
    expect(lineLabel({ depth: 1, label: null })).toBeNull()
    expect(lineLabel({ depth: 2, label: 'Lille' })).toBe('Lille')
  })
})
