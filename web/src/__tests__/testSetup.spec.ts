import { EditorView } from '@codemirror/view'
import { describe, expect, it } from 'vitest'

describe('test environment', () => {
  it('lets CodeMirror measure the text under jsdom', () => {
    const view = new EditorView({ doc: 'SELECT 1', parent: document.body })
    expect(() => view.coordsAtPos(3)).not.toThrow()
    view.destroy()
  })
})
