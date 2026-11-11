/**
 * Object navigators, the panels on the left of the editors
 * (`NavigatorPanel.vue`): each editor has its own, with its layout (open or
 * folded, its width) and its tree (open nodes, scroll), so that going from
 * one editor to another finds each navigator as it was left. Folded, a
 * navigator leaves a rail on its edge, which opens it again (`PanelRail.vue`).
 *
 * These are display preferences specific to this machine, kept in the
 * browser's local storage, one entry per editor
 * (`prisme.navigator.layout.<editor>`, `prisme.navigator.tree.<editor>`),
 * forgotten when the editor closes (`forget`). The last layout given is kept
 * too (`prisme.navigator.open`, `prisme.navigator.width`): an editor without
 * an entry of its own, a new one for example, starts from it. When storage
 * is unavailable, the defaults apply: open, at the width of the theme, the
 * root of the sources open.
 */

import { defineStore } from 'pinia'
import { type EffectScope, effectScope, reactive, watch } from 'vue'

import { SOURCES_KEY, belongsToSource } from '@/components/navigator/navigatorNodes'
import { forgetStoredLayout, useStoredLayout } from '@/components/split/storedLayout'
import { loadFlag, loadSize, saveFlag, saveSize } from '@/utils/storedValues'

/**
 * Key of the last state given to a navigator, open or folded, in local
 * storage.
 */
const OPEN_KEY = 'prisme.navigator.open'

/**
 * Key of the last width given to a navigator in local storage.
 */
const WIDTH_KEY = 'prisme.navigator.width'

/**
 * Delay between the last scroll of a tree and its save, in milliseconds:
 * a scroll moves at every frame.
 */
const SCROLL_SAVE_DELAY = 300

/**
 * Key of the layout of an editor's navigator in local storage.
 *
 * @param editorId Id of the editor.
 */
export function navigatorLayoutKey(editorId: number): string {
  return `prisme.navigator.layout.${editorId}`
}

/**
 * Key of the tree of an editor's navigator in local storage.
 *
 * @param editorId Id of the editor.
 */
export function navigatorTreeKey(editorId: number): string {
  return `prisme.navigator.tree.${editorId}`
}

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
   * Was the tree kept from an earlier visit? A new tree opens the source of
   * its editor by itself; a kept one stays as the user left it.
   */
  known: boolean
}

/**
 * Navigator of an editor: its layout, its tree, and the scope of the
 * watchers that save them.
 */
interface NavigatorEntry {
  layout: NavigatorLayout
  tree: NavigatorTree
  scope: EffectScope
  timer: ReturnType<typeof setTimeout> | undefined
}

/**
 * Tree kept for an editor, `null` if none is readable.
 *
 * @param editorId Id of the editor.
 */
function storedTree(editorId: number): Pick<NavigatorTree, 'expanded' | 'scrollTop'> | null {
  try {
    const saved: unknown = JSON.parse(window.localStorage.getItem(navigatorTreeKey(editorId)) ?? '')
    if (typeof saved !== 'object' || saved === null) {
      return null
    }
    const { expanded, scrollTop } = saved as Record<string, unknown>
    if (
      !Array.isArray(expanded) ||
      !expanded.every((key) => typeof key === 'string') ||
      typeof scrollTop !== 'number' ||
      !Number.isFinite(scrollTop) ||
      scrollTop < 0
    ) {
      return null
    }
    return { expanded: expanded as string[], scrollTop }
  } catch {
    // Storage unavailable, nothing kept, or unreadable: none.
    return null
  }
}

/**
 * Keeps the tree of an editor's navigator; no effect if storage is
 * unavailable.
 *
 * @param editorId Id of the editor.
 * @param tree Tree to keep.
 */
function saveTree(editorId: number, tree: NavigatorTree): void {
  try {
    window.localStorage.setItem(
      navigatorTreeKey(editorId),
      JSON.stringify({ expanded: tree.expanded, scrollTop: tree.scrollTop }),
    )
  } catch {
    // Display preference only: losing it harms nothing.
  }
}

export const useNavigatorStore = defineStore('navigator', () => {
  /**
   * Navigators read so far, by editor.
   */
  const entries = new Map<number, NavigatorEntry>()

  /**
   * Navigator of an editor, read from local storage the first time.
   *
   * @param editorId Id of the editor.
   */
  function entryOf(editorId: number): NavigatorEntry {
    const known = entries.get(editorId)
    if (known) {
      return known
    }
    const scope = effectScope(true)
    const kept = storedTree(editorId)
    const entry = scope.run(() => {
      // Kept from its first reading: it no longer follows the last layout given.
      const layout = useStoredLayout<NavigatorLayout>(
        navigatorLayoutKey(editorId),
        { open: loadFlag(OPEN_KEY, true), width: loadSize(WIDTH_KEY) },
        true,
      )
      // The last layout given: the start of the next editor without its own.
      watch(
        () => layout.open,
        (open) => saveFlag(OPEN_KEY, open),
      )
      watch(
        () => layout.width,
        (width) => saveSize(WIDTH_KEY, width),
      )
      const tree = reactive<NavigatorTree>({
        expanded: kept?.expanded ?? [SOURCES_KEY],
        scrollTop: kept?.scrollTop ?? 0,
        known: kept !== null,
      })
      watch(
        () => tree.expanded,
        () => saveTree(editorId, tree),
      )
      return { layout, tree, scope, timer: undefined }
    })!
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
   * Notes the scroll of an editor's tree; it is kept shortly after the last
   * move.
   *
   * @param editorId Id of the editor.
   * @param scrollTop Position of the scroll, in CSS pixels.
   */
  function scrollTo(editorId: number, scrollTop: number): void {
    const entry = entryOf(editorId)
    entry.tree.scrollTop = scrollTop
    clearTimeout(entry.timer)
    entry.timer = setTimeout(() => saveTree(editorId, entry.tree), SCROLL_SAVE_DELAY)
  }

  /**
   * Forgets the navigator of a closed editor: a reopened editor starts from
   * the last layout given, with a new tree.
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
    forgetStoredLayout(navigatorLayoutKey(editorId))
    try {
      window.localStorage.removeItem(navigatorTreeKey(editorId))
    } catch {
      // Storage unavailable: nothing was kept.
    }
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

  return { layoutOf, treeOf, scrollTo, forget, forgetSource }
})
