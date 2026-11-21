/**
 * Layout of the view of an editor (`EditorView`): its three panels, from top
 * to bottom, are the editor, the results and the Pivoteur, each open or
 * closed, with its height. They are panels of the display of the editor
 * (`editorDisplays` store), saved with it on the server side.
 *
 * The results and the Pivoteur depend on the editor: they start closed. The
 * highest open panel fills the height the others leave; the others keep the
 * height they were given, or share the room with it.
 */
import type { Panels } from '@/api/generated/Panels'

/**
 * Panel of the view of an editor.
 */
export type ViewPanel = 'editor' | 'results' | 'pivot'

/**
 * Panels of the view, from top to bottom.
 */
export const VIEW_PANELS: readonly ViewPanel[] = ['editor', 'results', 'pivot']

/**
 * Panel that fills the height the others leave: the highest open one;
 * `null` when all are closed.
 *
 * @param panels Panels of the editor's tab.
 */
export function fillingPanel(panels: Panels): ViewPanel | null {
  return VIEW_PANELS.find((panel) => panels[panel].open) ?? null
}
