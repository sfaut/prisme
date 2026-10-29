/**
 * CSV for Excel in French: the format of the "Télécharger en CSV" menu.
 *
 * Excel in French opens it directly with a double click: UTF-8 with a BOM,
 * `;` as separator, CRLF line endings, a first line with the column names.
 * The fields follow RFC 4180: a field that holds `;`, `"`, CR or LF is put
 * between `"`, its `"` doubled. NULL is an empty field; an empty string is
 * written `""`, so that it stays told apart from NULL when read back (as
 * PostgreSQL's `COPY ... CSV` does), Excel showing both as an empty cell.
 * The numbers of the `decimal` and `float` families take the decimal comma,
 * which Excel in French reads as numbers (with a dot, `1.5` becomes the 1st
 * of May); every other value goes as the server sent it.
 *
 * The server writes the same format for the export of a full result
 * (`crates/server/src/exports/csv.rs`): the reference files of
 * `tests/fixtures/export/csv-excel-fr/` keep both identical.
 */

import type { ColumnTypeInfo } from '@/api/generated/ColumnTypeInfo'
import type { TypeFamily } from '@/api/generated/TypeFamily'
import type { Cell } from '@/api/querySession'

import { type GridFrame, frameCell } from './resultGrid'

/**
 * Media type of the file.
 */
export const CSV_TYPE = 'text/csv;charset=utf-8'

/**
 * Byte order mark, by which Excel recognises UTF-8.
 */
const BOM = '﻿'

/**
 * Separator of the fields.
 */
const SEPARATOR = ';'

/**
 * End of a line, RFC 4180's.
 */
const LINE_END = '\r\n'

/**
 * Characters that require putting a field between quotes.
 */
const NEEDS_QUOTES = /[;"\r\n]/

/**
 * Rows written between two pauses, during which the browser handles the
 * user's actions: a large result does not freeze the page.
 */
const CHUNK = 2000

/**
 * Field of a line, between quotes when needed.
 *
 * @param value Text of the field.
 */
export function csvField(value: string): string {
  return NEEDS_QUOTES.test(value) ? `"${value.replace(/"/g, '""')}"` : value
}

/**
 * Field of a cell.
 *
 * @param cell Value, `null` for NULL, `undefined` for an unselected cell.
 * @param family Family of the column's type, absent if unknown.
 */
export function csvCell(cell: Cell | undefined, family: TypeFamily | undefined): string {
  if (cell === undefined || cell === null) {
    return ''
  }
  if (cell === '') {
    return '""'
  }
  return csvField(family === 'decimal' || family === 'float' ? cell.replace('.', ',') : cell)
}

/**
 * Names given to several columns: the export is refused rather than producing
 * columns that cannot be told apart (`docs/IDEAS-EXPORTS.md`, section 2).
 *
 * @param names Names of the exported columns.
 * @returns Each duplicate name once, in the order of the columns.
 */
export function duplicateNames(names: string[]): string[] {
  const seen = new Set<string>()
  const duplicates = new Set<string>()
  for (const name of names) {
    if (seen.has(name)) {
      duplicates.add(name)
    }
    seen.add(name)
  }
  return [...duplicates]
}

/**
 * Lets the browser handle the waiting events before the next chunk. A timer
 * keeps going when the tab is hidden, at a slower pace, where a frame of the
 * display would wait for its return.
 */
function pause(): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve))
}

/**
 * CSV file of a frame of the grid, written by chunks of rows.
 *
 * @param frame Rows and columns to write, in the displayed order.
 * @param rows Rows of the result.
 * @param columns Names of the columns.
 * @param types Type of each column; empty if unknown.
 */
export async function csvBlob(
  frame: GridFrame,
  rows: Cell[][],
  columns: string[],
  types: ColumnTypeInfo[],
): Promise<Blob> {
  const families = frame.fields.map((field) => types[field]?.family)
  const header = frame.fields.map((field) => csvField(columns[field] ?? '')).join(SEPARATOR)
  const parts = [`${BOM}${header}${LINE_END}`]
  for (let start = 0; start < frame.rows.length; start += CHUNK) {
    const end = Math.min(frame.rows.length, start + CHUNK)
    let text = ''
    for (let row = start; row < end; row++) {
      const fields = families.map((family, column) =>
        csvCell(frameCell(frame, rows, row, column), family),
      )
      text += `${fields.join(SEPARATOR)}${LINE_END}`
    }
    parts.push(text)
    if (end < frame.rows.length) {
      await pause()
    }
  }
  return new Blob(parts, { type: CSV_TYPE })
}
