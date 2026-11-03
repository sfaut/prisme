/**
 * Widths of the columns of the pivot table (`PivotGrid.vue`).
 *
 * A column of values takes the default width of the theme (`--pivot-column-width`), or the width
 * given by the user, in whole steps of the spacing scale (`--spacing`), saved with the tab. A width
 * names its column by the values of its node and its measure, as a sort does: the engine gives the
 * position of each one in the table (`positions` of `PivotTable`), and the grid places the columns
 * out of sight after them. The table being windowed, the room of the columns before a position
 * (`roomBefore`) and the column at a distance (`columnAt`) skip the widened columns one by one,
 * the others counting by the default width: the scroll position turns into the columns to draw
 * (`columnRange`), as `drawnRange` does for equal widths.
 */

import type { PivotColumnWidth } from '@/api/generated/PivotColumnWidth'

import type { PivotValueTarget } from './pivotEngine'

/**
 * Column of values given a width, by its position in the table.
 */
export interface WidenedColumn {
  /**
   * Position of the column in the table.
   */
  index: number
  /**
   * Width, in steps of the spacing scale.
   */
  width: number
}

/**
 * Columns of values of a table, as the scroll position reads them.
 */
export interface ColumnGeometry {
  /**
   * Number of columns.
   */
  count: number
  /**
   * Default width of a column, in pixels.
   */
  standard: number
  /**
   * Width of a step of the spacing scale, in pixels.
   */
  step: number
  /**
   * Columns given a width, in the order of the table.
   */
  widened: WidenedColumn[]
}

/**
 * Room taken by columns: a number of columns of the default width, and the steps of the widened
 * ones.
 */
export interface ColumnRoom {
  /**
   * Columns of the default width.
   */
  standard: number
  /**
   * Steps of the spacing scale taken by the widened columns.
   */
  steps: number
}

/**
 * Key of a column named by its values: the same for the same path and the same measure.
 *
 * @param target Values of the node of the column, and its measure.
 */
export function targetKey({ path, measure }: PivotValueTarget): string {
  return JSON.stringify([measure, path])
}

/**
 * Columns given a width that the table shows, by their position, in the order of the table.
 *
 * @param widths Widths of the configuration.
 * @param positions Position in the table of each column known, by its key (`targetKey`).
 */
export function widenedColumns(
  widths: PivotColumnWidth[],
  positions: ReadonlyMap<string, number>,
): WidenedColumn[] {
  const byIndex = new Map<number, number>()
  for (const { path, measure, width } of widths) {
    const index = positions.get(targetKey({ path, measure }))
    if (index !== undefined) {
      byIndex.set(index, width)
    }
  }
  return [...byIndex].map(([index, width]) => ({ index, width })).sort((a, b) => a.index - b.index)
}

/**
 * Room taken by the columns before a position.
 *
 * @param geometry Columns of the table.
 * @param index Position of a column; beyond the table, the columns after it count by the default
 *     width.
 */
export function roomBefore({ widened }: ColumnGeometry, index: number): ColumnRoom {
  let standard = index
  let steps = 0
  for (const column of widened) {
    if (column.index >= index) {
      break
    }
    standard--
    steps += column.width
  }
  return { standard, steps }
}

/**
 * Distance of a column from the start of the first one, in pixels.
 *
 * @param geometry Columns of the table.
 * @param index Position of the column.
 */
export function columnOffset(geometry: ColumnGeometry, index: number): number {
  const { standard, steps } = roomBefore(geometry, index)
  return standard * geometry.standard + steps * geometry.step
}

/**
 * Position of the column at a distance from the start of the first one, as if the columns of the
 * default width went on at both ends of the table.
 *
 * @param geometry Columns of the table.
 * @param distance Distance, in pixels.
 */
export function columnAt(geometry: ColumnGeometry, distance: number): number {
  let start = 0
  let index = 0
  for (const column of geometry.widened) {
    const before = (column.index - index) * geometry.standard
    if (distance < start + before) {
      break
    }
    start += before
    const width = column.width * geometry.step
    if (distance < start + width) {
      return column.index
    }
    start += width
    index = column.index + 1
  }
  return index + Math.floor((distance - start) / geometry.standard)
}

/**
 * Columns to draw after the scroll position: those of the visible area, and `overscan` columns
 * beyond each edge.
 *
 * @param geometry Columns of the table.
 * @param scroll Distance of the visible area from the start of the first column, in pixels.
 * @param viewport Width of the visible area, in pixels.
 * @param overscan Columns drawn beyond each edge of the visible area.
 * @returns
 *     Position of the first column to draw, and position after the last one; both are equal when
 *     there is nothing to draw.
 */
export function columnRange(
  geometry: ColumnGeometry,
  scroll: number,
  viewport: number,
  overscan: number,
): { start: number; end: number } {
  const edge = scroll + viewport
  const last = columnAt(geometry, edge)
  const reached = columnOffset(geometry, last) < edge ? last + 1 : last
  const end = Math.min(geometry.count, reached + overscan)
  const start = Math.min(end, Math.max(0, columnAt(geometry, scroll) - overscan))
  return { start, end }
}

/**
 * CSS length of a number of steps of the spacing scale.
 *
 * @param width Number of steps.
 */
export function stepsLength(width: number): string {
  return `calc(var(--spacing) * ${width})`
}

/**
 * CSS length of the room taken by columns.
 *
 * @param room Room of the columns.
 */
export function roomLength({ standard, steps }: ColumnRoom): string {
  return `calc(var(--pivot-column-width) * ${standard} + var(--spacing) * ${steps})`
}

/**
 * Tracks of the columns of a window: the default width, gathered by `repeat()`, or the width of a
 * widened column.
 *
 * @param widened Columns given a width.
 * @param first Position of the first column of the window.
 * @param count Number of columns of the window.
 */
export function windowTracks(widened: WidenedColumn[], first: number, count: number): string[] {
  const widths = new Map(widened.map(({ index, width }) => [index, width]))
  const tracks: string[] = []
  let plain = 0
  const flush = () => {
    if (plain > 0) {
      tracks.push(`repeat(${plain}, var(--pivot-column-width))`)
      plain = 0
    }
  }
  for (let index = first; index < first + count; index++) {
    const width = widths.get(index)
    if (width === undefined) {
      plain++
    } else {
      flush()
      tracks.push(stepsLength(width))
    }
  }
  flush()
  return tracks
}

/**
 * Bounds of a width, in steps of the spacing scale.
 */
export interface WidthBounds {
  /**
   * Width of a step, in pixels.
   */
  step: number
  /**
   * Smallest width, in steps.
   */
  min: number
  /**
   * Largest width fitted to the content, in steps.
   */
  max: number
}

/**
 * Width reached by dragging the edge of a column: the nearest step, not below the smallest width.
 *
 * @param start Width of the column at the start of the drag, in pixels.
 * @param delta Horizontal move of the pointer, in pixels.
 * @param bounds Bounds of the width; the largest one only caps a fitted width.
 */
export function draggedSteps(start: number, delta: number, { step, min }: WidthBounds): number {
  return Math.max(min, Math.round((start + delta) / step))
}

/**
 * Width that lets each text of a column show whole: the steps that hold the widest one, within
 * the bounds.
 *
 * @param needs Width each element of the column needs, in pixels; none for an empty column.
 * @param bounds Bounds of the width.
 */
export function fittedSteps(needs: number[], { step, min, max }: WidthBounds): number {
  const widest = Math.max(0, ...needs)
  return Math.min(max, Math.max(min, Math.ceil(widest / step)))
}
