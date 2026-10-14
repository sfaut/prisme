import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import {
  DEFAULT_EDITOR_SPLIT,
  MAX_EDITOR_SPLIT,
  MIN_EDITOR_SPLIT,
  clampEditorSplit,
  loadEditorSplit,
  saveEditorSplit,
} from '../editorSplit'

// Sous Node 26, le `localStorage` de Node (inactif sans fichier) masque celui
// de jsdom : un stockage en mémoire le remplace le temps des tests.
beforeEach(() => {
  const values = new Map<string, string>()
  vi.stubGlobal('localStorage', {
    getItem: (key: string) => values.get(key) ?? null,
    setItem: (key: string, value: string) => values.set(key, value),
  })
})

afterEach(() => vi.unstubAllGlobals())

describe("partage de la hauteur entre l'éditeur et les résultats", () => {
  it('reste entre le minimum et le maximum', () => {
    expect(clampEditorSplit(0.02)).toBe(MIN_EDITOR_SPLIT)
    expect(clampEditorSplit(0.5)).toBe(0.5)
    expect(clampEditorSplit(0.95)).toBe(MAX_EDITOR_SPLIT)
  })

  it('est retenu, avec le tiers du haut en l’absence de valeur lisible', () => {
    expect(loadEditorSplit()).toBe(DEFAULT_EDITOR_SPLIT)
    saveEditorSplit(0.45)
    expect(loadEditorSplit()).toBe(0.45)
    window.localStorage.setItem('prisme.editorSplit', 'abc')
    expect(loadEditorSplit()).toBe(DEFAULT_EDITOR_SPLIT)
  })
})
