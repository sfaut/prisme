/**
 * Retrait dans l'éditeur SQL : Tab et Maj-Tab indentent au lieu de
 * déplacer le focus vers l'élément suivant de la page (ticket #2), et
 * Entrée va à la ligne sans retrait (ticket #5) : la langue SQL de
 * CodeMirror ajouterait deux espaces à chaque suite de requête.
 *
 * - Tab sans sélection : des espaces jusqu'au prochain taquet (tous les
 *   quatre caractères), à la position du curseur.
 * - Tab avec une sélection : les lignes touchées avancent d'un niveau.
 * - Maj-Tab : les lignes touchées reculent d'un niveau, avec ou sans sélection.
 *
 * Une ligne réindentée voit ses tabulations de tête remplacées par des
 * espaces : son retrait est recalculé en colonnes, puis réécrit en espaces
 * seuls, aligné sur un taquet.
 *
 * Pour sortir de l'éditeur au clavier, CodeMirror garde ses propres
 * raccourcis : Échap puis Tab, ou Ctrl-M qui laisse Tab au navigateur.
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
 * Écart entre deux taquets de tabulation, en colonnes.
 */
export const TAB_SIZE = 4

/**
 * Sens d'une réindentation : un niveau de plus, ou un de moins.
 */
export type IndentDirection = 'more' | 'less'

/**
 * Nombre d'espaces qui mènent du curseur au taquet suivant.
 *
 * @param column Colonne du curseur, à partir de 0.
 */
export function spacesToNextStop(column: number): number {
  return TAB_SIZE - (column % TAB_SIZE)
}

/**
 * Retrait d'une ligne réindentée, en espaces seuls, aligné sur un taquet.
 *
 * Un retrait entre deux taquets avance jusqu'au suivant ou recule jusqu'au
 * précédent. Un retrait nul ne recule pas.
 *
 * @param indentation Blancs de tête de la ligne (espaces et tabulations).
 * @param direction Sens de la réindentation.
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
 * Réindente toutes les lignes touchées par la sélection, chacune une seule
 * fois. La dernière ligne d'une sélection qui s'arrête au tout début de
 * cette ligne n'est pas touchée.
 *
 * @param state État de l'éditeur.
 * @param direction Sens de la réindentation.
 * @returns La transaction, ou `null` si aucune ligne ne change.
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
  // Une borne en début de ligne y reste ; les autres suivent le retrait.
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
 * Tab : espaces jusqu'au prochain taquet à chaque curseur, ou, si du texte
 * est sélectionné, un niveau de retrait de plus sur les lignes touchées.
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
 * Entrée : un retour à la ligne à chaque curseur (la sélection est
 * remplacée), le curseur en première colonne de la nouvelle ligne, sans
 * retrait.
 */
export const insertPlainNewline: StateCommand = ({ state, dispatch }) => {
  if (state.readOnly) {
    return false
  }
  dispatch(state.update(state.replaceSelection('\n'), { scrollIntoView: true, userEvent: 'input' }))
  return true
}

/**
 * Maj-Tab : un niveau de retrait de moins sur les lignes touchées par la
 * sélection, ou sur la ligne du curseur.
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
