/**
 * Layouts of the views of the editors (`EditorView.vue`): for each editor,
 * the heights of its panels (editor, results, Pivoteur) and which ones are
 * closed (`editorLayout.ts`), kept in the browser's local storage. The view
 * lays its panels out with it, and the activity bar of the editor's tab
 * (`PanelActivityBar.vue`) opens and closes them: both read the same layout,
 * which outlives the view (an editor without a connection has none).
 */
import { defineStore } from 'pinia'
import { effectScope } from 'vue'

import {
  type EditorLayout,
  defaultEditorLayout,
  editorLayoutKey,
  forgetEditorLayout,
} from '@/components/editor/editorLayout'
import { useStoredLayout } from '@/components/split/storedLayout'

/**
 * Layout of an editor's view, with the scope of its saving.
 */
interface LayoutEntry {
  /**
   * Layout of the view.
   */
  layout: EditorLayout
  /**
   * Scope of the watcher that saves it.
   */
  scope: ReturnType<typeof effectScope>
}

export const useEditorLayoutsStore = defineStore('editorLayouts', () => {
  /**
   * Layouts read so far, by editor.
   */
  const entries = new Map<number, LayoutEntry>()

  /**
   * Layout of an editor's view, read from local storage the first time; a
   * new editor starts from the default layout.
   *
   * @param editorId Id of the editor.
   */
  function layoutOf(editorId: number): EditorLayout {
    const known = entries.get(editorId)
    if (known) {
      return known.layout
    }
    const scope = effectScope(true)
    const layout = scope.run(() =>
      useStoredLayout(editorLayoutKey(editorId), defaultEditorLayout()),
    )!
    entries.set(editorId, { layout, scope })
    return layout
  }

  /**
   * Forgets the layout of a closed editor: a reopened editor starts from the
   * default layout.
   *
   * @param editorId Id of the editor.
   */
  function forget(editorId: number): void {
    entries.get(editorId)?.scope.stop()
    entries.delete(editorId)
    forgetEditorLayout(editorId)
  }

  return { layoutOf, forget }
})
