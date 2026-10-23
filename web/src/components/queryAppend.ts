/**
 * Query appended at the end of the editor's text (ticket #10): a double click
 * on a table of the navigator adds its `SELECT` after the last query, without
 * overwriting anything, then runs it. The operations on an index (ticket #21)
 * are appended the same way, as a block of commented-out statements.
 *
 * The query goes after one empty line, ended by the separator in force at the
 * end of the text (`;`, or that of the last `DELIMITER`). A last query left
 * without its separator gets it right after its last significant character,
 * before the comments that may follow: otherwise the two queries would make
 * one. The server gives these two facts (`TextEnd`, the splitting of the
 * run). Positions are in UTF-16 units, like those of the editor and of
 * JavaScript strings.
 */

import type { TextEnd } from '@/api/generated/TextEnd'

/**
 * Change of the text that appends a query.
 */
export interface Appending {
  /**
   * Changes to apply, in positions of the current text.
   */
  changes: { from: number; to?: number; insert: string }[]
  /**
   * Whole text once changed.
   */
  text: string
  /**
   * Start of the appended query in the new text: the cursor that runs it.
   */
  start: number
}

/**
 * Blanks at the end of the text, those of the splitter.
 */
const TRAILING_BLANKS = /[ \t\r\n]*$/

/**
 * Appends a query at the end of a text, ended by the separator in force.
 *
 * @param text Current text of the editor.
 * @param query Query to append, without its separator.
 * @param end End of the text, as analysed by the server.
 */
export function appendQuery(text: string, query: string, end: TextEnd): Appending {
  return appendBlock(text, `${query}${end.delimiter}`, end)
}

/**
 * Appends a block (a query and its separator, commented-out statements) at
 * the end of a text, on its own line. A text made only of blanks is replaced
 * by the block; the blanks at the end of the others give way to a single
 * empty line. A last query left without its separator gets it.
 *
 * @param text Current text of the editor.
 * @param block Block to append, without a line break at its end.
 * @param end End of the text, as analysed by the server.
 */
export function appendBlock(text: string, block: string, end: TextEnd): Appending {
  const statement = `${block}\n`
  const contentEnd = text.length - (TRAILING_BLANKS.exec(text)?.[0].length ?? 0)
  if (contentEnd === 0) {
    return { changes: [{ from: 0, to: text.length, insert: statement }], text: statement, start: 0 }
  }
  const changes: Appending['changes'] = []
  let head = text.slice(0, contentEnd)
  if (end.open_end !== null) {
    changes.push({ from: end.open_end, insert: end.delimiter })
    head = `${text.slice(0, end.open_end)}${end.delimiter}${text.slice(end.open_end, contentEnd)}`
  }
  changes.push({ from: contentEnd, to: text.length, insert: `\n\n${statement}` })
  return { changes, text: `${head}\n\n${statement}`, start: head.length + 2 }
}
