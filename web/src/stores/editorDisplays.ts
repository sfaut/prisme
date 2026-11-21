/**
 * Displays of the editors: for each editor, its panels (object navigator,
 * editor, results, Pivoteur, data explorer, console), open or closed with
 * their size, the tree of its object navigator and its result tab shown. The panels and the
 * activity bar of an editor's tab read the same display, which outlives the
 * view (an editor without a connection has none).
 *
 * The display of each editor is saved on the server side, with its editor
 * (`PUT /api/editors/{id}/display`, `crates/server/src/editor_display.rs`),
 * shortly after its last change (`utils/deferredSave.ts`): it survives a page
 * reload, and the closing of the editor followed by its reopening. The
 * editors store hands over the display received with each editor (`seed`);
 * an editor without a saved display, a new one for example, starts from the
 * defaults (`defaultEditorDisplay`). The schema is documented in
 * `docs/DEVELOPER.md`, section "Saved display".
 */

import { defineStore } from 'pinia'
import { type EffectScope, effectScope, reactive, watch } from 'vue'

import { saveEditorDisplay } from '@/api/editors'
import type { EditorDisplay } from '@/api/generated/EditorDisplay'
import { SOURCES_KEY } from '@/components/navigator/navigatorNodes'
import { deferredSave, removeStoredKeys } from '@/utils/deferredSave'

/**
 * Starts of the keys under which the displays were kept in the browser
 * before being saved with their editor; they are removed.
 */
const LEGACY_PREFIXES = ['prisme.navigator.', 'prisme.editor.layout.', 'prisme.explorer.layout.']

/**
 * Display of a new editor: the navigator open, at the width of the theme,
 * on the root of the sources; the editor alone in the view, the results and
 * the Pivoteur closed; the data explorer closed; the console open, at the
 * height of the theme; no result tab chosen.
 */
export function defaultEditorDisplay(): EditorDisplay {
  return {
    panels: {
      navigator: { open: true, size: null },
      explorer: { open: false, size: null },
      editor: { open: true, size: null },
      results: { open: false, size: null },
      pivot: { open: false, size: null },
      console: { open: true, size: null },
    },
    navigator: { expanded: [SOURCES_KEY], scroll_top: 0, tables_by_rows: [] },
    active_result: null,
  }
}

/**
 * Display of an editor, with the scope of the watcher that saves it.
 */
interface DisplayEntry {
  /**
   * Display of the editor.
   */
  display: EditorDisplay
  /**
   * Was the display saved, or changed since it was read? A new display lets
   * the navigator open the source of its editor by itself.
   */
  saved: boolean
  /**
   * Scope of the watcher that saves it.
   */
  scope: EffectScope
}

export const useEditorDisplaysStore = defineStore('editorDisplays', () => {
  removeStoredKeys((key) => LEGACY_PREFIXES.some((prefix) => key.startsWith(prefix)))

  /**
   * Displays read so far, by editor.
   */
  const entries = new Map<number, DisplayEntry>()

  /**
   * Displays received with the editors, not read yet, by editor; `null`:
   * none saved.
   */
  const seeds = new Map<number, EditorDisplay | null>()

  const saves = deferredSave<number>(async (editorId, keepalive) => {
    const entry = entries.get(editorId)
    if (entry) {
      await saveEditorDisplay(editorId, entry.display, { keepalive })
    }
  })

  /**
   * Hands over the display of an editor, as received with the editor; it is
   * read when the display is first needed.
   *
   * @param editorId Id of the editor.
   * @param display Saved display, `null` if none.
   */
  function seed(editorId: number, display: EditorDisplay | null): void {
    if (!entries.has(editorId)) {
      seeds.set(editorId, display)
    }
  }

  /**
   * Display of an editor and its state, made from its saved display the
   * first time.
   *
   * @param editorId Id of the editor.
   */
  function entryOf(editorId: number): DisplayEntry {
    const known = entries.get(editorId)
    if (known) {
      return known
    }
    const saved = seeds.get(editorId) ?? null
    seeds.delete(editorId)
    const scope = effectScope(true)
    const entry: DisplayEntry = {
      display: reactive(saved ?? defaultEditorDisplay()),
      saved: saved !== null,
      scope,
    }
    scope.run(() =>
      watch(
        () => entry.display,
        () => {
          entry.saved = true
          saves.schedule(editorId)
        },
        { deep: true },
      ),
    )
    entries.set(editorId, entry)
    return entry
  }

  /**
   * Display of an editor.
   *
   * @param editorId Id of the editor.
   */
  function displayOf(editorId: number): EditorDisplay {
    return entryOf(editorId).display
  }

  /**
   * Was the display of an editor saved, or changed since it was read? A new
   * one lets the navigator open the source of its editor.
   *
   * @param editorId Id of the editor.
   */
  function isSaved(editorId: number): boolean {
    return entryOf(editorId).saved
  }

  /**
   * Displays read so far.
   */
  function loaded(): EditorDisplay[] {
    return [...entries.values()].map((entry) => entry.display)
  }

  /**
   * Saves at once the display of an editor if a save is waiting for its
   * delay: before the editor closes, which refuses later saves.
   *
   * @param editorId Id of the editor.
   */
  async function flush(editorId: number): Promise<void> {
    await saves.flush(editorId)
  }

  /**
   * Leaves the display of a closed editor: it stays saved with the editor,
   * which finds it again when reopened.
   *
   * @param editorId Id of the editor.
   */
  function forget(editorId: number): void {
    saves.cancel(editorId)
    entries.get(editorId)?.scope.stop()
    entries.delete(editorId)
    seeds.delete(editorId)
  }

  return { seed, displayOf, isSaved, loaded, flush, forget }
})
