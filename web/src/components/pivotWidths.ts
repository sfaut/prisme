/**
 * Widths of the columns of the pivot table (`PivotGrid.vue`).
 *
 * A column of values takes a default width fitted to its texts, or the width given by the user,
 * both in whole steps of the spacing scale (`--spacing`); a given width is saved with the tab. The
 * default width follows the longest text of the column, its values or its header, as the result
 * grid estimates its own (`estimatedWidth`): the engine gives the lengths of these texts with the
 * table (`text_lengths` of the module), over all its lines, so that a column keeps its width
 * whatever the scroll position. A given width names its column by the values of its node and its
 * measure, as a sort does: the engine gives the position of each one in the table (`positions` of
 * `PivotTable`). The steps before each column (`ColumnGeometry`) turn the scroll position into the
 * columns to draw (`columnRange`), as `drawnRange` does for equal widths.
 */

import type { PivotColumnWidth } from '@/api/generated/PivotColumnWidth'

import type { PivotTextLengths, PivotValueTarget } from './pivotEngine'
import { SORT_ROOM } from './resultGrid'

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
   * Width of each column, in steps of the spacing scale.
   */
  widths: number[]
  /**
   * Steps before each column, then those of all the columns: one number more than the columns.
   */
  starts: number[]
  /**
   * Width of a step of the spacing scale, in pixels.
   */
  step: number
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
 * Geometry of the columns of a table: their default widths, those given in their place.
 *
 * @param defaults Default width of each column, in steps.
 * @param widened Columns given a width; one beyond the table is left out.
 * @param step Width of a step, in pixels.
 */
export function columnGeometry(
  defaults: number[],
  widened: WidenedColumn[],
  step: number,
): ColumnGeometry {
  const widths = [...defaults]
  for (const { index, width } of widened) {
    if (index < widths.length) {
      widths[index] = width
    }
  }
  const starts = [0]
  let start = 0
  for (const width of widths) {
    start += width
    starts.push(start)
  }
  return { widths, starts, step }
}

/**
 * Steps taken by the columns before a position.
 *
 * @param geometry Columns of the table.
 * @param index Position of a column, bounded by the table: the number of columns gives the steps
 *     of them all.
 */
export function stepsBefore({ starts }: ColumnGeometry, index: number): number {
  return starts[Math.min(Math.max(0, index), starts.length - 1)] ?? 0
}

/**
 * Distance of a column from the start of the first one, in pixels.
 *
 * @param geometry Columns of the table.
 * @param index Position of the column, bounded by the table.
 */
export function columnOffset(geometry: ColumnGeometry, index: number): number {
  return stepsBefore(geometry, index) * geometry.step
}

/**
 * Position of the column at a distance from the start of the first one: 0 before it, the number
 * of columns beyond the last one.
 *
 * @param geometry Columns of the table.
 * @param distance Distance, in pixels.
 */
export function columnAt({ starts, step }: ColumnGeometry, distance: number): number {
  // The last column that starts at the distance or before it.
  let low = 0
  let high = starts.length - 1
  while (low < high) {
    const middle = Math.ceil((low + high) / 2)
    if ((starts[middle] ?? 0) * step <= distance) {
      low = middle
    } else {
      high = middle - 1
    }
  }
  return low
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
  const count = geometry.widths.length
  const edge = scroll + viewport
  const last = columnAt(geometry, edge)
  const reached = columnOffset(geometry, last) < edge ? last + 1 : last
  const end = Math.min(count, reached + overscan)
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
 * Tracks of the columns of a window: the width of each one.
 *
 * @param geometry Columns of the table.
 * @param first Position of the first column of the window.
 * @param count Number of columns of the window.
 */
export function windowTracks(geometry: ColumnGeometry, first: number, count: number): string[] {
  return geometry.widths.slice(first, first + count).map(stepsLength)
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
 * Room of the texts of a column, to estimate its width from a number of characters, in pixels.
 */
export interface TextRoom {
  /**
   * Average width of a character.
   */
  character: number
  /**
   * Room beside the text.
   */
  padding: number
}

/**
 * Default width of each column of values: that of its longest text, among its values and its
 * header, the latter with the room of its sort button, as the result grid estimates a width
 * (`estimatedWidth`), within the bounds.
 *
 * @param count Number of columns of the table.
 * @param lengths Lengths of the texts of the columns, in characters, from the engine.
 * @param measureLabels
 *     Label of each measure when the headers hold a row of measures (`hasMeasureRow`), which then
 *     heads each column; `null` otherwise, the last level of the columns heading it.
 * @param room Room of the characters and beside the text.
 * @param bounds Bounds of the width; the largest one caps the estimate.
 * @returns Width of each column, in steps.
 */
export function defaultSteps(
  count: number,
  lengths: PivotTextLengths,
  measureLabels: string[] | null,
  { character, padding }: TextRoom,
  { step, min, max }: WidthBounds,
): number[] {
  return Array.from({ length: count }, (_, column) => {
    const header =
      measureLabels === null || measureLabels.length === 0
        ? (lengths.labels[column] ?? 0)
        : (measureLabels[column % measureLabels.length]?.length ?? 0)
    const characters = Math.max(lengths.values[column] ?? 0, header + SORT_ROOM)
    return Math.min(max, Math.max(min, Math.ceil((characters * character + padding) / step)))
  })
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
