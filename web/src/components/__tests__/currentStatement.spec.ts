import { EditorState } from '@codemirror/state'
import { EditorView } from '@codemirror/view'
import { describe, expect, it } from 'vitest'

import { currentStatement, setStatements, statementAt } from '../currentStatement'

describe('statementAt', () => {
  const statements = [
    { start: 0, end: 9 },
    { start: 11, end: 20 },
  ]

  it('designates the last query that starts before the cursor, otherwise the first one', () => {
    expect(statementAt(statements, 3)).toBe(0)
    // Between two queries: the previous one.
    expect(statementAt(statements, 10)).toBe(0)
    expect(statementAt(statements, 15)).toBe(1)
    expect(statementAt([{ start: 4, end: 9 }], 0)).toBe(0)
    expect(statementAt([], 0)).toBeNull()
  })
})

describe('currentStatement', () => {
  /**
   * Highlighted text in the editor.
   *
   * @param view Editor.
   */
  function highlighted(view: EditorView): string[] {
    return [...view.dom.querySelectorAll('.cm-current-statement')].map(
      (node) => node.textContent ?? '',
    )
  }

  it('highlights the query under the cursor, and follows it', () => {
    const doc = 'SELECT 1;\nSELECT 2;'
    const view = new EditorView({
      state: EditorState.create({ doc, extensions: currentStatement() }),
      parent: document.body,
    })
    view.dispatch({
      effects: setStatements.of([
        { start: 0, end: 9 },
        { start: 10, end: 19 },
      ]),
    })
    expect(highlighted(view)).toEqual(['SELECT 1;'])
    view.dispatch({ selection: { anchor: 15 } })
    expect(highlighted(view)).toEqual(['SELECT 2;'])
    // Text inserted before: the spans follow while waiting for the next analysis.
    view.dispatch({ changes: { from: 0, insert: '-- x\n' } })
    expect(highlighted(view)).toEqual(['SELECT 2;'])
    view.destroy()
  })

  it('highlights nothing with a single query', () => {
    const view = new EditorView({
      state: EditorState.create({ doc: 'SELECT 1', extensions: currentStatement() }),
      parent: document.body,
    })
    view.dispatch({ effects: setStatements.of([{ start: 0, end: 8 }]) })
    expect(highlighted(view)).toEqual([])
    view.destroy()
  })
})
