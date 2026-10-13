/**
 * Mise en évidence, dans l'éditeur, de la requête que Ctrl-Entrée exécuterait.
 *
 * Les emplacements des requêtes viennent du serveur (`/api/analyze`, même
 * découpage que l'exécution), demandés peu après chaque frappe. En attendant
 * la réponse, ils suivent les modifications du texte. La requête sous le
 * curseur suit la règle du serveur (`statement_at`) : la dernière qui
 * commence avant le curseur, sinon la première. Rien n'est surligné tant que
 * le texte compte moins de deux requêtes : il n'y a alors pas à choisir.
 */

import { type EditorState, type Extension, StateEffect, StateField } from '@codemirror/state'
import { Decoration, type DecorationSet, EditorView } from '@codemirror/view'

import type { StatementSpan } from '@/api/generated/StatementSpan'

/**
 * Remplace les emplacements des requêtes, à la réception d'une analyse.
 */
export const setStatements = StateEffect.define<StatementSpan[]>()

/**
 * Emplacements des requêtes du texte, mis à jour à chaque modification.
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
 * Indice de la requête sous le curseur : la dernière qui commence avant lui,
 * sinon la première ; `null` s'il n'y a aucune requête.
 *
 * @param statements Emplacements des requêtes, dans l'ordre.
 * @param cursor Position du curseur, en unités UTF-16.
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
 * Marque de la requête sous le curseur.
 */
const currentMark = Decoration.mark({ class: 'cm-current-statement' })

/**
 * Surlignage de la requête sous le curseur, vide s'il y a moins de deux requêtes.
 *
 * @param state État de l'éditeur.
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
 * Surlignage recalculé à chaque changement du texte, du curseur ou des requêtes.
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
 * Extension de l'éditeur : mise en évidence de la requête sous le curseur.
 */
export function currentStatement(): Extension {
  return [
    statementsField,
    decorationField,
    EditorView.baseTheme({
      '.cm-current-statement': {
        backgroundColor: 'color-mix(in oklab, var(--accent) 7%, transparent)',
      },
    }),
  ]
}
