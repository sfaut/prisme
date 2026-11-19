import { describe, expect, it } from 'vitest'

import { type EditorLayout, defaultEditorLayout, fillingPanel, isFolded } from '../editorLayout'

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
  it('starts with the editor alone, the results and the Pivoteur closed', () => {
    const layout = defaultEditorLayout()
    expect(isFolded(layout, 'editor')).toBe(false)
    expect(isFolded(layout, 'results')).toBe(true)
    expect(isFolded(layout, 'pivot')).toBe(true)
    expect(layout.results).toBeNull()
    expect(layout.pivot).toBeNull()
    expect(fillingPanel(layout)).toBe('editor')
  })

  it('fills with the highest open panel, none when all are closed', () => {
    expect(fillingPanel(layoutWith({ resultsFolded: false }))).toBe('editor')
    expect(fillingPanel(layoutWith({ editorFolded: true, resultsFolded: false }))).toBe('results')
    expect(fillingPanel(layoutWith({ editorFolded: true, pivotFolded: false }))).toBe('pivot')
    expect(fillingPanel(layoutWith({ editorFolded: true }))).toBeNull()
  })
})
