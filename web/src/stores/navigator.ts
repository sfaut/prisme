/**
 * Object navigators, the panels on the left of the editors
 * (`NavigatorPanel.vue`): each editor has its own, with its layout (open or
 * closed, its width) and its tree (open nodes, scroll), so that going from
 * one editor to another finds each navigator as it was left.
 *
 * The state of each navigator is saved on the server side, with its editor
 * (`PUT /api/editors/{id}/navigator`, `crates/server/src/navigator_state.rs`),
 * shortly after its last change: it survives a page reload, and the closing
 * of the editor followed by its reopening. The editors store hands over the
 * state received with each editor (`seed`); an editor without a saved state,
 * a new one for example, starts from the defaults: open, at the width of the
 * theme, the root of the sources open.
 */

import { defineStore } from 'pinia'
import { type EffectScope, effectScope, reactive, watch } from 'vue'

import { saveNavigator } from '@/api/editors'
import type { NavigatorState } from '@/api/generated/NavigatorState'
import { SOURCES_KEY, belongsToSource } from '@/components/navigator/navigatorNodes'

/**
 * Delay between the last change of a navigator (a scroll moves at every
 * frame) and its save, in milliseconds.
 */
const SAVE_DELAY = 300

/**
 * Start of the keys under which the navigators were kept in the browser
 * before being saved with their editor; they are removed.
 */
const LEGACY_PREFIX = 'prisme.navigator.'

/**
 * Layout of an editor's navigator.
 */
export type NavigatorLayout = {
  /**
   * Is the navigator open?
   */
  open: boolean
  /**
   * Width given by the user, in CSS pixels; `null`: the default width of the
   * theme.
   */
  width: number | null
}

/**
 * Tree of an editor's navigator.
 */
export interface NavigatorTree {
  /**
   * Keys of the open nodes.
   */
  expanded: string[]
  /**
   * Position of the tree's scroll, in CSS pixels.
   */
  scrollTop: number
  /**
   * Was the tree saved before? A new tree opens the source of its editor by
   * itself; a saved one stays as the user left it.
   */
  known: boolean
}

/**
 * Navigator of an editor: its layout, its tree, the scope of the watcher
 * that saves them, and its pending save.
 */
interface NavigatorEntry {
  layout: NavigatorLayout
  tree: NavigatorTree
  scope: EffectScope
  timer: ReturnType<typeof setTimeout> | undefined
}

/**
 * Removes the navigators kept in the browser's local storage before they
 * were saved with their editor; no effect if storage is unavailable.
 */
function removeLegacyKeys(): void {
  try {
    const keys = Object.keys(window.localStorage).filter((key) => key.startsWith(LEGACY_PREFIX))
    for (const key of keys) {
      window.localStorage.removeItem(key)
    }
  } catch {
    // Storage unavailable: nothing was kept.
  }
}

/**
 * State of a navigator, as the server saves it.
 *
 * @param entry Navigator of an editor.
 */
function stateOf(entry: NavigatorEntry): NavigatorState {
  return {
    open: entry.layout.open,
    width: entry.layout.width,
    expanded: [...entry.tree.expanded],
    scroll_top: entry.tree.scrollTop,
  }
}

export const useNavigatorStore = defineStore('navigator', () => {
  removeLegacyKeys()

  /**
   * Navigators read so far, by editor.
   */
  const entries = new Map<number, NavigatorEntry>()

  /**
   * States received with the editors, not read yet, by editor; `null`: none
   * saved.
   */
  const seeds = new Map<number, NavigatorState | null>()

  /**
   * Hands over the state of an editor's navigator, as received with the
   * editor; it is read when the navigator is first needed.
   *
   * @param editorId Id of the editor.
   * @param state Saved state, `null` if none.
   */
  function seed(editorId: number, state: NavigatorState | null): void {
    if (!entries.has(editorId)) {
      seeds.set(editorId, state)
    }
  }

  /**
   * Saves the navigator of an editor at once; a failure shows in the
   * synchronisation state (`syncState`).
   *
   * @param editorId Id of the editor.
   * @param entry Its navigator.
   * @param keepalive Lets the save outlive the page, for a save made as it closes.
   */
  async function save(editorId: number, entry: NavigatorEntry, keepalive = false): Promise<void> {
    clearTimeout(entry.timer)
    entry.timer = undefined
    await saveNavigator(editorId, stateOf(entry), { keepalive }).catch(() => {
      // Shown by the synchronisation state; the next change saves again.
    })
  }

  /**
   * Saves the navigator of an editor shortly after its last change.
   *
   * @param editorId Id of the editor.
   * @param entry Its navigator.
   */
  function schedule(editorId: number, entry: NavigatorEntry): void {
    clearTimeout(entry.timer)
    entry.timer = setTimeout(() => void save(editorId, entry), SAVE_DELAY)
  }

  /**
   * Navigator of an editor, made from its saved state the first time.
   *
   * @param editorId Id of the editor.
   */
  function entryOf(editorId: number): NavigatorEntry {
    const known = entries.get(editorId)
    if (known) {
      return known
    }
    const saved = seeds.get(editorId) ?? null
    seeds.delete(editorId)
    const scope = effectScope(true)
    const entry: NavigatorEntry = {
      layout: reactive({ open: saved?.open ?? true, width: saved?.width ?? null }),
      tree: reactive({
        expanded: saved?.expanded ?? [SOURCES_KEY],
        scrollTop: saved?.scroll_top ?? 0,
        known: saved !== null,
      }),
      scope,
      timer: undefined,
    }
    scope.run(() =>
      watch([() => entry.layout.open, () => entry.layout.width, () => entry.tree.expanded], () =>
        schedule(editorId, entry),
      ),
    )
    entries.set(editorId, entry)
    return entry
  }

  /**
   * Layout of an editor's navigator: open or folded, and its width.
   *
   * @param editorId Id of the editor.
   */
  function layoutOf(editorId: number): NavigatorLayout {
    return entryOf(editorId).layout
  }

  /**
   * Tree of an editor's navigator: open nodes and scroll.
   *
   * @param editorId Id of the editor.
   */
  function treeOf(editorId: number): NavigatorTree {
    return entryOf(editorId).tree
  }

  /**
   * Notes the scroll of an editor's tree; it is saved shortly after the last
   * move.
   *
   * @param editorId Id of the editor.
   * @param scrollTop Position of the scroll, in CSS pixels.
   */
  function scrollTo(editorId: number, scrollTop: number): void {
    const entry = entryOf(editorId)
    entry.tree.scrollTop = scrollTop
    schedule(editorId, entry)
  }

  /**
   * Saves at once the navigator of an editor if a save is waiting for its
   * delay: before the editor closes, which refuses later saves.
   *
   * @param editorId Id of the editor.
   */
  async function flush(editorId: number): Promise<void> {
    const entry = entries.get(editorId)
    if (entry?.timer !== undefined) {
      await save(editorId, entry)
    }
  }

  /**
   * Saves at once the navigators waiting for their delay. Called when the
   * page is hidden or closed: a save left waiting would be lost with the page.
   */
  function flushPending(): void {
    for (const [editorId, entry] of entries) {
      if (entry.timer !== undefined) {
        void save(editorId, entry, true)
      }
    }
  }

  /**
   * Leaves the navigator of a closed editor: its state stays saved with the
   * editor, which finds it again when reopened.
   *
   * @param editorId Id of the editor.
   */
  function forget(editorId: number): void {
    const entry = entries.get(editorId)
    if (entry) {
      clearTimeout(entry.timer)
      entry.scope.stop()
      entries.delete(editorId)
    }
    seeds.delete(editorId)
  }

  /**
   * Closes a deleted source, and the nodes under it, in every navigator.
   *
   * @param connectionId Id of the source's connection.
   */
  function forgetSource(connectionId: number): void {
    for (const { tree } of entries.values()) {
      if (tree.expanded.some((key) => belongsToSource(key, connectionId))) {
        tree.expanded = tree.expanded.filter((key) => !belongsToSource(key, connectionId))
      }
    }
  }

  // The page can close within the delay of a save: the pending states go out first.
  document.addEventListener('visibilitychange', () => {
    if (document.visibilityState === 'hidden') {
      flushPending()
    }
  })
  window.addEventListener('pagehide', flushPending)

  return { seed, layoutOf, treeOf, scrollTo, flush, forget, forgetSource }
})
