/**
 * Layout of the pivot table (`PivotGrid.vue`) from a window of the engine: the header cells of
 * the column levels and of the measures, and the label of each line.
 *
 * The columns of a window that share a node at a level share its header cell there; the total of
 * a node reads `Total` on the level after its path, down to the last level. A single measure adds
 * no level to the headers (F-28), unless there is no column level: its name then heads its
 * column. The corner names the levels: those of the columns each on its row, those of the lines
 * on the last row. The lines are in compact form (C-07): the subtotal of a group stands on the line of the
 * group, its label indented by its depth. The header of a single column names the values that can
 * rank the lines (`HeaderCell.target`, C-13).
 */

import type { Cell } from '@/api/querySession'

import type { PivotColumn, PivotLine, PivotShape, PivotValueTarget } from './pivotEngine'

/**
 * Label of a total, in the headers and on the line of the grand total.
 */
export const TOTAL_TEXT = 'Total'

/**
 * Header cell of the columns of a window.
 */
export interface HeaderCell {
  /**
   * Row of the headers, from 0.
   */
  row: number
  /**
   * Number of rows covered.
   */
  rowSpan: number
  /**
   * First column covered, by its position in the window.
   */
  column: number
  /**
   * Number of columns covered.
   */
  span: number
  /**
   * Text of the cell, `null` for NULL.
   */
  text: Cell
  /**
   * What the cell heads: a value of a level, a total, or a measure.
   */
  kind: 'value' | 'total' | 'measure'
  /**
   * Column whose values can rank the lines, when the cell heads a single one; `null` otherwise.
   */
  target: PivotValueTarget | null
}

/**
 * Do the headers hold a row of measures? With two measures or more, and with a single one when
 * no level of columns names its column.
 *
 * @param columnLevels Number of levels of the columns.
 * @param measures Number of measures.
 */
export function hasMeasureRow(columnLevels: number, measures: number): boolean {
  return measures >= 2 || (columnLevels === 0 && measures === 1)
}

/**
 * Number of rows of the headers: one per level of the columns, plus that of the measures; one at
 * least, for the corner.
 *
 * @param columnLevels Number of levels of the columns.
 * @param measures Number of measures.
 */
export function headerRowCount(columnLevels: number, measures: number): number {
  return Math.max(1, columnLevels + (hasMeasureRow(columnLevels, measures) ? 1 : 0))
}

/**
 * Values of a column of a table, as they can rank its lines; `null` without any measure.
 *
 * @param column Column of the table.
 */
function columnTarget(column: PivotColumn): PivotValueTarget | null {
  return column.measure === null
    ? null
    : { path: column.path.map(([, value]) => value), measure: column.measure }
}

/**
 * Header cells of the columns of a window.
 *
 * @param columns Columns of the window, in their order.
 * @param columnLevels Number of levels of the columns.
 * @param measureLabels Label of each measure, in the order of its zone.
 */
export function headerCells(
  columns: PivotColumn[],
  columnLevels: number,
  measureLabels: string[],
): HeaderCell[] {
  const cells: HeaderCell[] = []
  const measureRow = hasMeasureRow(columnLevels, measureLabels.length)
  for (let level = 0; level < columnLevels; level++) {
    // Cell being extended, and what it heads: a node, or the total of a node.
    let current: HeaderCell | null = null
    let currentKey = ''
    columns.forEach((column, index) => {
      const step = column.path[level]
      let cell: Omit<HeaderCell, 'column' | 'span'>
      let key: string
      if (step !== undefined) {
        const last = !measureRow && level === columnLevels - 1
        const target = last ? columnTarget(column) : null
        cell = { row: level, rowSpan: 1, text: step[1], kind: 'value', target }
        key = `value ${step[0]}`
      } else if (column.total && level === column.path.length) {
        cell = {
          row: level,
          rowSpan: columnLevels - level,
          text: TOTAL_TEXT,
          kind: 'total',
          target: measureRow ? null : columnTarget(column),
        }
        key = `total ${column.node}`
      } else {
        // Under the cell of its total.
        current = null
        return
      }
      if (current !== null && currentKey === key) {
        current.span++
      } else {
        current = { ...cell, column: index, span: 1 }
        currentKey = key
        cells.push(current)
      }
    })
  }
  if (measureRow) {
    columns.forEach((column, index) => {
      cells.push({
        row: columnLevels,
        rowSpan: 1,
        column: index,
        span: 1,
        text: column.measure === null ? '' : (measureLabels[column.measure] ?? ''),
        kind: 'measure',
        target: columnTarget(column),
      })
    })
  }
  return cells
}

/**
 * Level named in the corner of the headers.
 */
export interface CornerLevel {
  /**
   * Position of the level on its axis.
   */
  level: number
  /**
   * Name of the level.
   */
  name: string
}

/**
 * Row of the corner of the headers, on the left of a row of the headers.
 */
export interface CornerRow {
  /**
   * Levels of the lines, at the start of the last row, above their labels; none on the other rows.
   */
  rows: CornerLevel[]
  /**
   * Level of the columns of the row, at its end, against its values; `null` on the row of the
   * measures.
   */
  column: CornerLevel | null
}

/**
 * Rows of the corner of the headers: each level of the columns on its row, the levels of the lines
 * on the last row.
 *
 * @param rowCount Number of rows of the headers (`headerRowCount`).
 * @param rowNames Names of the levels of the lines, the outermost first.
 * @param columnNames Names of the levels of the columns, the outermost first.
 */
export function cornerRows(
  rowCount: number,
  rowNames: string[],
  columnNames: string[],
): CornerRow[] {
  return Array.from({ length: rowCount }, (_, row) => {
    const column = columnNames[row]
    return {
      rows: row === rowCount - 1 ? rowNames.map((name, level) => ({ level, name })) : [],
      column: column === undefined ? null : { level: row, name: column },
    }
  })
}

/**
 * Mark of a sort button, as in the grid.
 *
 * @param descending Direction of the sort, `null` when the button does not drive the order.
 */
export function sortMark(descending: boolean | null): string {
  if (descending === null) {
    return '⇅'
  }
  return descending ? '▼' : '▲'
}

/**
 * Do two tables take the same room? A window of one can stand in for a window of the other while
 * it is read: a sort does not blink.
 *
 * @param a Shape of a table.
 * @param b Shape of another table.
 */
export function sameLayout(a: PivotShape, b: PivotShape): boolean {
  return (
    a.lines === b.lines &&
    a.columns === b.columns &&
    a.rowLevels === b.rowLevels &&
    a.columnLevels === b.columnLevels
  )
}

/**
 * Label of a line: the value of its level, `Total` for the grand total.
 *
 * @param line Line of the table.
 */
export function lineLabel(line: PivotLine): Cell {
  return line.depth === 0 ? TOTAL_TEXT : line.label
}
