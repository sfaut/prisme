import { EditorView } from '@codemirror/view'
import { codeFolding, foldEffect } from '@codemirror/language'
import { EditorSelection } from '@codemirror/state'
import { afterEach, describe, expect, it } from 'vitest'

import { positionOf, restoringEffects, startPosition, withinText } from '../editorPosition'

let view: EditorView | null = null

afterEach(() => {
  view?.destroy()
  view = null
})

describe('withinText', () => {
  it('keeps a position within the text as it is', () => {
    const position = { anchor: 2, head: 5, top: 0, folds: [[1, 4]] as [number, number][] }
    expect(withinText(position, 10)).toEqual(position)
  })

  it('brings back to the end of a shorter text, and drops the folds beyond it', () => {
    expect(
      withinText(
        {
          anchor: 12,
          head: 30,
          top: 20,
          folds: [
            [1, 4],
            [6, 15],
            [5, 5],
          ],
        },
        10,
      ),
    ).toEqual({ anchor: 10, head: 10, top: 10, folds: [[1, 4]] })
  })
})

describe('positionOf', () => {
  it('reads the selection and the folds of an editor', () => {
    view = new EditorView({
      doc: 'SELECT 1;\nSELECT 2;\nSELECT 3;',
      selection: EditorSelection.single(3, 8),
      extensions: [codeFolding()],
      parent: document.body,
    })
    view.dispatch({ effects: foldEffect.of({ from: 12, to: 18 }) })
    expect(positionOf(view)).toEqual({ anchor: 3, head: 8, top: 0, folds: [[12, 18]] })
  })

  it('reads the start of a new editor', () => {
    view = new EditorView({ doc: '', parent: document.body })
    expect(positionOf(view)).toEqual(startPosition())
  })

  it('folds again the saved passages', () => {
    view = new EditorView({
      doc: 'SELECT 1;\nSELECT 2;',
      extensions: [codeFolding()],
      parent: document.body,
    })
    view.dispatch({ effects: restoringEffects({ anchor: 0, head: 0, top: 0, folds: [[2, 6]] }) })
    expect(positionOf(view).folds).toEqual([[2, 6]])
  })
})
