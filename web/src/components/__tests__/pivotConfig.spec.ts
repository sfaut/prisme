import { describe, expect, it } from 'vitest'

import {
  columnAt,
  defaultMeasure,
  emptyConfig,
  isEmptyConfig,
  measureLabel,
  placeColumn,
  removeElement,
  withAggregate,
} from '../pivotConfig'
import type { PivotRequest } from '../pivotEngine'

/**
 * Columns of the example of the specification.
 */
const NAMES = ['region', 'ville', 'annee', 'trimestre', 'montant']

describe('defaultMeasure', () => {
  it('sums a number and counts the other columns', () => {
    expect(defaultMeasure(4, 'decimal')).toEqual({ aggregate: 'sum', column: 4 })
    expect(defaultMeasure(2, 'integer')).toEqual({ aggregate: 'sum', column: 2 })
    expect(defaultMeasure(2, 'float')).toEqual({ aggregate: 'sum', column: 2 })
    expect(defaultMeasure(0, 'text')).toEqual({ aggregate: 'count', column: 0 })
    expect(defaultMeasure(0, undefined)).toEqual({ aggregate: 'count', column: 0 })
  })
})

describe('measureLabel', () => {
  it('writes the expression of the measure', () => {
    expect(measureLabel({ aggregate: 'sum', column: 4 }, NAMES)).toBe('SUM(montant)')
    expect(measureLabel({ aggregate: 'count', column: 1 }, NAMES)).toBe('COUNT(ville)')
    expect(measureLabel({ aggregate: 'count_star', column: 1 }, NAMES)).toBe('COUNT(*)')
  })
})

describe('placeColumn', () => {
  it('adds a level or a measure at a position', () => {
    let config = placeColumn(emptyConfig(), 0, 'text', { zone: 'rows', index: 0 }, null)
    config = placeColumn(config, 1, 'text', { zone: 'rows', index: 0 }, null)
    config = placeColumn(config, 4, 'decimal', { zone: 'measures', index: 9 }, null)
    expect(config).toEqual({
      rows: [1, 0],
      columns: [],
      measures: [{ aggregate: 'sum', column: 4 }],
    })
    expect(isEmptyConfig(config)).toBe(false)
    expect(isEmptyConfig(emptyConfig())).toBe(true)
  })

  it('moves a level already placed instead of placing it twice', () => {
    const config: PivotRequest = { rows: [0, 1], columns: [2], measures: [] }
    expect(placeColumn(config, 0, 'text', { zone: 'columns', index: 0 }, null)).toEqual({
      rows: [1],
      columns: [0, 2],
      measures: [],
    })
    expect(placeColumn(config, 0, 'text', { zone: 'rows', index: 2 }, null).rows).toEqual([1, 0])
  })

  it('moves an element within its zone, the target counted before the move', () => {
    const config: PivotRequest = { rows: [0, 1, 2], columns: [], measures: [] }
    const from = { zone: 'rows' as const, index: 0 }
    expect(placeColumn(config, 0, 'text', { zone: 'rows', index: 2 }, from).rows).toEqual([1, 0, 2])
    expect(placeColumn(config, 0, 'text', { zone: 'rows', index: 3 }, from).rows).toEqual([1, 2, 0])
    expect(
      placeColumn(config, 2, 'text', { zone: 'rows', index: 0 }, { zone: 'rows', index: 2 }).rows,
    ).toEqual([2, 0, 1])
  })

  it('keeps a moved measure, and gives its default measure to a level that becomes one', () => {
    const config: PivotRequest = {
      rows: [0],
      columns: [],
      measures: [
        { aggregate: 'count_star', column: 1 },
        { aggregate: 'sum', column: 4 },
      ],
    }
    const moved = placeColumn(
      config,
      1,
      'text',
      { zone: 'measures', index: 2 },
      { zone: 'measures', index: 0 },
    )
    expect(moved.measures).toEqual([
      { aggregate: 'sum', column: 4 },
      { aggregate: 'count_star', column: 1 },
    ])
    const level = placeColumn(
      config,
      0,
      'text',
      { zone: 'measures', index: 0 },
      { zone: 'rows', index: 0 },
    )
    expect(level).toEqual({
      rows: [],
      columns: [],
      measures: [{ aggregate: 'count', column: 0 }, ...config.measures],
    })
    const measure = placeColumn(
      config,
      4,
      'decimal',
      { zone: 'columns', index: 0 },
      { zone: 'measures', index: 1 },
    )
    expect(measure).toEqual({
      rows: [0],
      columns: [4],
      measures: [{ aggregate: 'count_star', column: 1 }],
    })
  })
})

describe('removeElement', () => {
  it('removes one element of one zone', () => {
    const config: PivotRequest = {
      rows: [0, 1],
      columns: [2],
      measures: [{ aggregate: 'sum', column: 4 }],
    }
    expect(removeElement(config, { zone: 'rows', index: 0 })).toEqual({
      rows: [1],
      columns: [2],
      measures: config.measures,
    })
    expect(removeElement(config, { zone: 'measures', index: 0 }).measures).toEqual([])
    expect(columnAt(config, { zone: 'columns', index: 0 })).toBe(2)
    expect(columnAt(config, { zone: 'measures', index: 0 })).toBe(4)
  })
})

describe('withAggregate', () => {
  it('changes the aggregate of one measure', () => {
    const config: PivotRequest = {
      rows: [],
      columns: [],
      measures: [
        { aggregate: 'sum', column: 4 },
        { aggregate: 'sum', column: 2 },
      ],
    }
    expect(withAggregate(config, 1, 'count').measures).toEqual([
      { aggregate: 'sum', column: 4 },
      { aggregate: 'count', column: 2 },
    ])
  })
})
