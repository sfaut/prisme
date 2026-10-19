/**
 * Data explorer: the panel on the right (`DataExplorer.vue`) that shows the
 * JSON or XML values selected in a result grid, indented and foldable.
 *
 * The panel follows the grid on screen, that of the active result tab of the
 * active editor: this grid publishes its selection here (`show`), and
 * withdraws it when it leaves the screen (`release`). The panel is closed
 * when the application starts; while it is closed, the grids publish
 * nothing.
 */

import { defineStore } from 'pinia'
import { ref, shallowRef } from 'vue'

import type { DataCell } from '@/components/dataFormat'

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

export const useDataExplorerStore = defineStore('dataExplorer', () => {
  /**
   * Is the panel open?
   */
  const open = ref(false)

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

  return { open, content, show, release }
})
