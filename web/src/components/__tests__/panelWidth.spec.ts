import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { SIDER, clampPanelWidth, loadPanelWidth, savePanelWidth } from '../panelWidth'

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
    expect(clampPanelWidth(SIDER, 50, 1200)).toBe(SIDER.minWidth)
    expect(clampPanelWidth(SIDER, 450.4, 1200)).toBe(450)
    expect(clampPanelWidth(SIDER, 1100, 1200)).toBe(840)
    // Very narrow window: the minimum wins.
    expect(clampPanelWidth(SIDER, 300, 250)).toBe(SIDER.minWidth)
  })

  it('is remembered, with the default width when no value is readable', () => {
    expect(loadPanelWidth(SIDER)).toBe(SIDER.defaultWidth)
    savePanelWidth(SIDER, 420)
    expect(loadPanelWidth(SIDER)).toBe(420)
    window.localStorage.setItem('prisme.siderWidth', 'abc')
    expect(loadPanelWidth(SIDER)).toBe(SIDER.defaultWidth)
  })
})
