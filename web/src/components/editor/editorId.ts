/**
 * Id of the editor whose view holds a component, given by the view
 * (`EditorView`) to the components under it: the result grid publishes its
 * selection to the data explorer of its own editor, without the components
 * in between carrying the id.
 */
import { type InjectionKey, inject } from 'vue'

/**
 * Key of the editor's id among the values given by the view.
 */
export const EDITOR_ID: InjectionKey<number> = Symbol('editor id')

/**
 * Id of the editor whose view holds the calling component; 0 outside a view
 * (a component mounted alone, in a test).
 */
export function injectEditorId(): number {
  return inject(EDITOR_ID, 0)
}
