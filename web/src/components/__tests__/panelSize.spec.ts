import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { CONSOLE, SIDER, clampPanelSize, loadPanelSize, savePanelSize } from '../panelSize'

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
    expect(clampPanelSize(SIDER, 50, 1200)).toBe(SIDER.minSize)
    expect(clampPanelSize(SIDER, 450.4, 1200)).toBe(450)
    expect(clampPanelSize(SIDER, 1100, 1200)).toBe(840)
    // Very narrow window: the minimum wins.
    expect(clampPanelSize(SIDER, 300, 250)).toBe(SIDER.minSize)
  })

  it('is remembered, with the default width when no value is readable', () => {
    expect(loadPanelSize(SIDER)).toBe(SIDER.defaultSize)
    savePanelSize(SIDER, 420)
    expect(loadPanelSize(SIDER)).toBe(420)
    window.localStorage.setItem('prisme.siderWidth', 'abc')
    expect(loadPanelSize(SIDER)).toBe(SIDER.defaultSize)
  })
})

describe('height of the console', () => {
  it('stays between the minimum and two thirds of the window', () => {
    expect(clampPanelSize(CONSOLE, 10, 900)).toBe(CONSOLE.minSize)
    expect(clampPanelSize(CONSOLE, 800, 900)).toBe(594)
  })
})
