import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import {
  DEFAULT_EDITOR_SPLIT,
  MAX_EDITOR_SPLIT,
  MIN_EDITOR_SPLIT,
  clampEditorSplit,
  loadEditorSplit,
  saveEditorSplit,
} from '../editorSplit'

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

describe('split of the height between the editor and the results', () => {
  it('stays between the minimum and the maximum', () => {
    expect(clampEditorSplit(0.02)).toBe(MIN_EDITOR_SPLIT)
    expect(clampEditorSplit(0.5)).toBe(0.5)
    expect(clampEditorSplit(0.95)).toBe(MAX_EDITOR_SPLIT)
  })

  it('is remembered, with the top third when no value is readable', () => {
    expect(loadEditorSplit()).toBe(DEFAULT_EDITOR_SPLIT)
    saveEditorSplit(0.45)
    expect(loadEditorSplit()).toBe(0.45)
    window.localStorage.setItem('prisme.editorSplit', 'abc')
    expect(loadEditorSplit()).toBe(DEFAULT_EDITOR_SPLIT)
  })
})
