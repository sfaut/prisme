import { readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { describe, expect, it } from 'vitest'

import type { TypeFamily } from '@/api/generated/TypeFamily'
import type { Cell } from '@/api/querySession'

import { csvBlob, csvCell, csvField, duplicateNames } from '../csv'
import { emptySelection, selectCell, selectionFrame, viewFrame } from '../resultGrid'

/**
 * Reference files shared with the Rust serialiser.
 */
const FIXTURES = resolve(
  dirname(fileURLToPath(import.meta.url)),
  '../../../../tests/fixtures/export/csv-excel-fr',
)

const input = JSON.parse(readFileSync(resolve(FIXTURES, 'input.json'), 'utf-8')) as {
  columns: string[]
  families: TypeFamily[]
  rows: Cell[][]
}
const expected = readFileSync(resolve(FIXTURES, 'expected.csv'))

describe('CSV for Excel in French', () => {
  it('writes the reference file byte for byte', async () => {
    const types = input.families.map((family) => ({ family, name: '' }))
    const frame = viewFrame(
      input.rows.map((_, index) => index),
      input.columns.map((_, index) => index),
    )
    const blob = await csvBlob(frame, input.rows, input.columns, types)
    expect(Buffer.from(await blob.arrayBuffer())).toEqual(expected)
    expect(blob.type).toBe('text/csv;charset=utf-8')
  })

  it('quotes a field only when it needs it', () => {
    expect(csvField('simple')).toBe('simple')
    expect(csvField('a;b')).toBe('"a;b"')
    expect(csvField('dit "oui"')).toBe('"dit ""oui"""')
    expect(csvField('a\nb')).toBe('"a\nb"')
  })

  it('tells NULL, an empty string and an unselected cell apart', () => {
    expect(csvCell(null, 'text')).toBe('')
    expect(csvCell(undefined, 'text')).toBe('')
    expect(csvCell('', 'text')).toBe('""')
  })

  it('gives the decimal comma to decimal and float numbers only', () => {
    expect(csvCell('3.14', 'decimal')).toBe('3,14')
    expect(csvCell('1.5e-7', 'float')).toBe('1,5e-7')
    expect(csvCell('1.5', 'text')).toBe('1.5')
    expect(csvCell('2026-01-01 00:00:00.5', 'datetime')).toBe('2026-01-01 00:00:00.5')
    expect(csvCell('3.14', undefined)).toBe('3.14')
  })

  it('writes a selection in the displayed order, unselected cells empty', async () => {
    const rows: Cell[][] = [
      ['1', 'a', 'x'],
      ['2', 'b', 'y'],
    ]
    let selection = selectCell(emptySelection(), 0, 0, 'replace')
    selection = selectCell(selection, 1, 1, 'add')
    // Displayed order: third column, then first, then second.
    const frame = selectionFrame(selection, [0, 1], [2, 0, 1])
    const blob = await csvBlob(frame, rows, ['id', 'nom', 'ville'], [])
    // `text()` decodes the UTF-8 and leaves out the BOM.
    expect(await blob.text()).toBe('ville;id\r\nx;\r\n;2\r\n')
  })

  it('writes a large result over several chunks', async () => {
    const rows: Cell[][] = Array.from({ length: 4500 }, (_, index) => [String(index)])
    const frame = viewFrame(
      rows.map((_, index) => index),
      [0],
    )
    const text = await (await csvBlob(frame, rows, ['n'], [])).text()
    const lines = text.split('\r\n')
    expect(lines).toHaveLength(4502)
    expect(lines[4500]).toBe('4499')
  })
})

describe('duplicate names', () => {
  it('lists each duplicate name once', () => {
    expect(duplicateNames(['id', 'nom', 'id', 'id', 'nom', 'ville'])).toEqual(['id', 'nom'])
    expect(duplicateNames(['id', 'nom'])).toEqual([])
  })
})
