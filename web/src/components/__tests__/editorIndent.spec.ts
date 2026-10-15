import {
  EditorSelection,
  EditorState,
  type StateCommand,
  type Transaction,
} from '@codemirror/state'
import { describe, expect, it } from 'vitest'

import { dedentLines, indentOrInsertSpaces, reindented, spacesToNextStop } from '../editorIndent'

/**
 * Applique une commande à un texte et rend le texte et la sélection obtenus.
 *
 * @param command Commande à appliquer.
 * @param doc Texte de départ.
 * @param ranges Sélection de départ : `[ancre, tête]` par plage.
 */
function apply(command: StateCommand, doc: string, ranges: [number, number][]) {
  let state = EditorState.create({
    doc,
    selection: EditorSelection.create(
      ranges.map(([anchor, head]) => EditorSelection.range(anchor, head)),
    ),
    extensions: EditorState.allowMultipleSelections.of(true),
  })
  const handled = command({
    state,
    dispatch: (transaction: Transaction) => (state = transaction.state),
  })
  return {
    handled,
    text: state.doc.toString(),
    ranges: state.selection.ranges.map((range) => [range.anchor, range.head]),
  }
}

describe('taquets', () => {
  it('compte les espaces jusqu au taquet suivant', () => {
    expect(spacesToNextStop(0)).toBe(4)
    expect(spacesToNextStop(1)).toBe(3)
    expect(spacesToNextStop(4)).toBe(4)
    expect(spacesToNextStop(7)).toBe(1)
  })

  it('réindente en espaces seuls, alignés sur un taquet', () => {
    expect(reindented('', 'more')).toBe('    ')
    expect(reindented('  ', 'more')).toBe('    ')
    expect(reindented('    ', 'more')).toBe('        ')
    expect(reindented('    ', 'less')).toBe('')
    expect(reindented('      ', 'less')).toBe('    ')
    expect(reindented('', 'less')).toBe('')
    // Une tabulation vaut un niveau, et devient des espaces.
    expect(reindented('\t', 'more')).toBe('        ')
    expect(reindented('\t\t', 'less')).toBe('    ')
    expect(reindented('  \t', 'less')).toBe('')
  })
})

describe('Tab', () => {
  it('insère des espaces jusqu au taquet suivant, au curseur', () => {
    expect(apply(indentOrInsertSpaces, 'SELECT', [[0, 0]])).toMatchObject({
      handled: true,
      text: '    SELECT',
      ranges: [[4, 4]],
    })
    expect(apply(indentOrInsertSpaces, 'SELECT a', [[6, 6]])).toMatchObject({
      text: 'SELECT   a',
      ranges: [[8, 8]],
    })
  })

  it('compte une tabulation qui précède le curseur jusqu à son taquet', () => {
    expect(apply(indentOrInsertSpaces, '\ta', [[2, 2]]).text).toBe('\ta   ')
  })

  it('insère à chaque curseur', () => {
    expect(
      apply(indentOrInsertSpaces, 'a\nbc', [
        [1, 1],
        [4, 4],
      ]).text,
    ).toBe('a   \nbc  ')
  })

  it('indente toutes les lignes d une sélection, tabulations de tête remplacées', () => {
    const result = apply(indentOrInsertSpaces, 'SELECT a\n\tFROM t\n  WHERE b', [[0, 26]])
    expect(result.text).toBe('    SELECT a\n        FROM t\n    WHERE b')
    // L'ancre en début de ligne y reste, la tête suit le texte.
    expect(result.ranges).toEqual([[0, result.text.length]])
  })

  it('laisse la ligne où s arrête la sélection, si elle s y arrête au début', () => {
    expect(apply(indentOrInsertSpaces, 'a\nb', [[0, 2]]).text).toBe('    a\nb')
  })
})

describe('Maj-Tab', () => {
  it('désindente la ligne du curseur, sans sélection', () => {
    expect(apply(dedentLines, '      SELECT', [[8, 8]])).toMatchObject({
      handled: true,
      text: '    SELECT',
      ranges: [[6, 6]],
    })
  })

  it('désindente toutes les lignes d une sélection, tabulations de tête remplacées', () => {
    expect(apply(dedentLines, '\t\ta\n    b\nc', [[0, 10]]).text).toBe('    a\nb\nc')
  })

  it('ne change rien à une ligne sans retrait, mais garde la touche', () => {
    expect(apply(dedentLines, 'SELECT', [[3, 3]])).toMatchObject({ handled: true, text: 'SELECT' })
  })
})
