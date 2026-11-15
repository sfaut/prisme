/**
 * Selection of the result grid as a SQL `IN` clause, to filter a query on
 * the selected values: the distinct values of the selected cells, each a
 * literal of its column's type in the dialect of the editor's server, as the
 * CTE writes them (`cteLiteral`, `cte.ts`).
 *
 * One column gives its values on one line, several columns one row
 * constructor per line:
 *
 * ```sql
 * IN (1234, 2345, 3456)
 *
 * IN (
 *     ('val1', 1234),
 *     ('val2', 2345)
 * )
 * ```
 *
 * A value, or a row of values, met several times is written once, at its
 * first place. NULL never matches in an `IN`: a NULL of a single column is
 * left out, as the Pivoteur's lists write it apart; a row of several columns
 * that holds a NULL is kept as it is, so that no selected row goes missing.
 */

import type { ColumnTypeInfo } from '@/api/generated/ColumnTypeInfo'
import type { Cell } from '@/api/querySession'

import { visitByChunks } from './chunks'
import { type LiteralOptions, cteLiteral } from './cte'
import { type GridFrame, frameCell } from './resultGrid'

/**
 * How the clause is written: literals of the dialect, layout.
 */
export interface InClauseOptions extends LiteralOptions {
  /**
   * Width of an indentation level, in spaces.
   */
  indentSize: number
}

/**
 * `IN` clause of a frame of the grid, whose cells are all selected
 * (`selectionIsFull`). The values are read by chunks of rows (`chunks.ts`).
 *
 * @param frame Rows and columns of the selection, in the displayed order.
 * @param rows Rows of the result.
 * @param types Type of each column; empty if unknown.
 * @param options How the clause is written.
 * @returns The clause, `null` when no value is left (only NULL).
 */
export async function inClause(
  frame: GridFrame,
  rows: Cell[][],
  types: ColumnTypeInfo[],
  options: InClauseOptions,
): Promise<string | null> {
  const families = frame.fields.map((field) => types[field]?.family)
  const single = families.length === 1
  // A set keeps the order of first insertion.
  const values = new Set<string>()
  await visitByChunks(frame.rows.length, (row) => {
    const cells = families.map((_, column) => frameCell(frame, rows, row, column) ?? null)
    if (single && cells[0] === null) {
      return
    }
    const literals = cells.map((cell, column) => cteLiteral(cell, families[column], options))
    values.add(single ? literals[0]! : `(${literals.join(', ')})`)
  })
  if (values.size === 0) {
    return null
  }
  if (single) {
    return `IN (${[...values].join(', ')})`
  }
  const indent = ' '.repeat(options.indentSize)
  return `IN (\n${[...values].map((value) => `${indent}${value}`).join(',\n')}\n)`
}
