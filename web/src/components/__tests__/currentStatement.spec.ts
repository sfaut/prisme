import { EditorState } from '@codemirror/state'
import { EditorView } from '@codemirror/view'
import { describe, expect, it } from 'vitest'

import { currentStatement, setStatements, statementAt } from '../currentStatement'

describe('statementAt', () => {
  const statements = [
    { start: 0, end: 9 },
    { start: 11, end: 20 },
  ]

  it('désigne la dernière requête qui commence avant le curseur, sinon la première', () => {
    expect(statementAt(statements, 3)).toBe(0)
    // Entre deux requêtes : la précédente.
    expect(statementAt(statements, 10)).toBe(0)
    expect(statementAt(statements, 15)).toBe(1)
    expect(statementAt([{ start: 4, end: 9 }], 0)).toBe(0)
    expect(statementAt([], 0)).toBeNull()
  })
})

describe('currentStatement', () => {
  /**
   * Texte surligné dans l'éditeur.
   *
   * @param view Éditeur.
   */
  function highlighted(view: EditorView): string[] {
    return [...view.dom.querySelectorAll('.cm-current-statement')].map(
      (node) => node.textContent ?? '',
    )
  }

  it('surligne la requête sous le curseur, et la suit', () => {
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
    // Texte inséré avant : les emplacements suivent en attendant la prochaine analyse.
    view.dispatch({ changes: { from: 0, insert: '-- x\n' } })
    expect(highlighted(view)).toEqual(['SELECT 2;'])
    view.destroy()
  })

  it('ne surligne rien avec une seule requête', () => {
    const view = new EditorView({
      state: EditorState.create({ doc: 'SELECT 1', extensions: currentStatement() }),
      parent: document.body,
    })
    view.dispatch({ effects: setStatements.of([{ start: 0, end: 8 }]) })
    expect(highlighted(view)).toEqual([])
    view.destroy()
  })
})
