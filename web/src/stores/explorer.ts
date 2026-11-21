/**
 * Data explorers: the panels on the right of the editors (`ExplorerPanel.vue`)
 * that show the JSON or XML values selected in a result grid, indented and
 * foldable. Each editor has its own.
 *
 * The explorer of an editor follows its grid on screen, that of its active
 * result tab while the editor is active: this grid publishes its selection
 * here, for its editor (`show`), and withdraws it when it leaves the screen
 * (`release`). While the explorer of an editor is closed, its grids publish
 * nothing.
 *
 * The panel of the explorer of each editor (open or closed, its width) is
 * part of the display of the editor, saved with it on the server side
 * (`editorDisplays` store): closed by default, at the width of the theme.
 * The wrapping of long lines (`wrap`) is common to all the explorers, off by
 * default, saved with the display settings of the user (`userDisplay`
 * store).
 */

import { defineStore } from 'pinia'
import { computed, shallowReactive } from 'vue'

import type { Panel } from '@/api/generated/Panel'
import type { DataCell } from '@/components/explorer/dataFormat'
import { useEditorDisplaysStore } from '@/stores/editorDisplays'
import { useUserDisplayStore } from '@/stores/userDisplay'

/**
 * What the grid on screen gives the explorer.
 *
 * - `none`: nothing selected (or no grid on screen);
 * - `several`: a selection over several columns;
 * - `scalar`: values of a column that holds neither JSON nor XML;
 * - `values`: the selected cells of a JSON or XML column, from top to bottom.
 */
export type ExplorerContent =
  | { state: 'none' }
  | { state: 'several' }
  | { state: 'scalar'; column: string }
  | { state: 'values'; column: string; kind: 'json' | 'xml'; cells: DataCell[] }

/**
 * Tells whether two contents are the same, cell by cell: a grid publishes
 * again only what changed.
 *
 * @param a First content.
 * @param b Second content.
 */
export function sameContent(a: ExplorerContent, b: ExplorerContent): boolean {
  if (a.state === 'values' && b.state === 'values') {
    return (
      a.column === b.column &&
      a.kind === b.kind &&
      a.cells.length === b.cells.length &&
      a.cells.every(
        (cell, index) => cell.row === b.cells[index]!.row && cell.value === b.cells[index]!.value,
      )
    )
  }
  if (a.state === 'scalar' && b.state === 'scalar') {
    return a.column === b.column
  }
  return a.state === b.state && a.state !== 'values' && a.state !== 'scalar'
}

/**
 * Content of an explorer without a grid that publishes.
 */
const NOTHING: ExplorerContent = { state: 'none' }

export const useExplorerStore = defineStore('explorer', () => {
  /**
   * Are long lines wrapped to the width of the panel?
   */
  const user = useUserDisplayStore()
  const wrap = computed({
    get: () => user.display.explorer.wrap,
    set: (value: boolean) => (user.display.explorer.wrap = value),
  })

  const displays = useEditorDisplaysStore()

  /**
   * Panel of an editor's explorer: open or closed, and its width.
   *
   * @param editorId Id of the editor.
   */
  function layoutOf(editorId: number): Panel {
    return displays.displayOf(editorId).panels.explorer
  }

  /**
   * Content published for each editor by its grid on screen.
   */
  const contents = shallowReactive(new Map<number, ExplorerContent>())

  /**
   * Grid that published the content of each editor.
   */
  const owners = new Map<number, symbol>()

  /**
   * Content of an editor's explorer: what its grid on screen published.
   *
   * @param editorId Id of the editor.
   */
  function contentOf(editorId: number): ExplorerContent {
    return contents.get(editorId) ?? NOTHING
  }

  /**
   * Publishes the content of a grid on screen, for its editor.
   *
   * @param editorId Id of the grid's editor.
   * @param grid Identity of the grid.
   * @param next Its content.
   */
  function show(editorId: number, grid: symbol, next: ExplorerContent): void {
    owners.set(editorId, grid)
    if (!sameContent(contentOf(editorId), next)) {
      contents.set(editorId, next)
    }
  }

  /**
   * Withdraws the content of a grid that leaves the screen; no effect if
   * another grid of its editor published since.
   *
   * @param editorId Id of the grid's editor.
   * @param grid Identity of the grid.
   */
  function release(editorId: number, grid: symbol): void {
    if (owners.get(editorId) === grid) {
      owners.delete(editorId)
      contents.delete(editorId)
    }
  }

  /**
   * Forgets the content of the explorer of a closed editor; its panel stays
   * saved with the editor, which finds it again when reopened.
   *
   * @param editorId Id of the editor.
   */
  function forget(editorId: number): void {
    owners.delete(editorId)
    contents.delete(editorId)
  }

  return { wrap, layoutOf, contentOf, show, release, forget }
})
