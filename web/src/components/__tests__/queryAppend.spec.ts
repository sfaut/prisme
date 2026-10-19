import { describe, expect, it } from 'vitest'

import { appendQuery } from '../queryAppend'

/**
 * End of a text whose last query has its separator `;`.
 */
const CLOSED = { delimiter: ';', open_end: null }

/**
 * Applies the changes to the text, from the last one, as the editor would.
 *
 * @param text Initial text.
 * @param changes Changes, in positions of the initial text.
 */
function applied(text: string, changes: { from: number; to?: number; insert: string }[]): string {
  return [...changes]
    .reverse()
    .reduce(
      (result, change) =>
        `${result.slice(0, change.from)}${change.insert}${result.slice(change.to ?? change.from)}`,
      text,
    )
}

describe('query appended after the last one', () => {
  it('goes after one empty line, with its separator, cursor on its start', () => {
    const appending = appendQuery('SELECT 1;\n', 'SELECT * FROM t', CLOSED)
    expect(appending.text).toBe('SELECT 1;\n\nSELECT * FROM t;\n')
    expect(appending.text.slice(appending.start)).toBe('SELECT * FROM t;\n')
    expect(applied('SELECT 1;\n', appending.changes)).toBe(appending.text)
  })

  it('replaces the blanks at the end of the text with a single empty line', () => {
    expect(appendQuery('SELECT 1;\n\n\n   ', 'SELECT 2', CLOSED).text).toBe(
      'SELECT 1;\n\nSELECT 2;\n',
    )
    expect(appendQuery('SELECT 1;', 'SELECT 2', CLOSED).text).toBe('SELECT 1;\n\nSELECT 2;\n')
  })

  it('ends the last query with its missing separator, before its comment', () => {
    const text = 'SELECT 1 -- note'
    const appending = appendQuery(text, 'SELECT 2', { delimiter: ';', open_end: 8 })
    expect(appending.text).toBe('SELECT 1; -- note\n\nSELECT 2;\n')
    expect(appending.text.slice(appending.start)).toBe('SELECT 2;\n')
    expect(applied(text, appending.changes)).toBe(appending.text)
  })

  it('takes the separator of the last DELIMITER command', () => {
    expect(
      appendQuery('DELIMITER //\nCALL p()//', 'SELECT 2', { delimiter: '//', open_end: null }).text,
    ).toBe('DELIMITER //\nCALL p()//\n\nSELECT 2//\n')
  })

  it('keeps the comments at the end of the text, before the query', () => {
    expect(appendQuery('SELECT 1;\n-- to check\n', 'SELECT 2', CLOSED).text).toBe(
      'SELECT 1;\n-- to check\n\nSELECT 2;\n',
    )
  })

  it('takes the place of a text made only of blanks', () => {
    expect(appendQuery('', 'SELECT 2', CLOSED)).toEqual({
      changes: [{ from: 0, to: 0, insert: 'SELECT 2;\n' }],
      text: 'SELECT 2;\n',
      start: 0,
    })
    expect(appendQuery('\n  \n', 'SELECT 2', CLOSED).text).toBe('SELECT 2;\n')
  })
})
