import { describe, expect, it } from 'vitest'

import type { Panels } from '@/api/generated/Panels'
import { defaultEditorDisplay } from '@/stores/editorDisplays'

import { fillingPanel } from '../editorLayout'

/**
 * Panels of a new editor, with some of the view open or closed.
 *
 * @param open Open state of panels of the view.
 */
function panelsWith(open: Partial<Record<'editor' | 'results' | 'pivot', boolean>>): Panels {
  const panels = defaultEditorDisplay().panels
  for (const [panel, state] of Object.entries(open) as [
    'editor' | 'results' | 'pivot',
    boolean,
  ][]) {
    panels[panel].open = state
  }
  return panels
}

describe('fillingPanel', () => {
  it('gives the room to the editor of a new view', () => {
    expect(fillingPanel(panelsWith({}))).toBe('editor')
  })

  it('gives the room to the highest open panel', () => {
    expect(fillingPanel(panelsWith({ results: true }))).toBe('editor')
    expect(fillingPanel(panelsWith({ editor: false, results: true }))).toBe('results')
    expect(fillingPanel(panelsWith({ editor: false, pivot: true }))).toBe('pivot')
    expect(fillingPanel(panelsWith({ editor: false }))).toBeNull()
  })
})
