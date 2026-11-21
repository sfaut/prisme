/**
 * Object navigators, the panels on the left of the editors
 * (`NavigatorPanel.vue`): each editor has its own, with its panel (open or
 * closed, its width) and its tree (open nodes, scroll, "Tables" folders
 * sorted by row count), so that going from one editor to another finds each
 * navigator as it was left.
 *
 * Both are parts of the display of the editor (`editorDisplays` store),
 * saved with it on the server side: this store gives them to the navigator
 * and changes its tree.
 */

import { defineStore } from 'pinia'

import type { NavigatorTree } from '@/api/generated/NavigatorTree'
import type { Panel } from '@/api/generated/Panel'
import { belongsToSource } from '@/components/navigator/navigatorNodes'
import { useEditorDisplaysStore } from '@/stores/editorDisplays'

/**
 * Order of the tables of a "Tables" folder: by name, or by row count, the
 * largest first.
 */
export type TableSort = 'name' | 'rows'

export const useNavigatorStore = defineStore('navigator', () => {
  const displays = useEditorDisplaysStore()

  /**
   * Panel of an editor's navigator: open or closed, and its width.
   *
   * @param editorId Id of the editor.
   */
  function layoutOf(editorId: number): Panel {
    return displays.displayOf(editorId).panels.navigator
  }

  /**
   * Tree of an editor's navigator: open nodes, scroll, sorted folders.
   *
   * @param editorId Id of the editor.
   */
  function treeOf(editorId: number): NavigatorTree {
    return displays.displayOf(editorId).navigator
  }

  /**
   * Is the tree of an editor new? A new tree opens the source of its editor
   * by itself; a saved one stays as the user left it.
   *
   * @param editorId Id of the editor.
   */
  function isNew(editorId: number): boolean {
    return !displays.isSaved(editorId)
  }

  /**
   * Notes the scroll of an editor's tree; it is saved shortly after the last
   * move.
   *
   * @param editorId Id of the editor.
   * @param scrollTop Position of the scroll, in CSS pixels.
   */
  function scrollTo(editorId: number, scrollTop: number): void {
    treeOf(editorId).scroll_top = scrollTop
  }

  /**
   * Sorts the tables of a "Tables" folder of an editor's navigator.
   *
   * @param editorId Id of the editor.
   * @param folderKey Key of the "Tables" folder.
   * @param sort By name, or by row count.
   */
  function sortTables(editorId: number, folderKey: string, sort: TableSort): void {
    const tree = treeOf(editorId)
    if (tree.tables_by_rows.includes(folderKey) === (sort === 'rows')) {
      return
    }
    tree.tables_by_rows =
      sort === 'rows'
        ? [...tree.tables_by_rows, folderKey]
        : tree.tables_by_rows.filter((key) => key !== folderKey)
  }

  /**
   * Closes a deleted source, and the nodes under it, and forgets the sorts of
   * its "Tables" folders, in every navigator.
   *
   * @param connectionId Id of the source's connection.
   */
  function forgetSource(connectionId: number): void {
    for (const { navigator: tree } of displays.loaded()) {
      if (tree.expanded.some((key) => belongsToSource(key, connectionId))) {
        tree.expanded = tree.expanded.filter((key) => !belongsToSource(key, connectionId))
      }
      if (tree.tables_by_rows.some((key) => belongsToSource(key, connectionId))) {
        tree.tables_by_rows = tree.tables_by_rows.filter(
          (key) => !belongsToSource(key, connectionId),
        )
      }
    }
  }

  return { layoutOf, treeOf, isNew, scrollTo, sortTables, forgetSource }
})
