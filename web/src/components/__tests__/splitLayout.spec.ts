import { describe, expect, it } from 'vitest'

import { type DragPanel, type DragStart, dragSizes } from '../split/splitLayout'

/**
 * A panel of 300 pixels, at least 201, folded in 24.
 */
const panel = (size: number, min = 201, folded: number | null = 24): DragPanel => ({
  size,
  min,
  folded,
  isFolded: false,
})

const start = (overrides: Partial<DragStart> = {}): DragStart => ({
  target: panel(300),
  fillRoom: 100,
  ...overrides,
})

describe('dragSizes', () => {
  it('follows the move exactly, without rounding', () => {
    expect(dragSizes(start(), 31)).toEqual({ size: 331, folded: false })
    // A move of a pixel of a screen at 125 %, 0.8 pixel of CSS.
    expect(dragSizes(start(), 0.8).size).toBeCloseTo(300.8)
  })

  it('stops when the panel that fills is at its smallest size', () => {
    expect(dragSizes(start(), 200).size).toBe(400)
  })

  it('stops at the smallest size, then folds past half the way', () => {
    // Half the way between 201 and 24: 112.5.
    expect(dragSizes(start(), -150)).toEqual({ size: 201, folded: false })
    expect(dragSizes(start(), -200)).toEqual({ size: null, folded: true })
  })

  it('unfolds past half the way, at its smallest size', () => {
    const folded = start({ target: { ...panel(24), isFolded: true }, fillRoom: 300 })
    expect(dragSizes(folded, 80)).toEqual({ size: null, folded: true })
    expect(dragSizes(folded, 100)).toEqual({ size: 201, folded: false })
  })

  it('stays folded without room for its smallest size', () => {
    const folded = start({ target: { ...panel(24), isFolded: true }, fillRoom: 50 })
    expect(dragSizes(folded, 200)).toEqual({ size: null, folded: true })
  })
})
