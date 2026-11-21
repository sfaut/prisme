import { describe, expect, it } from 'vitest'

import { compactCount } from '../compactCount'

/**
 * Non-breaking space put by `Intl` before the unit.
 */
const NBSP = ' '

describe('compactCount', () => {
  it('keeps a count below 1,000 exact', () => {
    expect(compactCount(0)).toBe('0')
    expect(compactCount(345)).toBe('345')
    expect(compactCount(999)).toBe('999')
  })

  it('shortens a larger count to one decimal at most', () => {
    expect(compactCount(1000)).toBe(`1${NBSP}k`)
    expect(compactCount(1300)).toBe(`1,3${NBSP}k`)
    expect(compactCount(4_500_000)).toBe(`4,5${NBSP}M`)
    expect(compactCount(1_234_567_890)).toBe(`1,2${NBSP}Md`)
  })
})
