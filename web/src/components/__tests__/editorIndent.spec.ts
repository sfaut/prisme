import {
  EditorSelection,
  EditorState,
  type StateCommand,
  type Transaction,
} from '@codemirror/state'
import { describe, expect, it } from 'vitest'

import {
  dedentLines,
  indentOrInsertSpaces,
  insertPlainNewline,
  reindented,
  spacesToNextStop,
} from '../editorIndent'

/**
 * Applies a command to a text and returns the resulting text and selection.
 *
 * @param command Command to apply.
 * @param doc Initial text.
 * @param ranges Initial selection: `[anchor, head]` per range.
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

describe('tab stops', () => {
  it('counts the spaces up to the next tab stop', () => {
    expect(spacesToNextStop(0)).toBe(4)
    expect(spacesToNextStop(1)).toBe(3)
    expect(spacesToNextStop(4)).toBe(4)
    expect(spacesToNextStop(7)).toBe(1)
  })

  it('reindents in spaces only, aligned on a tab stop', () => {
    expect(reindented('', 'more')).toBe('    ')
    expect(reindented('  ', 'more')).toBe('    ')
    expect(reindented('    ', 'more')).toBe('        ')
    expect(reindented('    ', 'less')).toBe('')
    expect(reindented('      ', 'less')).toBe('    ')
    expect(reindented('', 'less')).toBe('')
    // A tab is worth one level, and becomes spaces.
    expect(reindented('\t', 'more')).toBe('        ')
    expect(reindented('\t\t', 'less')).toBe('    ')
    expect(reindented('  \t', 'less')).toBe('')
  })
})

describe('Tab', () => {
  it('inserts spaces up to the next tab stop, at the cursor', () => {
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

  it('counts a tab that precedes the cursor up to its tab stop', () => {
    expect(apply(indentOrInsertSpaces, '\ta', [[2, 2]]).text).toBe('\ta   ')
  })

  it('inserts at each cursor', () => {
    expect(
      apply(indentOrInsertSpaces, 'a\nbc', [
        [1, 1],
        [4, 4],
      ]).text,
    ).toBe('a   \nbc  ')
  })

  it('indents all the lines of a selection, leading tabs replaced', () => {
    const result = apply(indentOrInsertSpaces, 'SELECT a\n\tFROM t\n  WHERE b', [[0, 26]])
    expect(result.text).toBe('    SELECT a\n        FROM t\n    WHERE b')
    // The anchor at the start of a line stays there, the head follows the text.
    expect(result.ranges).toEqual([[0, result.text.length]])
  })

  it('leaves the line where the selection stops, if it stops at its start', () => {
    expect(apply(indentOrInsertSpaces, 'a\nb', [[0, 2]]).text).toBe('    a\nb')
  })
})

describe('Enter', () => {
  it('goes to a new line in the first column, even after an indented line', () => {
    expect(apply(insertPlainNewline, 'SELECT a', [[8, 8]])).toMatchObject({
      handled: true,
      text: 'SELECT a\n',
      ranges: [[9, 9]],
    })
    expect(apply(insertPlainNewline, '    FROM t', [[10, 10]]).text).toBe('    FROM t\n')
  })

  it('replaces the selection with the line break', () => {
    expect(apply(insertPlainNewline, 'SELECT a, b', [[8, 11]]).text).toBe('SELECT a\n')
  })
})

describe('Shift+Tab', () => {
  it('dedents the cursor line, without a selection', () => {
    expect(apply(dedentLines, '      SELECT', [[8, 8]])).toMatchObject({
      handled: true,
      text: '    SELECT',
      ranges: [[6, 6]],
    })
  })

  it('dedents all the lines of a selection, leading tabs replaced', () => {
    expect(apply(dedentLines, '\t\ta\n    b\nc', [[0, 10]]).text).toBe('    a\nb\nc')
  })

  it('changes nothing on a line without indentation, but keeps the key', () => {
    expect(apply(dedentLines, 'SELECT', [[3, 3]])).toMatchObject({ handled: true, text: 'SELECT' })
  })
})
