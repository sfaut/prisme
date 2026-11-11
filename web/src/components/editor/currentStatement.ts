/**
 * Highlighting, in the editor, of the query that Ctrl+Enter would run.
 *
 * The spans of the queries come from the server (`/api/analyze`, same
 * splitting as the run), requested shortly after each keystroke. While
 * waiting for the answer, they follow the changes of the text. The query
 * under the cursor follows the server's rule (`statement_at`): the last one
 * that starts before the cursor, otherwise the first one. Nothing is
 * highlighted as long as the text counts fewer than two queries: there is
 * then nothing to choose.
 */

import { type EditorState, type Extension, StateEffect, StateField } from '@codemirror/state'
import { Decoration, type DecorationSet, EditorView } from '@codemirror/view'

import type { StatementSpan } from '@/api/generated/StatementSpan'

/**
 * Replaces the spans of the queries, on receiving an analysis.
 */
export const setStatements = StateEffect.define<StatementSpan[]>()

/**
 * Spans of the text's queries, updated on each change.
 */
const statementsField = StateField.define<StatementSpan[]>({
  create: () => [],
  update(statements, transaction) {
    for (const effect of transaction.effects) {
      if (effect.is(setStatements)) {
        return effect.value
      }
    }
    if (!transaction.docChanged) {
      return statements
    }
    return statements.map(({ start, end }) => ({
      start: transaction.changes.mapPos(start, 1),
      end: transaction.changes.mapPos(end, -1),
    }))
  },
})

/**
 * Index of the query under the cursor: the last one that starts before it,
 * otherwise the first one; `null` if there is no query.
 *
 * @param statements Spans of the queries, in order.
 * @param cursor Position of the cursor, in UTF-16 units.
 */
export function statementAt(statements: StatementSpan[], cursor: number): number | null {
  if (statements.length === 0) {
    return null
  }
  let index = 0
  statements.forEach((statement, position) => {
    if (statement.start <= cursor) {
      index = position
    }
  })
  return index
}

/**
 * Mark of the query under the cursor.
 */
const currentMark = Decoration.mark({ class: 'cm-current-statement' })

/**
 * Highlight of the query under the cursor, empty if there are fewer than two queries.
 *
 * @param state State of the editor.
 */
function currentDecoration(state: EditorState): DecorationSet {
  const statements = state.field(statementsField)
  const index = statements.length < 2 ? null : statementAt(statements, state.selection.main.head)
  const statement = index === null ? undefined : statements[index]
  if (statement === undefined || statement.end <= statement.start) {
    return Decoration.none
  }
  return Decoration.set([currentMark.range(statement.start, statement.end)])
}

/**
 * Highlight recomputed on each change of the text, the cursor or the queries.
 */
const decorationField = StateField.define<DecorationSet>({
  create: currentDecoration,
  update: (decorations, transaction) =>
    transaction.docChanged ||
    transaction.selection !== undefined ||
    transaction.effects.some((effect) => effect.is(setStatements))
      ? currentDecoration(transaction.state)
      : decorations,
  provide: (field) => EditorView.decorations.from(field),
})

/**
 * Editor extension: highlighting of the query under the cursor.
 */
export function currentStatement(): Extension {
  return [
    statementsField,
    decorationField,
    EditorView.baseTheme({
      '.cm-current-statement': {
        backgroundColor: 'var(--editor-current-statement)',
      },
    }),
  ]
}
