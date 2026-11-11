import { MySQL, sql } from '@codemirror/lang-sql'
import { EditorSelection, EditorState, type TransactionSpec } from '@codemirror/state'
import { describe, expect, it } from 'vitest'

import { trimTrailingBlanks } from '../editorTrim'

/**
 * State of an editor with the SQL language and the removal of the blanks.
 *
 * @param doc Text.
 * @param cursors Positions of the cursors.
 */
function editor(doc: string, ...cursors: number[]): EditorState {
  return EditorState.create({
    doc,
    selection: EditorSelection.create(cursors.map((cursor) => EditorSelection.cursor(cursor))),
    extensions: [
      sql({ dialect: MySQL }),
      trimTrailingBlanks(),
      EditorState.allowMultipleSelections.of(true),
    ],
  })
}

/**
 * Applies a transaction and returns the text and the cursors.
 *
 * @param state Initial state.
 * @param spec Transaction.
 */
function after(state: EditorState, spec: TransactionSpec) {
  const result = state.update(spec).state
  return {
    text: result.doc.toString(),
    cursors: result.selection.ranges.map((range) => range.head),
  }
}

describe('blanks at the end of the lines', () => {
  it('go when the cursor leaves the line, and the cursor follows the text', () => {
    expect(
      after(editor('SELECT a,   \nFROM t', 12), { selection: { anchor: 17 }, userEvent: 'select' }),
    ).toEqual({ text: 'SELECT a,\nFROM t', cursors: [14] })
  })

  it('stay while the cursor stays on the line', () => {
    expect(
      after(editor('SELECT a,   \nFROM t', 12), { selection: { anchor: 3 }, userEvent: 'select' })
        .text,
    ).toBe('SELECT a,   \nFROM t')
  })

  it('go on Enter, from the line the cursor leaves', () => {
    expect(
      after(editor('SELECT a,   ', 12), {
        changes: { from: 12, insert: '\n' },
        selection: { anchor: 13 },
        userEvent: 'input',
      }).text,
    ).toBe('SELECT a,\n')
  })

  it('stay inside a string or a quoted identifier that goes on to the next line', () => {
    const string = "SELECT 'Bonjour   \nle monde';"
    expect(after(editor(string, 18), { selection: { anchor: 22 }, userEvent: 'select' }).text).toBe(
      string,
    )
    const identifier = 'SELECT 1 AS `total   \nbrut`;'
    expect(
      after(editor(identifier, 21), { selection: { anchor: 24 }, userEvent: 'select' }).text,
    ).toBe(identifier)
  })

  it('go after a closed string, and in a comment', () => {
    expect(
      after(editor("SELECT 'a'   \nFROM t", 13), { selection: { anchor: 16 }, userEvent: 'select' })
        .text,
    ).toBe("SELECT 'a'\nFROM t")
    expect(
      after(editor('-- note \t \nSELECT 1', 10), { selection: { anchor: 13 }, userEvent: 'select' })
        .text,
    ).toBe('-- note\nSELECT 1')
  })

  it('go from each line a cursor leaves', () => {
    const state = editor('a  \nb  \nc', 3, 7)
    expect(
      after(state, {
        selection: EditorSelection.create([EditorSelection.cursor(9)]),
        userEvent: 'select',
      }).text,
    ).toBe('a\nb\nc')
  })

  it('stay on a change that does not come from the user, and on undo', () => {
    const state = editor('SELECT a,   \nFROM t', 12)
    expect(after(state, { selection: { anchor: 17 } }).text).toBe('SELECT a,   \nFROM t')
    expect(after(state, { selection: { anchor: 17 }, userEvent: 'undo' }).text).toBe(
      'SELECT a,   \nFROM t',
    )
  })
})
