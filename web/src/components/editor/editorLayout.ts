/**
 * Layout of the view of an editor (`EditorView`): the heights of its panels, in CSS
 * pixels, and which ones are closed (the names `…Folded` are those of the
 * storage), kept in the browser's local storage (`split/storedLayout.ts`),
 * one entry per editor (`editorLayouts` store), so that closing the results
 * of one tab leaves the others as they are. Closing an editor forgets its
 * entry: a reopened editor starts from the default layout.
 *
 * The three panels of the view, from top to bottom, are the editor, the
 * results and the Pivoteur. The results and the Pivoteur depend on the editor:
 * they start closed. The highest open panel fills the height the others
 * leave; the others keep the height they were given, or share the room with
 * it.
 *
 * The console of the editor, at the bottom of its tab under the side panels
 * and the view, keeps its height and its state here too: open by default,
 * at the height of the theme.
 */
import { forgetStoredLayout } from '@/components/split/storedLayout'

/**
 * Panel of the view of an editor.
 */
export type ViewPanel = 'editor' | 'results' | 'pivot'

/**
 * Panels of the view, from top to bottom.
 */
export const VIEW_PANELS: readonly ViewPanel[] = ['editor', 'results', 'pivot']

/**
 * Layout of the view: heights in CSS pixels (`null`: a share of the room),
 * and closed panels. The editor has no height of its own: open, it fills.
 * Then the console: its height in CSS pixels (`null`: that of the theme),
 * and whether it is closed.
 */
export interface EditorLayout {
  [name: string]: number | boolean | null
  results: number | null
  pivot: number | null
  editorFolded: boolean
  resultsFolded: boolean
  pivotFolded: boolean
  console: number | null
  consoleFolded: boolean
}

/**
 * Layout of a new editor: the editor alone, the results and the Pivoteur
 * closed; the console open, at the height of the theme.
 */
export function defaultEditorLayout(): EditorLayout {
  return {
    results: null,
    pivot: null,
    editorFolded: false,
    resultsFolded: true,
    pivotFolded: true,
    console: null,
    consoleFolded: false,
  }
}

/**
 * Is a panel closed?
 *
 * @param layout Layout of the view.
 * @param panel Panel.
 */
export function isFolded(layout: EditorLayout, panel: ViewPanel): boolean {
  return layout[`${panel}Folded`] === true
}

/**
 * Panel that fills the height the others leave: the highest open one;
 * `null` when all are closed.
 *
 * @param layout Layout of the view.
 */
export function fillingPanel(layout: EditorLayout): ViewPanel | null {
  return VIEW_PANELS.find((panel) => !isFolded(layout, panel)) ?? null
}

/**
 * Key of an editor's layout in the local storage.
 *
 * @param editorId Id of the editor.
 */
export function editorLayoutKey(editorId: number): string {
  return `prisme.editor.layout.${editorId}`
}

/**
 * Forgets the layout of a closed editor.
 *
 * @param editorId Id of the editor.
 */
export function forgetEditorLayout(editorId: number): void {
  forgetStoredLayout(editorLayoutKey(editorId))
}
