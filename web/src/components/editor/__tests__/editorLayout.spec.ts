import { describe, expect, it } from 'vitest'

import {
  type EditorLayout,
  defaultEditorLayout,
  fillingPanel,
  isFolded,
  lineTarget,
} from '../editorLayout'

/**
 * Default layout, some folds changed.
 *
 * @param folds Folds changed.
 */
function layoutWith(
  folds: Partial<Pick<EditorLayout, 'editorFolded' | 'resultsFolded' | 'pivotFolded'>>,
): EditorLayout {
  return { ...defaultEditorLayout(), ...folds }
}

describe('editorLayout', () => {
  it('starts with the editor alone, the results and the Pivoteur folded under it', () => {
    const layout = defaultEditorLayout()
    expect(isFolded(layout, 'editor')).toBe(false)
    expect(isFolded(layout, 'results')).toBe(true)
    expect(isFolded(layout, 'pivot')).toBe(true)
    expect(layout.results).toBeNull()
    expect(layout.pivot).toBeNull()
    expect(fillingPanel(layout)).toBe('editor')
  })

  it('fills with the highest open panel, none when all are folded', () => {
    expect(fillingPanel(layoutWith({ resultsFolded: false }))).toBe('editor')
    expect(fillingPanel(layoutWith({ editorFolded: true, resultsFolded: false }))).toBe('results')
    expect(fillingPanel(layoutWith({ editorFolded: true, pivotFolded: false }))).toBe('pivot')
    expect(fillingPanel(layoutWith({ editorFolded: true }))).toBeNull()
  })

  it('gives each line the panel under it, below the panel that fills', () => {
    const layout = layoutWith({ resultsFolded: false, pivotFolded: false })
    expect(lineTarget(layout, 'editor', 'results')).toBe('results')
    expect(lineTarget(layout, 'results', 'pivot')).toBe('pivot')
  })

  it('gives a line the folded panel above it, above the panel that fills', () => {
    const results = layoutWith({ editorFolded: true, resultsFolded: false })
    expect(lineTarget(results, 'editor', 'results')).toBe('editor')
    expect(lineTarget(results, 'results', 'pivot')).toBe('pivot')
    const pivot = layoutWith({ editorFolded: true, pivotFolded: false })
    expect(lineTarget(pivot, 'editor', 'results')).toBe('editor')
    expect(lineTarget(pivot, 'results', 'pivot')).toBe('results')
  })

  it('gives each line the panel above it when all are folded', () => {
    const layout = layoutWith({ editorFolded: true })
    expect(lineTarget(layout, 'editor', 'results')).toBe('editor')
    expect(lineTarget(layout, 'results', 'pivot')).toBe('results')
  })
})
