/**
 * Logic of the result grid: sorting, filtering, selection and copy.
 *
 * These functions do not depend on the display (see `ResultGrid.vue`). The
 * rows of the result are never copied: the sorted and filtered view is a
 * list of indexes into the original array.
 *
 * Vocabulary: a "view row" is a position in the displayed grid (after
 * sorting and filtering); a "source row" is a position in the result
 * received from the server.
 */

import type { ColumnTypeInfo } from '@/api/generated/ColumnTypeInfo'
import type { Cell } from '@/api/querySession'

/**
 * Direction of the sort.
 */
export type SortDirection = 'asc' | 'desc'

/**
 * Sort applied to the grid.
 */
export interface SortState {
  /**
   * Index of the sorted column.
   */
  column: number
  /**
   * Direction of the sort.
   */
  direction: SortDirection
}

/**
 * User filter of the grid. Two forms: the global filter searches the text in
 * all the columns (`column` is `null`); the targeted filter searches it in
 * the column at the index `column` of the `SELECT`.
 */
export interface UserFilter {
  /**
   * Searched text, case-insensitive. Empty: no text to search.
   */
  text: string
  /**
   * Index of the targeted column, or `null` for the global filter.
   */
  column: number | null
}

/**
 * Rectangular range of cells, in view rows, bounds included.
 */
export interface CellRange {
  top: number
  bottom: number
  left: number
  right: number
}

/**
 * Position of a cell: view row and column.
 */
export interface CellPosition {
  row: number
  column: number
}

/**
 * Selection: one or more ranges, and the starting cell of an extension (Shift+click).
 */
export interface GridSelection {
  ranges: CellRange[]
  anchor: CellPosition | null
}

/**
 * How a click changes the selection.
 *
 * - `replace`: simple click, the selection is replaced;
 * - `extend`: Shift+click, the last range extends up to the click;
 * - `add`: Ctrl+click, a range is added to the previous ones.
 */
export type SelectMode = 'replace' | 'extend' | 'add'

/**
 * Text shown for NULL in the grid.
 */
export const NULL_TEXT = 'NULL'

/**
 * Minimum width of a resized column, in pixels: enough to keep the sort
 * button and the start of the name.
 */
export const MIN_COLUMN_WIDTH = 40

/**
 * Tells whether a column is fixed-length text (`CHAR`), shown in monospace:
 * these are usually codes or references, more readable character by
 * character. `VARCHAR` and `TEXT` keep the regular font.
 *
 * @param type Type of the column, absent if it is unknown.
 */
export function isFixedLengthText(type: ColumnTypeInfo | undefined): boolean {
  return type?.family === 'text' && /^CHAR\b/.test(type.name)
}

/**
 * Display order of the columns after a move.
 *
 * @param order Current order: index in the `SELECT` of each displayed column.
 * @param from Displayed position of the moved column.
 * @param to
 *     Insertion position, counted before removing the column (0: far left,
 *     `order.length`: far right).
 * @returns The new order, and the new position of the moved column.
 */
export function moveColumn(
  order: number[],
  from: number,
  to: number,
): { order: number[]; position: number } {
  const moved = order[from]
  if (moved === undefined) {
    return { order, position: from }
  }
  const position = to > from ? to - 1 : to
  const result = order.filter((_, index) => index !== from)
  result.splice(position, 0, moved)
  return { order: result, position }
}

/**
 * Distance to travel before a press on a column name (or on a result tab)
 * becomes a move, in pixels: a slightly shaky click stays a click.
 */
export const MOVE_THRESHOLD = 5

/**
 * Insertion position of a column dropped at the abscissa `x`: before the
 * first column whose middle is to the right of the pointer.
 *
 * @param centers Abscissa of the middle of each displayed header, from left to right.
 * @param x Abscissa of the pointer.
 */
export function dropPosition(centers: number[], x: number): number {
  const index = centers.findIndex((center) => x < center)
  return index < 0 ? centers.length : index
}

/**
 * Width of a character, in pixels, to estimate the width of the columns.
 */
export const CHAR_WIDTH = 7

/**
 * Room added beside a text, in pixels, so that it does not touch the edges of
 * its cell.
 */
export const CELL_PADDING = 16

/**
 * Maximum estimated width of a column, in pixels.
 */
export const MAX_ESTIMATED_WIDTH = 360

/**
 * Maximum width of a column fitted to its content (double click on the edge
 * of its header), in pixels.
 */
export const MAX_FITTED_WIDTH = 720

/**
 * Number of rows read to estimate the width of the columns.
 */
const WIDTH_SAMPLE = 200

/**
 * Width of a column after its longest text: header (plus the room of the
 * sort button) and given rows.
 *
 * @param name Name of the column.
 * @param rows Rows to measure.
 * @param column Index of the column.
 * @param max Maximum width, in pixels.
 */
function contentWidth(name: string, rows: Cell[][], column: number, max: number): number {
  let longest = name.length + 2
  for (const row of rows) {
    longest = Math.max(longest, (row[column] ?? NULL_TEXT).length)
  }
  return Math.min(max, Math.max(48, longest * CHAR_WIDTH + CELL_PADDING))
}

/**
 * Estimated width of a column, on the header and the first rows, capped so
 * that a single column does not take the whole screen.
 *
 * @param name Name of the column.
 * @param rows Rows of the result.
 * @param column Index of the column.
 */
export function estimatedWidth(name: string, rows: Cell[][], column: number): number {
  return contentWidth(name, rows.slice(0, WIDTH_SAMPLE), column, MAX_ESTIMATED_WIDTH)
}

/**
 * Width fitted to the content of a column: its longest value over all the
 * received rows, with a higher cap than the estimate.
 *
 * @param name Name of the column.
 * @param rows Rows of the result.
 * @param column Index of the column.
 */
export function fittedWidth(name: string, rows: Cell[][], column: number): number {
  return contentWidth(name, rows, column, MAX_FITTED_WIDTH)
}

/**
 * Width of a column after dragging its right edge.
 *
 * @param start Width at the start of the drag, in pixels.
 * @param delta Horizontal move of the pointer, in pixels.
 */
export function resizedWidth(start: number, delta: number): number {
  return Math.max(MIN_COLUMN_WIDTH, Math.round(start + delta))
}

/**
 * Pattern of a decimal number, to sort numbers by value.
 */
const NUMBER = /^-?\d+(\.\d+)?([eE][-+]?\d+)?$/

/**
 * Collator of the text sort, created once: `localeCompare` with options builds
 * a new collator on each call, which makes a sort of 100,000 rows ten times
 * slower.
 */
const TEXT_COLLATOR = new Intl.Collator(undefined, { numeric: true })

/**
 * Compares two cells for the ascending sort.
 *
 * NULL comes first, as in MySQL and MariaDB. Two numbers compare by value,
 * the rest as text, taking digits into account ("a2" before "a10").
 *
 * @param a First cell.
 * @param b Second cell.
 */
export function compareCells(a: Cell, b: Cell): number {
  if (a === null || b === null) {
    return a === b ? 0 : a === null ? -1 : 1
  }
  if (NUMBER.test(a) && NUMBER.test(b)) {
    return Number(a) - Number(b)
  }
  return TEXT_COLLATOR.compare(a, b)
}

/**
 * Computes the view: indexes of the source rows that pass the user filter, in the sort order.
 *
 * Without a sort, the order received from the server is kept. The sort is
 * stable: for equal values, the server's order is kept too.
 *
 * @param rows Rows of the result.
 * @param userFilter User filter.
 * @param sort Sort, `null` for none.
 */
export function visibleRows(
  rows: Cell[][],
  userFilter: UserFilter,
  sort: SortState | null,
): number[] {
  const needle = userFilter.text.trim().toLowerCase()
  const view: number[] = []
  rows.forEach((row, index) => {
    if (needle === '' || matches(row, needle, userFilter.column)) {
      view.push(index)
    }
  })
  if (sort) {
    const sign = sort.direction === 'asc' ? 1 : -1
    view.sort(
      (a, b) => sign * compareCells(rows[a]![sort.column] ?? null, rows[b]![sort.column] ?? null),
    )
  }
  return view
}

/**
 * View kept from one update to the next, so that the rows received since are
 * added to it instead of sorting all the rows again (see `updateView`).
 */
export interface ViewMemory {
  /**
   * Rows the view was computed on: a new array (a new result) starts again.
   */
  rows: Cell[][] | null
  /**
   * Searched text the view was computed with, in lower case.
   */
  needle: string
  /**
   * Targeted column the view was computed with, `null` for the global filter.
   */
  column: number | null
  /**
   * Sort the view was computed with, `null` for none.
   */
  sort: SortState | null
  /**
   * Number of source rows already examined.
   */
  scanned: number
  /**
   * View of the examined rows, in the sort order.
   */
  view: number[]
}

/**
 * Empty memory: the first update computes the whole view.
 */
export function createViewMemory(): ViewMemory {
  return { rows: null, needle: '', column: null, sort: null, scanned: 0, view: [] }
}

/**
 * Tells whether two results have the same columns, with the same names in the
 * same order.
 *
 * @param a Names of the columns of a result.
 * @param b Names of the columns of another result.
 */
export function sameColumns(a: string[], b: string[]): boolean {
  return a.length === b.length && a.every((name, index) => name === b[index])
}

/**
 * Updates the view after rows were received. Only the rows added since the
 * last update are filtered and sorted, then merged into the view: a sort of
 * 50 000 rows takes about 0.2 s, and the rows arrive by batches of 500.
 *
 * The result is the one `visibleRows` gives on the whole array, as long as
 * the comparison is consistent. A new array, a new user filter or a new sort
 * starts again from the first row.
 *
 * A call examines at most `chunk` rows, so that a large result can be viewed
 * over several calls, between two frames of the display. Until the last
 * call, the view covers only the rows examined so far (`memory.scanned`
 * tells how many).
 *
 * @param memory Memory of the previous update, updated in place.
 * @param rows Rows of the result.
 * @param userFilter User filter.
 * @param sort Sort, `null` for none.
 * @param chunk Maximum number of rows examined by this call.
 * @returns A new array each time, so that Vue sees the change.
 */
export function updateView(
  memory: ViewMemory,
  rows: Cell[][],
  userFilter: UserFilter,
  sort: SortState | null,
  chunk: number = Number.POSITIVE_INFINITY,
): number[] {
  const needle = userFilter.text.trim().toLowerCase()
  const sameQuery =
    memory.rows === rows &&
    memory.needle === needle &&
    memory.column === userFilter.column &&
    memory.sort?.column === sort?.column &&
    memory.sort?.direction === sort?.direction
  if (!sameQuery || rows.length < memory.scanned) {
    Object.assign(memory, { rows, needle, column: userFilter.column, sort, scanned: 0, view: [] })
  }

  const end = Math.min(rows.length, memory.scanned + chunk)
  const added: number[] = []
  for (let index = memory.scanned; index < end; index++) {
    if (needle === '' || matches(rows[index]!, needle, userFilter.column)) {
      added.push(index)
    }
  }
  memory.scanned = end

  if (sort) {
    const sign = sort.direction === 'asc' ? 1 : -1
    const order = (a: number, b: number) =>
      sign * compareCells(rows[a]![sort.column] ?? null, rows[b]![sort.column] ?? null)
    added.sort(order)
    memory.view = mergeSorted(memory.view, added, order)
  } else {
    for (const index of added) {
      memory.view.push(index)
    }
  }
  return memory.view.slice()
}

/**
 * Merges two lists sorted by `order`. On equal keys, the element of `first`
 * comes first: `first` holds the earlier rows, so the sort stays stable.
 *
 * @param first Sorted list of the earlier rows.
 * @param second Sorted list of the rows received since.
 * @param order Comparison of two source rows.
 */
function mergeSorted(
  first: number[],
  second: number[],
  order: (a: number, b: number) => number,
): number[] {
  const merged: number[] = []
  let i = 0
  let j = 0
  while (i < first.length && j < second.length) {
    if (order(second[j]!, first[i]!) < 0) {
      merged.push(second[j++]!)
    } else {
      merged.push(first[i++]!)
    }
  }
  for (; i < first.length; i++) {
    merged.push(first[i]!)
  }
  for (; j < second.length; j++) {
    merged.push(second[j]!)
  }
  return merged
}

/**
 * Tells whether a row contains the searched text, in one column or in all.
 *
 * @param row Row.
 * @param needle Searched text, in lower case.
 * @param column Column, or `null` for all.
 */
function matches(row: Cell[], needle: string, column: number | null): boolean {
  const cells = column === null ? row : [row[column] ?? null]
  return cells.some((cell) => (cell ?? NULL_TEXT).toLowerCase().includes(needle))
}

/**
 * Next sort after a click on a column header: ascending, then descending, then none.
 *
 * @param current Current sort.
 * @param column Clicked column.
 */
export function nextSort(current: SortState | null, column: number): SortState | null {
  if (current?.column !== column) {
    return { column, direction: 'asc' }
  }
  return current.direction === 'asc' ? { column, direction: 'desc' } : null
}

/**
 * Empty selection.
 */
export function emptySelection(): GridSelection {
  return { ranges: [], anchor: null }
}

/**
 * Applies a click on a target range, according to the mode.
 *
 * @param selection Current selection.
 * @param target Range aimed at by the click (a cell, a whole row or a whole column).
 * @param position Clicked cell, new starting point.
 * @param mode Simple click, Shift+click or Ctrl+click.
 * @param stretch Extends the range from the starting point to the target.
 */
function applySelection(
  selection: GridSelection,
  target: CellRange,
  position: CellPosition,
  mode: SelectMode,
  stretch: (anchor: CellPosition) => CellRange,
): GridSelection {
  if (mode === 'extend' && selection.anchor) {
    const ranges = selection.ranges.slice(0, -1)
    return { ranges: [...ranges, stretch(selection.anchor)], anchor: selection.anchor }
  }
  const ranges = mode === 'add' ? [...selection.ranges, target] : [target]
  return { ranges, anchor: position }
}

/**
 * Selects a cell.
 *
 * @param selection Current selection.
 * @param row View row.
 * @param column Column.
 * @param mode Simple click, Shift+click or Ctrl+click.
 */
export function selectCell(
  selection: GridSelection,
  row: number,
  column: number,
  mode: SelectMode,
): GridSelection {
  return applySelection(
    selection,
    { top: row, bottom: row, left: column, right: column },
    { row, column },
    mode,
    (anchor) => ({
      top: Math.min(anchor.row, row),
      bottom: Math.max(anchor.row, row),
      left: Math.min(anchor.column, column),
      right: Math.max(anchor.column, column),
    }),
  )
}

/**
 * Selects a whole row.
 *
 * @param selection Current selection.
 * @param row View row.
 * @param columnCount Number of columns.
 * @param mode Simple click, Shift+click or Ctrl+click.
 */
export function selectRow(
  selection: GridSelection,
  row: number,
  columnCount: number,
  mode: SelectMode,
): GridSelection {
  const right = columnCount - 1
  return applySelection(
    selection,
    { top: row, bottom: row, left: 0, right },
    { row, column: 0 },
    mode,
    (anchor) => ({
      top: Math.min(anchor.row, row),
      bottom: Math.max(anchor.row, row),
      left: 0,
      right,
    }),
  )
}

/**
 * Selects a whole column.
 *
 * @param selection Current selection.
 * @param column Column.
 * @param rowCount Number of view rows.
 * @param mode Simple click, Shift+click or Ctrl+click.
 */
export function selectColumn(
  selection: GridSelection,
  column: number,
  rowCount: number,
  mode: SelectMode,
): GridSelection {
  const bottom = rowCount - 1
  return applySelection(
    selection,
    { top: 0, bottom, left: column, right: column },
    { row: 0, column },
    mode,
    (anchor) => ({
      top: 0,
      bottom,
      left: Math.min(anchor.column, column),
      right: Math.max(anchor.column, column),
    }),
  )
}

/**
 * Selects the whole grid.
 *
 * @param rowCount Number of view rows.
 * @param columnCount Number of columns.
 */
export function selectAll(rowCount: number, columnCount: number): GridSelection {
  if (rowCount === 0 || columnCount === 0) {
    return emptySelection()
  }
  return {
    ranges: [{ top: 0, bottom: rowCount - 1, left: 0, right: columnCount - 1 }],
    anchor: { row: 0, column: 0 },
  }
}

/**
 * Tells whether a cell is selected.
 *
 * @param selection Selection.
 * @param row View row.
 * @param column Column.
 */
export function isSelected(selection: GridSelection, row: number, column: number): boolean {
  return selection.ranges.some(
    (range) =>
      row >= range.top && row <= range.bottom && column >= range.left && column <= range.right,
  )
}

/**
 * Raw copy of the selection: the values as they are, separated by tabs, one
 * line per row of the result, ready to paste into a spreadsheet.
 *
 * The text covers the rows and the columns touched by the selection; an
 * unselected cell inside this frame is copied empty. NULL is copied as an
 * empty cell. Quotes go through as they are; the control characters of a
 * value or of a column name are written out (`rawCell`).
 *
 * @param selection Selection.
 * @param view View: indexes of the source rows.
 * @param rows Rows of the result.
 * @param columns Names of the columns.
 * @param withHeaders Adds a first line with the names of the copied columns.
 * @param order
 *     Index in the `SELECT` of each displayed column, when the columns have
 *     been moved: the selection is on the displayed positions.
 */
export function selectionToText(
  selection: GridSelection,
  view: number[],
  rows: Cell[][],
  columns: string[],
  withHeaders: boolean,
  order: number[] = columns.map((_, index) => index),
): string {
  const rowIndexes = coveredIndexes(selection.ranges, 'top', 'bottom')
  const columnIndexes = coveredIndexes(selection.ranges, 'left', 'right')
  const lines = rowIndexes.map((row) =>
    columnIndexes
      .map((column) => {
        const source = view[row]
        const field = order[column] ?? column
        const cell = source === undefined ? null : (rows[source]?.[field] ?? null)
        return isSelected(selection, row, column) ? rawCell(cell ?? '') : ''
      })
      .join('\t'),
  )
  if (withHeaders && columnIndexes.length > 0) {
    lines.unshift(
      columnIndexes.map((column) => rawCell(columns[order[column] ?? column] ?? '')).join('\t'),
    )
  }
  return lines.join('\n')
}

/**
 * Indexes covered by at least one range, sorted, between the given bounds.
 *
 * @param ranges Ranges.
 * @param start Start bound (`top` or `left`).
 * @param end End bound (`bottom` or `right`).
 */
function coveredIndexes(
  ranges: CellRange[],
  start: 'top' | 'left',
  end: 'bottom' | 'right',
): number[] {
  const indexes = new Set<number>()
  for (const range of ranges) {
    for (let index = range[start]; index <= range[end]; index++) {
      indexes.add(index)
    }
  }
  return [...indexes].sort((a, b) => a - b)
}

/**
 * Literal spelling of the control characters that have a well-known one
 * (C escape sequences, ASCII table).
 */
const CONTROL_ESCAPES: Record<number, string> = {
  0x00: '\\0',
  0x07: '\\a',
  0x08: '\\b',
  0x09: '\\t',
  0x0a: '\\n',
  0x0b: '\\v',
  0x0c: '\\f',
  0x0d: '\\r',
  0x1b: '\\e',
}

/**
 * Value copied as it is, its control characters written out: a tab becomes
 * `\t`, a line break `\n`, etc. (`CONTROL_ESCAPES`); a control character
 * without a known spelling becomes `\xHH` (hexadecimal code). The copy thus
 * keeps one value per cell and one line per row of the result.
 *
 * The `\` already present stay as they are (decision of ticket #3): a `\t`
 * written in the value is no longer told apart from a tab. This copy is made
 * to be quick; an exact export will go through CSV or TSV.
 *
 * @param value Value to copy.
 */
function rawCell(value: string): string {
  let text = ''
  for (const character of value) {
    const code = character.codePointAt(0) ?? 0
    const control = code < 0x20 || (code >= 0x7f && code <= 0x9f)
    text += control
      ? (CONTROL_ESCAPES[code] ?? `\\x${code.toString(16).toUpperCase().padStart(2, '0')}`)
      : character
  }
  return text
}
