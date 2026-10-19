/**
 * Removal of the blanks at the end of the lines in the SQL editor (ticket
 * #15): when the cursor leaves a line, the blanks that end it (spaces, tabs)
 * are removed; the line breaks stay.
 *
 * - A line is left when a cursor that stood on it goes to another line: arrow
 *   keys, mouse, Enter. Only the user's actions count: a text replaced from
 *   outside or a generated query trims nothing, nor do undo and redo, which
 *   must be able to give the blanks back.
 * - Blanks inside a string or a quoted identifier that goes on to the next
 *   line belong to the value (`'Bonjour···⏎le monde'`): they stay. Those of a
 *   comment go, like the others.
 * - The removal joins the transaction that moves the cursor: Ctrl+Z undoes
 *   both at once.
 */

import { syntaxTree } from '@codemirror/language'
import {
  type ChangeSpec,
  EditorState,
  type Extension,
  Transaction,
  type TransactionSpec,
} from '@codemirror/state'

/**
 * Blanks at the end of a line (CodeMirror lines hold no line break).
 */
const TRAILING_BLANKS = /\s+$/u

/**
 * Nodes of the SQL syntax whose content is data: their blanks stay.
 */
const DATA_NODES = new Set(['String', 'QuotedIdentifier'])

/**
 * Does the position stand inside a string or a quoted identifier that goes on
 * after it?
 *
 * @param state State of the editor.
 * @param position Position in the text.
 */
function insideData(state: EditorState, position: number): boolean {
  // Strings and quoted identifiers are tokens: the innermost node is the one.
  const node = syntaxTree(state).resolveInner(position, -1)
  return DATA_NODES.has(node.name) && node.to > position
}

/**
 * Removals of the blanks that end the lines the transaction leaves, in
 * positions of the text after the transaction.
 *
 * @param transaction Transaction of the user.
 */
function trimsOfLeftLines(transaction: Transaction): ChangeSpec[] {
  const { startState, state, changes } = transaction
  const reached = new Set(
    state.selection.ranges.map((range) => state.doc.lineAt(range.head).number),
  )
  const left = new Set<number>()
  for (const range of startState.selection.ranges) {
    const line = startState.doc.lineAt(range.head)
    const number = state.doc.lineAt(changes.mapPos(line.from, -1)).number
    if (!reached.has(number)) {
      left.add(number)
    }
  }
  const trims: ChangeSpec[] = []
  for (const number of left) {
    const line = state.doc.line(number)
    const blanks = TRAILING_BLANKS.exec(line.text)?.[0]
    if (blanks !== undefined && !insideData(state, line.to)) {
      trims.push({ from: line.to - blanks.length, to: line.to })
    }
  }
  return trims
}

/**
 * Editor extension: the blanks at the end of a line go when the cursor leaves it.
 */
export function trimTrailingBlanks(): Extension {
  return EditorState.transactionFilter.of(
    (transaction): TransactionSpec | readonly TransactionSpec[] => {
      const byUser =
        transaction.annotation(Transaction.userEvent) !== undefined &&
        !transaction.isUserEvent('undo') &&
        !transaction.isUserEvent('redo')
      if (!byUser || (!transaction.docChanged && transaction.selection === undefined)) {
        return transaction
      }
      const trims = trimsOfLeftLines(transaction)
      return trims.length === 0 ? transaction : [transaction, { changes: trims, sequential: true }]
    },
  )
}
