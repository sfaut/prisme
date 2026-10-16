import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import {
  DEFAULT_SIDER_WIDTH,
  MIN_SIDER_WIDTH,
  clampSiderWidth,
  loadSiderWidth,
  saveSiderWidth,
} from '../siderWidth'

// Under Node 26, Node's `localStorage` (inactive without a file) hides that of
// jsdom: an in-memory storage replaces it for the tests.
beforeEach(() => {
  const values = new Map<string, string>()
  vi.stubGlobal('localStorage', {
    getItem: (key: string) => values.get(key) ?? null,
    setItem: (key: string, value: string) => values.set(key, value),
  })
})

afterEach(() => vi.unstubAllGlobals())

describe('width of the left panel', () => {
  it('stays between the minimum and 70 % of the window', () => {
    expect(clampSiderWidth(50, 1200)).toBe(MIN_SIDER_WIDTH)
    expect(clampSiderWidth(450.4, 1200)).toBe(450)
    expect(clampSiderWidth(1100, 1200)).toBe(840)
    // Very narrow window: the minimum wins.
    expect(clampSiderWidth(300, 250)).toBe(MIN_SIDER_WIDTH)
  })

  it('is remembered, with the default width when no value is readable', () => {
    expect(loadSiderWidth()).toBe(DEFAULT_SIDER_WIDTH)
    saveSiderWidth(420)
    expect(loadSiderWidth()).toBe(420)
    window.localStorage.setItem('prisme.siderWidth', 'abc')
    expect(loadSiderWidth()).toBe(DEFAULT_SIDER_WIDTH)
  })
})
