import { describe, expect, it } from 'vitest'

import {
  columnAt,
  columnGeometry,
  columnOffset,
  columnRange,
  defaultSteps,
  draggedSteps,
  fittedSteps,
  stepsBefore,
  stepsLength,
  targetKey,
  widenedColumns,
  windowTracks,
} from '../pivotWidths'
import { drawnRange } from '../resultGrid'

/**
 * Ten columns of 10 steps of 3 pixels, 30 pixels: the column 2 is given 20 steps, the column 5
 * is given 4. They start at 0, 30, 60, 120, 150, 180, 192, 222, 252 and 282, and end at 312.
 */
const DEFAULTS = Array.from({ length: 10 }, () => 10)
const GEOMETRY = columnGeometry(
  DEFAULTS,
  [
    { index: 2, width: 20 },
    { index: 5, width: 4 },
    { index: 12, width: 50 },
  ],
  3,
)

describe('widenedColumns', () => {
  it('places the widths whose columns the table shows, in the order of the table', () => {
    const positions = new Map([
      [targetKey({ path: ['b'], measure: 0 }), 4],
      [targetKey({ path: ['a', null], measure: 1 }), 1],
    ])
    const widths = [
      { path: ['b'], measure: 0, width: 20 },
      { path: ['c'], measure: 0, width: 30 },
      { path: ['a', null], measure: 1, width: 10 },
    ]
    expect(widenedColumns(widths, positions)).toEqual([
      { index: 1, width: 10 },
      { index: 4, width: 20 },
    ])
  })

  it('names a column by its values, NULL apart from the text, and by its measure', () => {
    expect(targetKey({ path: [null], measure: 0 })).not.toBe(
      targetKey({ path: ['NULL'], measure: 0 }),
    )
    expect(targetKey({ path: ['a'], measure: 0 })).not.toBe(targetKey({ path: ['a'], measure: 1 }))
    expect(targetKey({ path: ['a', 'b'], measure: 0 })).not.toBe(
      targetKey({ path: ['a,b'], measure: 0 }),
    )
  })
})

describe('the geometry of the columns', () => {
  it('counts the steps before a column, those given in place of the default ones', () => {
    expect(GEOMETRY.widths).toEqual([10, 10, 20, 10, 10, 4, 10, 10, 10, 10])
    expect(stepsBefore(GEOMETRY, 0)).toBe(0)
    expect(stepsBefore(GEOMETRY, 2)).toBe(20)
    expect(stepsBefore(GEOMETRY, 3)).toBe(40)
    expect(stepsBefore(GEOMETRY, 10)).toBe(104)
    expect(stepsBefore(GEOMETRY, 15)).toBe(104)
    expect(columnOffset(GEOMETRY, 6)).toBe(192)
  })

  it('finds the column at a distance, within the table', () => {
    const distances = [-31, 0, 29, 30, 60, 119, 120, 180, 191, 192, 311, 312, 350]
    expect(distances.map((distance) => columnAt(GEOMETRY, distance))).toEqual([
      0, 0, 0, 1, 2, 2, 3, 5, 5, 6, 9, 10, 10,
    ])
    expect(columnAt(columnGeometry([], [], 3), 50)).toBe(0)
  })

  it('gives the columns to draw, as drawnRange does for equal widths', () => {
    const equal = columnGeometry(DEFAULTS, [], 3)
    const areas = [
      [0, 0],
      [0, 100],
      [45, 90],
      [60, 60],
      [290, 100],
    ] as const
    for (const [scroll, viewport] of areas) {
      expect(columnRange(equal, scroll, viewport, 2)).toEqual(
        drawnRange(scroll, viewport, 30, 10, 2),
      )
    }
    // Nothing to see: only the columns beyond the edge.
    expect(columnRange(equal, 0, -50, 2)).toEqual({ start: 0, end: 2 })
    // From 125 to 185: the columns 3 to 5.
    expect(columnRange(GEOMETRY, 125, 60, 0)).toEqual({ start: 3, end: 6 })
    expect(columnRange(GEOMETRY, 125, 60, 1)).toEqual({ start: 2, end: 7 })
    expect(columnRange(GEOMETRY, 300, 100, 1)).toEqual({ start: 8, end: 10 })
  })

  it('gives the tracks of a window, the width of each column', () => {
    expect(windowTracks(GEOMETRY, 1, 5)).toEqual([
      'calc(var(--spacing) * 10)',
      'calc(var(--spacing) * 20)',
      'calc(var(--spacing) * 10)',
      'calc(var(--spacing) * 10)',
      'calc(var(--spacing) * 4)',
    ])
    expect(windowTracks(GEOMETRY, 7, 0)).toEqual([])
    expect(stepsLength(13)).toBe('calc(var(--spacing) * 13)')
  })
})

describe('defaultSteps', () => {
  // A character of 6 pixels, 15 pixels beside the text, steps of 3 pixels.
  const ROOM = { character: 6, padding: 15 }
  const BOUNDS = { step: 3, min: 13, max: 120 }

  it('fits the longest text of a column, its values or its header with its sort button', () => {
    const lengths = { values: [8, 2, 30, 0], labels: [4, 10, 5, 0] }
    // 8 characters: (48 + 15) / 3 = 21 steps; header of 10 + 2: (72 + 15) / 3 = 29 steps.
    expect(defaultSteps(4, lengths, null, ROOM, BOUNDS)).toEqual([21, 29, 65, 13])
  })

  it('reads the header of a column in the label of its measure, when they head the columns', () => {
    const lengths = { values: [1, 1, 1, 1], labels: [4, 4, 5, 5] }
    // SUM(montant) and COUNT(*), 12 and 8 characters, plus 2.
    expect(defaultSteps(4, lengths, ['SUM(montant)', 'COUNT(*)'], ROOM, BOUNDS)).toEqual([
      33, 25, 33, 25,
    ])
  })

  it('keeps the width within the bounds, and fills a missing length with nothing', () => {
    const lengths = { values: [1000], labels: [] }
    expect(defaultSteps(2, lengths, null, ROOM, BOUNDS)).toEqual([120, 13])
  })
})

describe('the widths given', () => {
  const BOUNDS = { step: 3, min: 13, max: 240 }

  it('rounds a drag to the nearest step, not below the smallest width', () => {
    expect(draggedSteps(108, 10, BOUNDS)).toBe(39)
    expect(draggedSteps(108, 11, BOUNDS)).toBe(40)
    expect(draggedSteps(108, -100, BOUNDS)).toBe(13)
    expect(draggedSteps(108, 2000, BOUNDS)).toBe(703)
  })

  it('fits the widest text, within the bounds', () => {
    expect(fittedSteps([100, 130.5, 90], BOUNDS)).toBe(44)
    expect(fittedSteps([], BOUNDS)).toBe(13)
    expect(fittedSteps([5000], BOUNDS)).toBe(240)
  })
})
