import { describe, expect, it } from 'vitest'

import {
  type ColumnGeometry,
  columnAt,
  columnOffset,
  columnRange,
  draggedSteps,
  fittedSteps,
  roomBefore,
  roomLength,
  stepsLength,
  targetKey,
  widenedColumns,
  windowTracks,
} from '../pivotWidths'
import { drawnRange } from '../resultGrid'

/**
 * Ten columns of 30 pixels, in steps of 3 pixels: the column 2 takes 20 steps, the column 5 takes
 * 4. They start at 0, 30, 60, 120, 150, 180, 192, 222, 252 and 282, and end at 312.
 */
const GEOMETRY: ColumnGeometry = {
  count: 10,
  standard: 30,
  step: 3,
  widened: [
    { index: 2, width: 20 },
    { index: 5, width: 4 },
  ],
}

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
  it('counts the room before a column, the widened ones by their steps', () => {
    expect(roomBefore(GEOMETRY, 0)).toEqual({ standard: 0, steps: 0 })
    expect(roomBefore(GEOMETRY, 2)).toEqual({ standard: 2, steps: 0 })
    expect(roomBefore(GEOMETRY, 3)).toEqual({ standard: 2, steps: 20 })
    expect(roomBefore(GEOMETRY, 10)).toEqual({ standard: 8, steps: 24 })
    expect(roomLength(roomBefore(GEOMETRY, 10))).toBe(
      'calc(var(--pivot-column-width) * 8 + var(--spacing) * 24)',
    )
    expect(columnOffset(GEOMETRY, 6)).toBe(192)
  })

  it('finds the column at a distance, the default width going on at both ends', () => {
    const distances = [0, 29, 30, 60, 119, 120, 180, 191, 192, 311, 312, 350]
    expect(distances.map((distance) => columnAt(GEOMETRY, distance))).toEqual([
      0, 0, 1, 2, 2, 3, 5, 5, 6, 9, 10, 11,
    ])
    expect(columnAt(GEOMETRY, -31)).toBe(-2)
  })

  it('gives the columns to draw, as drawnRange does for equal widths', () => {
    const equal = { ...GEOMETRY, widened: [] }
    const areas = [
      [0, 0],
      [0, 100],
      [45, 90],
      [60, 60],
      [290, 100],
      [0, -50],
    ] as const
    for (const [scroll, viewport] of areas) {
      expect(columnRange(equal, scroll, viewport, 2)).toEqual(
        drawnRange(scroll, viewport, 30, 10, 2),
      )
    }
    // From 125 to 185: the columns 3 to 5.
    expect(columnRange(GEOMETRY, 125, 60, 0)).toEqual({ start: 3, end: 6 })
    expect(columnRange(GEOMETRY, 125, 60, 1)).toEqual({ start: 2, end: 7 })
    expect(columnRange(GEOMETRY, 300, 100, 1)).toEqual({ start: 8, end: 10 })
  })

  it('gives the tracks of a window, the default widths gathered', () => {
    expect(windowTracks(GEOMETRY.widened, 1, 6)).toEqual([
      'repeat(1, var(--pivot-column-width))',
      'calc(var(--spacing) * 20)',
      'repeat(2, var(--pivot-column-width))',
      'calc(var(--spacing) * 4)',
      'repeat(1, var(--pivot-column-width))',
    ])
    expect(windowTracks(GEOMETRY.widened, 7, 0)).toEqual([])
    expect(stepsLength(13)).toBe('calc(var(--spacing) * 13)')
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
