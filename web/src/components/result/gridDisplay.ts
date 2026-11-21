/**
 * Display of the result grid of a tab, saved with the tab on the server side
 * (`GridDisplay`, `crates/server/src/grid_display.rs`): widths and order of
 * its columns, sort, scroll, user filter and its form.
 *
 * The grid (`ResultGrid.vue`) owns its layout (`GridLayout`: everything but
 * the filter), the tab owns the filter (`QueryTab.filter`, `filterOpen`);
 * the document saved joins the two (`gridDocument`), shortly after the last
 * change (`deferredSave`). The layout applies only to a result with the
 * columns it was made on (`startingLayout`): a new run carries it over to
 * the tab of the same query, whose columns may have changed.
 */
import { saveResultGrid } from '@/api/editors'
import type { GridDisplay } from '@/api/generated/GridDisplay'
import type { GridLayout, QueryTab } from '@/api/querySession'
import { deferredSave } from '@/utils/deferredSave'

import type { SortState } from './resultGrid'
import { sameColumns } from './resultGrid'

/**
 * State of a grid that opens on a result.
 */
export interface StartingLayout {
  /**
   * Width given to the columns, by index in the `SELECT`.
   */
  widths: Record<number, string>
  /**
   * Index in the `SELECT` of each column shown, from left to right.
   */
  order: number[]
  /**
   * Sort of the rows, `null` for the order of the result.
   */
  sort: SortState | null
  /**
   * Scroll to give back, in CSS pixels.
   */
  scroll: { top: number; left: number }
}

/**
 * State a grid opens with on a result: the saved layout if it was made on
 * the same columns, the defaults otherwise (widths estimated, order of the
 * `SELECT`, no sort, top left). A saved order that is not one of the
 * columns, or a sort on a column that does not exist, is dropped.
 *
 * @param layout Saved layout, `null` without one.
 * @param columns Names of the columns of the result.
 */
export function startingLayout(layout: GridLayout | null, columns: string[]): StartingLayout {
  const indexes = columns.map((_, index) => index)
  if (layout === null || !sameColumns(layout.columns, columns)) {
    return { widths: {}, order: indexes, sort: null, scroll: { top: 0, left: 0 } }
  }
  const widths: Record<number, string> = {}
  layout.widths.forEach((width, column) => {
    if (width !== null && column < columns.length) {
      widths[column] = width
    }
  })
  const sorted = [...layout.order].sort((a, b) => a - b)
  const order = sameColumns(sorted.map(String), indexes.map(String)) ? layout.order : indexes
  const sort = layout.sort !== null && layout.sort.column < columns.length ? layout.sort : null
  return { widths, order, sort, scroll: { top: layout.scroll_top, left: layout.scroll_left } }
}

/**
 * Display of the grid of a tab, as saved: its layout, or the defaults on the
 * columns of its result, with the filter of the tab.
 *
 * @param tab Result tab.
 */
export function gridDocument(tab: QueryTab): GridDisplay {
  const layout: GridLayout = tab.grid ?? {
    columns: tab.result?.columns ?? [],
    widths: [],
    order: [],
    sort: null,
    scroll_top: 0,
    scroll_left: 0,
  }
  return { ...layout, filter: { ...tab.filter }, filter_open: tab.filterOpen }
}

const saves = deferredSave<QueryTab>((tab, keepalive) =>
  saveResultGrid(tab.id, gridDocument(tab), { keepalive }),
)

/**
 * Saves the display of the grid of a tab, shortly after its last change.
 *
 * @param tab Result tab.
 */
export function scheduleGridSave(tab: QueryTab): void {
  saves.schedule(tab)
}

/**
 * Keeps the new layout of the grid of a tab, and saves it.
 *
 * @param tab Result tab.
 * @param layout New layout; `null` keeps none.
 */
export function keepGridLayout(tab: QueryTab, layout: GridLayout | null): void {
  tab.grid = layout
  scheduleGridSave(tab)
}
