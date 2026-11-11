/**
 * Data explorer: the panel on the right (`ExplorerPanel.vue`) that shows the
 * JSON or XML values selected in a result grid, indented and foldable.
 *
 * The panel follows the grid on screen, that of the active result tab of the
 * active editor: this grid publishes its selection here (`show`), and
 * withdraws it when it leaves the screen (`release`). The panel is closed
 * when the application starts; while it is closed, the grids publish
 * nothing.
 *
 * Its width and the wrapping of long lines (`wrap`) are display preferences
 * specific to this machine, kept in the browser's local storage
 * (`prisme.explorer.width`, `prisme.explorer.wrap`): the default width and no
 * wrapping by default, and when storage is unavailable.
 */

import { defineStore } from 'pinia'
import { ref, shallowRef, watch } from 'vue'

import type { DataCell } from '@/components/explorer/dataFormat'
import { loadFlag, loadSize, saveFlag, saveSize } from '@/utils/storedValues'

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
 * Key of the width of the panel in local storage.
 */
const WIDTH_KEY = 'prisme.explorer.width'

/**
 * Key of the wrapping preference in local storage.
 */
const WRAP_KEY = 'prisme.explorer.wrap'

export const useExplorerStore = defineStore('explorer', () => {
  /**
   * Is the panel open?
   */
  const open = ref(false)

  /**
   * Width given by the user, in CSS pixels; `null`: the default width of the
   * theme.
   */
  const width = ref(loadSize(WIDTH_KEY))
  watch(width, (value) => saveSize(WIDTH_KEY, value))

  /**
   * Are long lines wrapped to the width of the panel?
   */
  const wrap = ref(loadFlag(WRAP_KEY, false))
  watch(wrap, (value) => saveFlag(WRAP_KEY, value))

  /**
   * Content published by the grid on screen.
   */
  const content = shallowRef<ExplorerContent>({ state: 'none' })

  /**
   * Grid that published the content, `null` if none.
   */
  let owner: symbol | null = null

  /**
   * Publishes the content of a grid on screen.
   *
   * @param grid Identity of the grid.
   * @param next Its content.
   */
  function show(grid: symbol, next: ExplorerContent): void {
    owner = grid
    if (!sameContent(content.value, next)) {
      content.value = next
    }
  }

  /**
   * Withdraws the content of a grid that leaves the screen; no effect if
   * another grid published since.
   *
   * @param grid Identity of the grid.
   */
  function release(grid: symbol): void {
    if (owner === grid) {
      owner = null
      content.value = { state: 'none' }
    }
  }

  return { open, width, wrap, content, show, release }
})
