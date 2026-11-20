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
 * The layout of the explorer of each editor (open or closed, its width) is a
 * display preference specific to this machine, kept in the browser's local
 * storage (`prisme.explorer.layout.<editor>`), forgotten when the editor
 * closes (`forget`): closed by default, at the width of the theme, which an
 * editor without a layout of its own, a new one for example, takes; the
 * wrapping of long lines (`wrap`, `prisme.explorer.wrap`) is common to all
 * the explorers, off by default. When storage is unavailable, the defaults
 * apply.
 */

import { defineStore } from 'pinia'
import { type EffectScope, effectScope, ref, shallowReactive, watch } from 'vue'

import type { DataCell } from '@/components/explorer/dataFormat'
import { forgetStoredLayout, useStoredLayout } from '@/components/split/storedLayout'
import { loadFlag, saveFlag } from '@/utils/storedValues'

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
 * Key of the layout of an editor's explorer in local storage.
 *
 * @param editorId Id of the editor.
 */
export function explorerLayoutKey(editorId: number): string {
  return `prisme.explorer.layout.${editorId}`
}

/**
 * Layout of an editor's explorer.
 */
export type ExplorerLayout = {
  /**
   * Is the explorer open?
   */
  open: boolean
  /**
   * Width given by the user, in CSS pixels; `null`: the default width of the
   * theme.
   */
  width: number | null
}

/**
 * Content of an explorer without a grid that publishes.
 */
const NOTHING: ExplorerContent = { state: 'none' }

/**
 * Key of the wrapping preference in local storage.
 */
const WRAP_KEY = 'prisme.explorer.wrap'

export const useExplorerStore = defineStore('explorer', () => {
  /**
   * Are long lines wrapped to the width of the panel?
   */
  const wrap = ref(loadFlag(WRAP_KEY, false))
  watch(wrap, (value) => saveFlag(WRAP_KEY, value))

  /**
   * Layouts read so far, by editor, with the scope of the watchers that save
   * them.
   */
  const layouts = new Map<number, { layout: ExplorerLayout; scope: EffectScope }>()

  /**
   * Layout of an editor's explorer, read from local storage the first time.
   *
   * @param editorId Id of the editor.
   */
  function layoutOf(editorId: number): ExplorerLayout {
    const known = layouts.get(editorId)
    if (known) {
      return known.layout
    }
    const scope = effectScope(true)
    const layout = scope.run(() =>
      useStoredLayout<ExplorerLayout>(explorerLayoutKey(editorId), { open: false, width: null }),
    )!
    layouts.set(editorId, { layout, scope })
    return layout
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
   * Forgets the explorer of a closed editor: a reopened editor starts
   * closed, at the last width given.
   *
   * @param editorId Id of the editor.
   */
  function forget(editorId: number): void {
    layouts.get(editorId)?.scope.stop()
    layouts.delete(editorId)
    owners.delete(editorId)
    contents.delete(editorId)
    forgetStoredLayout(explorerLayoutKey(editorId))
  }

  return { wrap, layoutOf, contentOf, show, release, forget }
})
