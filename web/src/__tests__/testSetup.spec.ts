import { EditorView } from '@codemirror/view'
import { describe, expect, it } from 'vitest'

describe('environnement de test', () => {
  it('laisse CodeMirror mesurer le texte sous jsdom', () => {
    const view = new EditorView({ doc: 'SELECT 1', parent: document.body })
    expect(() => view.coordsAtPos(3)).not.toThrow()
    view.destroy()
  })
})
