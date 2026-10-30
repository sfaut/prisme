/**
 * Data as a CTE: the rows of a result written as a `WITH` query that gives
 * them back, to query them again on the same server or on another one
 * (`docs/IDEAS-EXPORTS.md`, section 4.5).
 *
 * ```sql
 * WITH ds (id, name, created_at) AS (
 *     VALUES
 *         ROW (1, 'Ada', TIMESTAMP'2026-10-12 09:32:00'),
 *         ROW (2, NULL, NULL)
 * )
 * SELECT ALL id, name, created_at
 * FROM ds
 * ;
 * ```
 *
 * MySQL writes each row `ROW (…)`, from 8.0.19; MariaDB writes it `(…)` and
 * refuses `ROW`. Each value is a literal of its column's type, so that the
 * columns of the CTE keep it (types measured on MySQL 8.0.19 to 9.3.0 and
 * MariaDB 10.6.3 to 11.8.9):
 *
 * - integers and decimal numbers as the server sent them; floating numbers
 *   with an exponent (`1.5E0`), which makes them `DOUBLE` instead of
 *   `DECIMAL`;
 * - dates, date-times and times as typed literals (`DATE'2026-10-12'`,
 *   `TIMESTAMP'2026-10-12 09:32:00'`, `TIME'12:00:00'`). MySQL refuses a
 *   typed date whose month or day is zero under its default modes
 *   (`NO_ZERO_DATE`, `NO_ZERO_IN_DATE`): such a date is written as a string
 *   for MySQL, and its column becomes text;
 * - JSON cast for MySQL (`CAST('…' AS JSON)`), a string for MariaDB, which
 *   has no JSON type;
 * - bytes in hexadecimal (`X'FF001A'`) when the server sent them so (`0x…`,
 *   bytes that are not UTF-8), otherwise as a binary string
 *   (`_binary'abc'`); bits as a bit-value literal (`b'101'`);
 * - every other value as a string.
 *
 * A value of a numeric column that does not read as a number is written as a
 * string, never as is. Strings double their quotes, and their backslashes
 * unless the `NO_BACKSLASH_ESCAPES` mode is active; names are quoted only
 * when needed (`quoteIfNeeded`).
 */

import type { ColumnTypeInfo } from '@/api/generated/ColumnTypeInfo'
import type { Flavor } from '@/api/generated/Flavor'
import type { TypeFamily } from '@/api/generated/TypeFamily'
import type { Cell } from '@/api/querySession'

import { writeByChunks } from './chunks'
import { quoteIfNeeded } from './navigatorNodes'
import { type GridFrame, frameCell } from './resultGrid'

/**
 * Media type of the file.
 */
export const SQL_TYPE = 'application/sql;charset=utf-8'

/**
 * First MySQL version that reads `VALUES ROW (…)`.
 */
export const MYSQL_VALUES_SINCE: [number, number, number] = [8, 0, 19]

/**
 * Name of the CTE.
 */
const NAME = 'ds'

/**
 * Number as a SQL literal reads it: sign, digits, decimal part, exponent.
 */
const NUMBER = /^[+-]?(\d+(\.\d*)?|\.\d+)([eE][+-]?\d+)?$/

/**
 * Exponent of a number.
 */
const EXPONENT = /[eE]/

/**
 * Date, or date-time, whose month or day is zero (`0000-00-00`,
 * `2026-10-00`).
 */
const ZERO_IN_DATE = /^\d{4}-(00-\d{2}|\d{2}-00)/

/**
 * Bytes that are not UTF-8, as the server sends them: `0x` and their
 * uppercase hexadecimal (`crates/connectors/src/mysql.rs`, `cell()`).
 */
const HEX_BYTES = /^0x((?:[0-9A-F]{2})*)$/

/**
 * How the CTE is written: dialect, modes of the session, layout.
 */
export interface CteOptions {
  /**
   * Dialect written: MySQL (`ROW (…)`) or MariaDB.
   */
  flavor: Flavor
  /**
   * Is the `ANSI_QUOTES` mode active? Names are then quoted with double
   * quotes, backticks otherwise.
   */
  ansiQuotes: boolean
  /**
   * Is the backslash an escape in strings (`NO_BACKSLASH_ESCAPES` mode
   * inactive)?
   */
  backslashEscapes: boolean
  /**
   * Reserved words of the dialect, in capitals: a name that is one is
   * quoted.
   */
  reserved: ReadonlySet<string>
  /**
   * Width of an indentation level, in spaces.
   */
  indentSize: number
  /**
   * Separator that ends the query, alone on the last line.
   */
  delimiter: string
}

/**
 * String literal.
 *
 * @param text Text of the string.
 * @param backslashEscapes Is the backslash an escape in strings?
 */
export function cteString(text: string, backslashEscapes: boolean): string {
  const escaped = backslashEscapes ? text.replace(/\\/g, '\\\\').replace(/\0/g, '\\0') : text
  return `'${escaped.replace(/'/g, "''")}'`
}

/**
 * Bytes of a binary cell: those of its hexadecimal (`0x…`), or those of its
 * text in UTF-8.
 *
 * @param cell Value of the cell.
 */
function cellBytes(cell: string): Uint8Array {
  const hex = HEX_BYTES.exec(cell)?.[1]
  return hex === undefined
    ? new TextEncoder().encode(cell)
    : Uint8Array.from(hex.match(/../g) ?? [], (pair) => parseInt(pair, 16))
}

/**
 * Bit-value literal of a `BIT` cell (`b'101'`), without its leading zeros.
 *
 * @param cell Value of the cell.
 */
function bitLiteral(cell: string): string {
  const bits = [...cellBytes(cell)].map((byte) => byte.toString(2).padStart(8, '0')).join('')
  return `b'${bits.replace(/^0+(?=.)/, '')}'`
}

/**
 * Typed literal of a date or a date-time; a string for MySQL when its month
 * or day is zero.
 *
 * @param keyword Type of the literal: `DATE` or `TIMESTAMP`.
 * @param cell Value of the cell.
 * @param options How the CTE is written.
 */
function dateLiteral(keyword: string, cell: string, options: CteOptions): string {
  const string = cteString(cell, options.backslashEscapes)
  return options.flavor === 'mysql' && ZERO_IN_DATE.test(cell) ? string : `${keyword}${string}`
}

/**
 * Literal of a value, of its column's type.
 *
 * @param cell Value, `null` for NULL.
 * @param family Family of the column's type, absent if unknown.
 * @param options How the CTE is written.
 */
export function cteLiteral(
  cell: Cell,
  family: TypeFamily | undefined,
  options: CteOptions,
): string {
  if (cell === null) {
    return 'NULL'
  }
  const string = cteString(cell, options.backslashEscapes)
  switch (family) {
    case 'integer':
    case 'decimal':
    case 'year':
      return NUMBER.test(cell) ? cell : string
    case 'float':
      if (!NUMBER.test(cell)) {
        return string
      }
      return EXPONENT.test(cell) ? cell : `${cell}E0`
    case 'date':
      return dateLiteral('DATE', cell, options)
    case 'datetime':
      return dateLiteral('TIMESTAMP', cell, options)
    case 'time':
      return `TIME${string}`
    case 'json':
      return options.flavor === 'mysql' ? `CAST(${string} AS JSON)` : string
    case 'binary':
    case 'geometry': {
      const hex = HEX_BYTES.exec(cell)?.[1]
      return hex === undefined ? `_binary${string}` : `X'${hex}'`
    }
    case 'bit':
      return bitLiteral(cell)
    default:
      return string
  }
}

/**
 * CTE of a frame of the grid, written by chunks of rows (`chunks.ts`). The
 * frame holds at least one row, and every one of its cells is selected
 * (`frameIsFull`): `VALUES` needs a row, and an unselected cell has no
 * value to write.
 *
 * @param frame Rows and columns to write, in the displayed order.
 * @param rows Rows of the result.
 * @param columns Names of the columns.
 * @param types Type of each column; empty if unknown.
 * @param options How the CTE is written.
 * @returns
 *     The query, its separator alone on the last line, without a line break
 *     at its end.
 */
export async function cteText(
  frame: GridFrame,
  rows: Cell[][],
  columns: string[],
  types: ColumnTypeInfo[],
  options: CteOptions,
): Promise<string> {
  const indent = ' '.repeat(options.indentSize)
  const names = frame.fields
    .map((field) => quoteIfNeeded(columns[field] ?? '', options.ansiQuotes, options.reserved))
    .join(', ')
  const families = frame.fields.map((field) => types[field]?.family)
  const row = options.flavor === 'mysql' ? 'ROW ' : ''
  const last = frame.rows.length - 1
  const lines = await writeByChunks(frame.rows.length, (index) => {
    const values = families.map((family, column) =>
      cteLiteral(frameCell(frame, rows, index, column) ?? null, family, options),
    )
    return `${indent}${indent}${row}(${values.join(', ')})${index < last ? ',' : ''}\n`
  })
  return [
    `WITH ${NAME} (${names}) AS (\n${indent}VALUES\n`,
    ...lines,
    `)\nSELECT ALL ${names}\nFROM ${NAME}\n${options.delimiter}`,
  ].join('')
}
