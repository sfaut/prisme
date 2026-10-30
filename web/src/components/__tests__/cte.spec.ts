import { readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { describe, expect, it } from 'vitest'

import type { Flavor } from '@/api/generated/Flavor'
import type { TypeFamily } from '@/api/generated/TypeFamily'
import type { Cell } from '@/api/querySession'

import { type CteOptions, cteLiteral, cteStatement, cteString, cteText } from '../cte'
import { viewFrame } from '../resultGrid'

/**
 * Reference files, also run on real servers (`crates/connectors/tests/cte.rs`).
 */
const FIXTURES = resolve(
  dirname(fileURLToPath(import.meta.url)),
  '../../../../tests/fixtures/export/cte',
)

const input = JSON.parse(readFileSync(resolve(FIXTURES, 'input.json'), 'utf-8')) as {
  reserved: string[]
  columns: string[]
  families: TypeFamily[]
  rows: Cell[][]
}

/**
 * Options of a CTE: MySQL, default modes, indentation of 4.
 *
 * @param changes Options that differ.
 */
function options(changes: Partial<CteOptions> = {}): CteOptions {
  return {
    flavor: 'mysql',
    ansiQuotes: false,
    backslashEscapes: true,
    reserved: new Set(['SELECT']),
    indentSize: 4,
    ...changes,
  }
}

describe('CTE', () => {
  it.each([
    ['mysql', false],
    ['mysql', true],
    ['mariadb', false],
    ['mariadb', true],
  ] as [Flavor, boolean][])(
    'writes the reference file of %s byte for byte (modes: %s)',
    async (flavor, modes) => {
      const types = input.families.map((family) => ({ family, name: '' }))
      const frame = viewFrame(
        input.rows.map((_, index) => index),
        input.columns.map((_, index) => index),
      )
      const text = await cteText(
        frame,
        input.rows,
        input.columns,
        types,
        options({
          flavor,
          ansiQuotes: modes,
          backslashEscapes: !modes,
          reserved: new Set(input.reserved),
        }),
      )
      const file = `expected-${flavor}${modes ? '-modes' : ''}.sql`
      expect(`${cteStatement(text, ';')}\n`).toBe(readFileSync(resolve(FIXTURES, file), 'utf-8'))
    },
  )

  it('writes the form of each dialect, names quoted only when needed', async () => {
    const rows: Cell[][] = [
      ['1', 'Ada', '2026-10-12 09:32:00'],
      ['2', null, null],
    ]
    const columns = ['id', 'name', 'created_at']
    const types = (['integer', 'text', 'datetime'] as TypeFamily[]).map((family) => ({
      family,
      name: '',
    }))
    const frame = viewFrame([0, 1], [0, 1, 2])
    expect(cteStatement(await cteText(frame, rows, columns, types, options()), ';')).toBe(
      [
        'WITH ds (id, name, created_at) AS (',
        '    VALUES',
        "        ROW (1, 'Ada', TIMESTAMP'2026-10-12 09:32:00'),",
        '        ROW (2, NULL, NULL)',
        ')',
        'SELECT ALL id, name, created_at',
        'FROM ds',
        ';',
      ].join('\n'),
    )
    const mariadb = await cteText(
      frame,
      rows,
      columns,
      types,
      options({ flavor: 'mariadb', indentSize: 2 }),
    )
    expect(mariadb).toContain(
      "\n    (1, 'Ada', TIMESTAMP'2026-10-12 09:32:00'),\n    (2, NULL, NULL)\n",
    )
    expect(cteStatement(mariadb, '//').endsWith('\nFROM ds\n//')).toBe(true)
  })

  it('follows the order of the frame, and its sorted rows', async () => {
    const rows: Cell[][] = [
      ['1', 'b'],
      ['2', 'a'],
    ]
    const text = await cteText(viewFrame([1, 0], [1, 0]), rows, ['n', 'l'], [], options())
    expect(text).toContain(
      "WITH ds (l, n) AS (\n    VALUES\n        ROW ('a', '2'),\n        ROW ('b', '1')\n)",
    )
  })

  it('quotes strings according to NO_BACKSLASH_ESCAPES', () => {
    expect(cteString("dit 'oui' a\\b \0", true)).toBe("'dit ''oui'' a\\\\b \\0'")
    expect(cteString("dit 'oui' a\\b \0", false)).toBe("'dit ''oui'' a\\b \0'")
  })

  it('writes numbers as is, floating ones with an exponent, and refuses what is not a number', () => {
    expect(cteLiteral('-3.50', 'decimal', options())).toBe('-3.50')
    expect(cteLiteral('2026', 'year', options())).toBe('2026')
    expect(cteLiteral('1.5', 'float', options())).toBe('1.5E0')
    expect(cteLiteral('1e20', 'float', options())).toBe('1e20')
    expect(cteLiteral('1); DROP TABLE t; --', 'integer', options())).toBe("'1); DROP TABLE t; --'")
    expect(cteLiteral('NaN', 'float', options())).toBe("'NaN'")
  })

  it('types dates, writing a zero month or day as a string for MySQL only', () => {
    expect(cteLiteral('2026-10-12', 'date', options())).toBe("DATE'2026-10-12'")
    expect(cteLiteral('0000-01-01', 'date', options())).toBe("DATE'0000-01-01'")
    expect(cteLiteral('0000-00-00', 'date', options())).toBe("'0000-00-00'")
    expect(cteLiteral('2026-10-00 12:00:00', 'datetime', options())).toBe("'2026-10-00 12:00:00'")
    expect(cteLiteral('0000-00-00', 'date', options({ flavor: 'mariadb' }))).toBe(
      "DATE'0000-00-00'",
    )
    expect(cteLiteral('-838:59:59', 'time', options())).toBe("TIME'-838:59:59'")
  })

  it('casts JSON for MySQL only', () => {
    expect(cteLiteral('[1, 2]', 'json', options())).toBe("CAST('[1, 2]' AS JSON)")
    expect(cteLiteral('[1, 2]', 'json', options({ flavor: 'mariadb' }))).toBe("'[1, 2]'")
  })

  it('writes bytes in hexadecimal or as a binary string, bits as a bit value', () => {
    expect(cteLiteral('0xFF001A', 'binary', options())).toBe("X'FF001A'")
    expect(cteLiteral('abc', 'binary', options())).toBe("_binary'abc'")
    expect(cteLiteral('0x', 'geometry', options())).toBe("X''")
    expect(cteLiteral('\u0005', 'bit', options())).toBe("b'101'")
    expect(cteLiteral('\u0000', 'bit', options())).toBe("b'0'")
    expect(cteLiteral('0x0100', 'bit', options())).toBe("b'100000000'")
  })

  it('writes NULL, and every other value as a string', () => {
    expect(cteLiteral(null, 'integer', options())).toBe('NULL')
    expect(cteLiteral('M', 'enum', options())).toBe("'M'")
    expect(cteLiteral('', undefined, options())).toBe("''")
  })

  it('quotes names according to ANSI_QUOTES and the reserved words', async () => {
    const text = await cteText(
      viewFrame([0], [0, 1, 2]),
      [['1', '2', '3']],
      ['select', 'a b', 'ok'],
      [],
      options({ ansiQuotes: true }),
    )
    expect(text.split('\n')[0]).toBe('WITH ds ("select", "a b", ok) AS (')
  })
})
