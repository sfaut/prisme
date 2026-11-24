import { describe, expect, it } from 'vitest'

import type { TypeFamily } from '@/api/generated/TypeFamily'
import type { Cell } from '@/api/querySession'

import { type InClauseOptions, inClause } from '../inClause'
import { viewFrame } from '../resultGrid'

/**
 * Options of a clause: MySQL, default modes, indentation of 4.
 *
 * @param changes Options that differ.
 */
function options(changes: Partial<InClauseOptions> = {}): InClauseOptions {
  return { flavor: 'mysql', backslashEscapes: true, indentSize: 4, ...changes }
}

/**
 * Types of the columns, from their families.
 *
 * @param families Family of each column.
 */
function types(...families: TypeFamily[]) {
  return families.map((family) => ({ family, name: '' }))
}

/**
 * Clause of every row and column of a result.
 *
 * @param rows Rows of the result.
 * @param families Family of each column.
 * @param changes Options that differ.
 */
function clause(rows: Cell[][], families: TypeFamily[], changes: Partial<InClauseOptions> = {}) {
  const frame = viewFrame(
    rows.map((_, index) => index),
    families.map((_, index) => index),
  )
  return inClause(frame, rows, types(...families), options(changes))
}

describe('IN clause', () => {
  it('writes the values of one column on one line, numbers bare', async () => {
    expect(await clause([['1234'], ['2345'], ['3456']], ['integer'])).toBe('IN (1234, 2345, 3456)')
  })

  it('writes a row constructor per line for several columns', async () => {
    const rows: Cell[][] = [
      ['val1', '1234'],
      ['val2', '2345'],
    ]
    expect(await clause(rows, ['text', 'integer'])).toBe(
      ['IN (', "    ('val1', 1234),", "    ('val2', 2345)", ')'].join('\n'),
    )
    expect(await clause(rows, ['text', 'integer'], { indentSize: 2 })).toContain(
      "\n  ('val1', 1234),\n",
    )
  })

  it('writes each value once, at its first place', async () => {
    expect(await clause([['b'], ['a'], ['b']], ['text'])).toBe("IN ('b', 'a')")
    const rows: Cell[][] = [
      ['a', '1'],
      ['a', '2'],
      ['a', '1'],
    ]
    expect(await clause(rows, ['text', 'integer'])).toBe(
      ['IN (', "    ('a', 1),", "    ('a', 2)", ')'].join('\n'),
    )
  })

  it('leaves NULL out of one column, keeps a row of several that holds it', async () => {
    expect(await clause([['1'], ['2'], ['2'], [null]], ['integer'])).toBe('IN (1, 2)')
    expect(await clause([[null], [null]], ['integer'])).toBeNull()
    expect(
      await clause(
        [
          ['a', null],
          ['a', null],
        ],
        ['text', 'integer'],
      ),
    ).toBe(['IN (', "    ('a', NULL)", ')'].join('\n'))
  })

  it('writes the literals of the dialect and of its modes', async () => {
    expect(await clause([["it's"], ['a\\b']], ['text'])).toBe("IN ('it''s', 'a\\\\b')")
    expect(await clause([['a\\b']], ['text'], { backslashEscapes: false })).toBe("IN ('a\\b')")
    expect(await clause([['2026-10-12']], ['date'])).toBe("IN (DATE'2026-10-12')")
    expect(await clause([['[1]']], ['json'])).toBe("IN (CAST('[1]' AS JSON))")
    expect(await clause([['[1]']], ['json'], { flavor: 'mariadb' })).toBe("IN ('[1]')")
    const sqlite = { flavor: 'sqlite' as const, backslashEscapes: false }
    expect(await clause([['2026-10-12'], ['a\\b']], ['date'], sqlite)).toBe(
      "IN ('2026-10-12', 'a\\b')",
    )
    expect(await clause([['0x6162']], ['binary'], sqlite)).toBe("IN (X'6162')")
    // A value of a numeric column that does not read as a number is a string.
    expect(await clause([['1); DROP TABLE t; --']], ['integer'])).toBe(
      "IN ('1); DROP TABLE t; --')",
    )
  })

  it('reads the columns of the frame in their displayed order', async () => {
    const rows: Cell[][] = [['1', 'a']]
    const frame = viewFrame([0], [1, 0])
    expect(await inClause(frame, rows, types('integer', 'text'), options())).toBe(
      ['IN (', "    ('a', 1)", ')'].join('\n'),
    )
  })
})
