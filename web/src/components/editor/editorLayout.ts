/**
 * Layout of the view of an editor (`EditorView`): the heights of its panels, in CSS
 * pixels, and which ones are folded, kept in the browser's local storage
 * (`split/storedLayout.ts`), one entry per editor, so that folding the editor
 * of one tab leaves the others as they are. Closing an editor forgets its
 * entry: a reopened editor starts from the default layout.
 */
import { forgetStoredLayout } from '@/components/split/storedLayout'

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
