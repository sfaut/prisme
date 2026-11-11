import { codeFolding, foldedRanges } from '@codemirror/language'
import { json } from '@codemirror/lang-json'
import { xml } from '@codemirror/lang-xml'
import { EditorState, type Extension } from '@codemirror/state'
import { describe, expect, it } from 'vitest'

import { cursorLevel, foldableNodes, foldingEffects } from '../dataFolding'

const DOCUMENT = [
  '{',
  '    "name": "a",',
  '    "address": {',
  '        "city": {',
  '            "zip": 1',
  '        }',
  '    },',
  '    "tags": [',
  '        1',
  '    ],',
  '    "empty": {}',
  '}',
].join('\n')

/**
 * State of the explorer with a document, the cursor at the start of a line.
 *
 * @param doc Text.
 * @param language Language of the text.
 * @param line Number of the cursor's line, from 1.
 */
function explorer(doc: string, language: Extension, line = 1): EditorState {
  const state = EditorState.create({ doc, extensions: [language, codeFolding()] })
  return state.update({ selection: { anchor: state.doc.line(line).from } }).state
}

/**
 * First lines of the folded ranges, in the order of the text.
 *
 * @param state State of the editor.
 */
function foldedLines(state: EditorState): number[] {
  const lines: number[] = []
  foldedRanges(state).between(0, state.doc.length, (from) => {
    lines.push(state.doc.lineAt(from).number)
  })
  return lines.sort((a, b) => a - b)
}

describe('folding of the data explorer', () => {
  it('finds the multi-line objects and arrays, with their level', () => {
    const state = explorer(DOCUMENT, json())
    expect(
      foldableNodes(state).map((node) => [state.doc.lineAt(node.from).number, node.level]),
    ).toEqual([
      [1, 0],
      [3, 1],
      [4, 2],
      [8, 1],
    ])
  })

  it('takes the level of the node that starts on the cursor line, or holds it', () => {
    const nodes = foldableNodes(explorer(DOCUMENT, json()))
    expect(cursorLevel(explorer(DOCUMENT, json(), 1), nodes)).toBe(0)
    expect(cursorLevel(explorer(DOCUMENT, json(), 2), nodes)).toBe(1)
    expect(cursorLevel(explorer(DOCUMENT, json(), 3), nodes)).toBe(1)
    expect(cursorLevel(explorer(DOCUMENT, json(), 5), nodes)).toBe(3)
    expect(cursorLevel(explorer(DOCUMENT, json(), 7), nodes)).toBe(1)
  })

  it('folds and unfolds a level, then everything', () => {
    let state = explorer(DOCUMENT, json())
    state = state.update({ effects: foldingEffects(state, true, 1) }).state
    expect(foldedLines(state)).toEqual([3, 8])
    // Already folded: nothing more.
    expect(foldingEffects(state, true, 1)).toEqual([])
    state = state.update({ effects: foldingEffects(state, true, null) }).state
    expect(foldedLines(state)).toEqual([1, 3, 4, 8])
    state = state.update({ effects: foldingEffects(state, false, 1) }).state
    expect(foldedLines(state)).toEqual([1, 4])
    state = state.update({ effects: foldingEffects(state, false, null) }).state
    expect(foldedLines(state)).toEqual([])
  })

  it('folds the XML elements', () => {
    const state = explorer('<a>\n    <b>\n        <c/>\n    </b>\n    <d>x</d>\n</a>', xml())
    expect(
      foldableNodes(state).map((node) => [state.doc.lineAt(node.from).number, node.level]),
    ).toEqual([
      [1, 0],
      [2, 1],
    ])
  })
})
