/**
 * Highlight of the search results in the editor of the data explorer
 * (`DataExplorer.vue`): every node found, and the current one more strongly.
 * Going to a result unfolds what hides it and scrolls to it.
 *
 * The places come from the search (`dataSearch.ts`), for the document in the
 * editor; the colors from the theme (`editorTheme.ts`).
 */

import { foldedRanges, unfoldEffect } from '@codemirror/language'
import { type EditorState, StateEffect, StateField } from '@codemirror/state'
import { Decoration, type DecorationSet, EditorView } from '@codemirror/view'

import type { SearchRange } from './dataSearch'

/**
 * Results to highlight, and the index of the current one.
 */
export interface MatchHighlight {
  matches: SearchRange[]
  current: number
}

/**
 * Replaces the highlighted results.
 */
export const setMatches = StateEffect.define<MatchHighlight>()

/**
 * Mark of a result.
 */
const match = Decoration.mark({ class: 'cm-dataMatch' })

/**
 * Mark of the current result.
 */
const currentMatch = Decoration.mark({ class: 'cm-dataMatch-current' })

/**
 * Highlighted results, to place in the extensions of the editor.
 */
export const matchHighlight = StateField.define<DecorationSet>({
  create: () => Decoration.none,
  update(value, transaction) {
    for (const effect of transaction.effects) {
      if (effect.is(setMatches)) {
        const { matches, current } = effect.value
        return Decoration.set(
          matches.flatMap((range, index) =>
            range.from < range.to
              ? [(index === current ? currentMatch : match).range(range.from, range.to)]
              : [],
          ),
          true,
        )
      }
    }
    return value.map(transaction.changes)
  },
  provide: (field) => EditorView.decorations.from(field),
})

/**
 * Effects that show a result: unfold the folded parts that hide its start,
 * scroll to it.
 *
 * @param state State of the editor.
 * @param range Place of the result.
 */
export function revealEffects(state: EditorState, range: SearchRange): StateEffect<unknown>[] {
  const effects: StateEffect<unknown>[] = []
  foldedRanges(state).between(range.from, range.from, (from, to) => {
    if (from < range.from && range.from < to) {
      effects.push(unfoldEffect.of({ from, to }))
    }
  })
  effects.push(EditorView.scrollIntoView(range.from, { y: 'center' }))
  return effects
}
