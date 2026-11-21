import { describe, expect, it } from 'vitest'

import { nextShownTab } from '../shownTab'

describe('nextShownTab', () => {
  it('shows the first tab of a new batch', () => {
    expect(nextShownTab([4, 5], [1, 2], 2)).toBe(4)
    expect(nextShownTab([1, 2, 6], [1, 2], 1)).toBe(6)
    expect(nextShownTab([3, 4], [], 0)).toBe(3)
  })

  it('keeps the tab saved with the editor when its tabs are read', () => {
    expect(nextShownTab([3, 4, 5], [], 4)).toBe(4)
  })

  it('shows the first tab when the saved one is no longer there', () => {
    expect(nextShownTab([3, 4], [], 9)).toBe(3)
  })

  it('keeps the tab shown, or the first one when it disappears', () => {
    expect(nextShownTab([1, 2], [1, 2, 3], 2)).toBe(2)
    expect(nextShownTab([1, 2], [1, 2, 3], 3)).toBe(1)
    expect(nextShownTab([], [1], 1)).toBe(0)
  })
})
