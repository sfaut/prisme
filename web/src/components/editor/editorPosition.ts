/**
 * Position in the text of the SQL editor (`SqlEditor.vue`), saved with the
 * display of its editor (`SqlPosition`): the selection, the first line shown
 * at the top, the folded passages. The positions are counted in UTF-16 code
 * units, as CodeMirror counts them. The text may have changed since the
 * position was saved (saved later, or changed elsewhere): a position read
 * back is brought within the text (`withinText`).
 */
import { foldEffect, foldedRanges } from '@codemirror/language'
import type { StateEffect } from '@codemirror/state'
import { EditorView } from '@codemirror/view'

import type { SqlPosition } from '@/api/generated/SqlPosition'

/**
 * Position at the start of the text, without a fold.
 */
export function startPosition(): SqlPosition {
  return { anchor: 0, head: 0, top: 0, folds: [] }
}

/**
 * Saved position brought within a text: each position beyond its end comes
 * back to its end, and the folds that no longer fit are dropped.
 *
 * @param position Saved position.
 * @param length Length of the text, in UTF-16 code units.
 */
export function withinText(position: SqlPosition, length: number): SqlPosition {
  const clamp = (offset: number) => Math.min(Math.max(offset, 0), length)
  return {
    anchor: clamp(position.anchor),
    head: clamp(position.head),
    top: clamp(position.top),
    folds: position.folds.filter(([from, to]) => from >= 0 && from < to && to <= length),
  }
}

/**
 * Effects that put a new editor back at a saved position: its folds, and
 * the first line shown. The selection goes to the creation of the view.
 *
 * @param position Position brought within the text (`withinText`).
 */
export function restoringEffects(position: SqlPosition): StateEffect<unknown>[] {
  return [
    ...position.folds.map(([from, to]) => foldEffect.of({ from, to })),
    EditorView.scrollIntoView(position.top, { y: 'start' }),
  ]
}

/**
 * Current position of an editor.
 *
 * @param view View of the editor.
 */
export function positionOf(view: EditorView): SqlPosition {
  const { anchor, head } = view.state.selection.main
  const folds: [number, number][] = []
  foldedRanges(view.state).between(0, view.state.doc.length, (from, to) => {
    folds.push([from, to])
  })
  const height = Math.max(view.scrollDOM.scrollTop - view.documentPadding.top, 0)
  return { anchor, head, top: view.lineBlockAtHeight(height).from, folds }
}
