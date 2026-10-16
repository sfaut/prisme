/**
 * Indentation in the SQL editor: Tab and Shift+Tab indent instead of moving
 * the focus to the next element of the page (ticket #2), and Enter goes to a
 * new line without indentation (ticket #5): CodeMirror's SQL language would
 * add two spaces to each continuation of a query.
 *
 * - Tab without a selection: spaces up to the next tab stop (every four
 *   characters), at the cursor position.
 * - Tab with a selection: the touched lines move forward one level.
 * - Shift+Tab: the touched lines move back one level, with or without a
 *   selection.
 *
 * A reindented line has its leading tabs replaced by spaces: its
 * indentation is recomputed in columns, then rewritten in spaces only,
 * aligned on a tab stop.
 *
 * To leave the editor with the keyboard, CodeMirror keeps its own shortcuts:
 * Escape then Tab, or Ctrl+M which leaves Tab to the browser.
 */

import {
  type ChangeSpec,
  EditorSelection,
  type EditorState,
  type StateCommand,
  type Transaction,
  countColumn,
} from '@codemirror/state'

/**
 * Gap between two tab stops, in columns.
 */
export const TAB_SIZE = 4

/**
 * Direction of a reindentation: one level more, or one less.
 */
export type IndentDirection = 'more' | 'less'

/**
 * Number of spaces that lead from the cursor to the next tab stop.
 *
 * @param column Column of the cursor, from 0.
 */
export function spacesToNextStop(column: number): number {
  return TAB_SIZE - (column % TAB_SIZE)
}

/**
 * Indentation of a reindented line, in spaces only, aligned on a tab stop.
 *
 * An indentation between two tab stops moves forward to the next one or back
 * to the previous one. A zero indentation does not move back.
 *
 * @param indentation Leading blanks of the line (spaces and tabs).
 * @param direction Direction of the reindentation.
 */
export function reindented(indentation: string, direction: IndentDirection): string {
  const column = countColumn(indentation, TAB_SIZE)
  const level =
    direction === 'more'
      ? Math.floor(column / TAB_SIZE) + 1
      : Math.max(0, Math.ceil(column / TAB_SIZE) - 1)
  return ' '.repeat(level * TAB_SIZE)
}

/**
 * Reindents all the lines touched by the selection, each one only once. The
 * last line of a selection that stops at the very start of this line is not
 * touched.
 *
 * @param state State of the editor.
 * @param direction Direction of the reindentation.
 * @returns The transaction, or `null` if no line changes.
 */
function reindentLines(state: EditorState, direction: IndentDirection): Transaction | null {
  const changes: ChangeSpec[] = []
  const done = new Set<number>()
  for (const range of state.selection.ranges) {
    const first = state.doc.lineAt(range.from).number
    let last = state.doc.lineAt(range.to).number
    if (!range.empty && last > first && state.doc.lineAt(range.to).from === range.to) {
      last--
    }
    for (let number = first; number <= last; number++) {
      if (done.has(number)) {
        continue
      }
      done.add(number)
      const line = state.doc.line(number)
      const indentation = /^[ \t]*/.exec(line.text)?.[0] ?? ''
      const replacement = reindented(indentation, direction)
      if (replacement !== indentation) {
        changes.push({
          from: line.from,
          to: line.from + indentation.length,
          insert: replacement,
        })
      }
    }
  }
  if (changes.length === 0) {
    return null
  }
  const changeSet = state.changes(changes)
  // A bound at the start of a line stays there; the others follow the indentation.
  const selection = EditorSelection.create(
    state.selection.ranges.map((range) => {
      const map = (position: number) =>
        changeSet.mapPos(position, state.doc.lineAt(position).from === position ? -1 : 1)
      return EditorSelection.range(map(range.anchor), map(range.head))
    }),
    state.selection.mainIndex,
  )
  return state.update({
    changes: changeSet,
    selection,
    scrollIntoView: true,
    userEvent: direction === 'more' ? 'input.indent' : 'delete.dedent',
  })
}

/**
 * Tab: spaces up to the next tab stop at each cursor, or, if text is
 * selected, one more indentation level on the touched lines.
 */
export const indentOrInsertSpaces: StateCommand = ({ state, dispatch }) => {
  if (state.readOnly) {
    return false
  }
  if (state.selection.ranges.some((range) => !range.empty)) {
    const transaction = reindentLines(state, 'more')
    if (transaction !== null) {
      dispatch(transaction)
    }
    return true
  }
  const transaction = state.changeByRange((range) => {
    const line = state.doc.lineAt(range.head)
    const column = countColumn(line.text.slice(0, range.head - line.from), TAB_SIZE)
    const spaces = ' '.repeat(spacesToNextStop(column))
    return {
      changes: { from: range.head, insert: spaces },
      range: EditorSelection.cursor(range.head + spaces.length),
    }
  })
  dispatch(state.update(transaction, { scrollIntoView: true, userEvent: 'input' }))
  return true
}

/**
 * Enter: a line break at each cursor (the selection is replaced), the cursor
 * in the first column of the new line, without indentation.
 */
export const insertPlainNewline: StateCommand = ({ state, dispatch }) => {
  if (state.readOnly) {
    return false
  }
  dispatch(state.update(state.replaceSelection('\n'), { scrollIntoView: true, userEvent: 'input' }))
  return true
}

/**
 * Shift+Tab: one less indentation level on the lines touched by the
 * selection, or on the cursor's line.
 */
export const dedentLines: StateCommand = ({ state, dispatch }) => {
  if (state.readOnly) {
    return false
  }
  const transaction = reindentLines(state, 'less')
  if (transaction !== null) {
    dispatch(transaction)
  }
  return true
}
