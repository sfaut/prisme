import { describe, expect, it } from 'vitest'

import type { Cell } from '@/api/querySession'

import {
  dropPosition,
  moveColumn,
  MAX_ESTIMATED_WIDTH,
  MAX_FITTED_WIDTH,
  estimatedWidth,
  fittedWidth,
  MIN_COLUMN_WIDTH,
  resizedWidth,
  compareCells,
  emptySelection,
  isFixedLengthText,
  isSelected,
  nextSort,
  selectAll,
  selectCell,
  selectColumn,
  selectRow,
  selectionToText,
  createViewMemory,
  type GridFilter,
  type SortState,
  updateView,
  visibleRows,
} from '../resultGrid'

const columns = ['id', 'name', 'city']
const rows: Cell[][] = [
  ['10', 'Zoé', 'Lyon'],
  ['2', 'adam', null],
  ['1', 'Éric', 'Paris'],
]
const noFilter = { text: '', column: null }

describe('fixed-length text', () => {
  it('recognizes CHAR, with or without a length, and only it', () => {
    expect(isFixedLengthText({ family: 'text', name: 'CHAR(4)' })).toBe(true)
    expect(isFixedLengthText({ family: 'text', name: 'CHAR' })).toBe(true)
    expect(isFixedLengthText({ family: 'text', name: 'VARCHAR(20)' })).toBe(false)
    expect(isFixedLengthText({ family: 'text', name: 'TEXT' })).toBe(false)
    expect(isFixedLengthText({ family: 'binary', name: 'BINARY(16)' })).toBe(false)
    expect(isFixedLengthText(undefined)).toBe(false)
  })
})

describe('sort', () => {
  it('compares numbers by value and puts NULL first', () => {
    expect(compareCells('2', '10')).toBeLessThan(0)
    expect(compareCells(null, 'a')).toBeLessThan(0)
    expect(compareCells('a2', 'a10')).toBeLessThan(0)
  })

  it('sorts the view without touching the rows', () => {
    expect(visibleRows(rows, noFilter, { column: 0, direction: 'asc' })).toEqual([2, 1, 0])
    expect(visibleRows(rows, noFilter, { column: 2, direction: 'asc' })).toEqual([1, 0, 2])
    expect(visibleRows(rows, noFilter, { column: 0, direction: 'desc' })).toEqual([0, 1, 2])
    expect(rows[0]?.[0]).toBe('10')
  })

  it('goes from ascending to descending then to no sort', () => {
    const asc = nextSort(null, 1)
    expect(asc).toEqual({ column: 1, direction: 'asc' })
    const desc = nextSort(asc, 1)
    expect(desc).toEqual({ column: 1, direction: 'desc' })
    expect(nextSort(desc, 1)).toBeNull()
    expect(nextSort(desc, 2)).toEqual({ column: 2, direction: 'asc' })
  })
})

describe('filter', () => {
  it('searches in all the columns, case-insensitive', () => {
    expect(visibleRows(rows, { text: 'PAR', column: null }, null)).toEqual([2])
  })

  it('searches in a single column', () => {
    expect(visibleRows(rows, { text: '1', column: 0 }, null)).toEqual([0, 2])
    expect(visibleRows(rows, { text: '1', column: 1 }, null)).toEqual([])
  })

  it('finds NULL by its displayed text', () => {
    expect(visibleRows(rows, { text: 'null', column: 2 }, null)).toEqual([1])
  })
})

describe('selection', () => {
  it('extends a range from the starting point (Shift+click)', () => {
    let selection = selectCell(emptySelection(), 0, 0, 'replace')
    selection = selectCell(selection, 1, 1, 'extend')
    expect(selection.ranges).toEqual([{ top: 0, bottom: 1, left: 0, right: 1 }])
    expect(isSelected(selection, 1, 0)).toBe(true)
    expect(isSelected(selection, 2, 0)).toBe(false)
  })

  it('adds ranges (Ctrl+click)', () => {
    let selection = selectCell(emptySelection(), 0, 0, 'replace')
    selection = selectCell(selection, 2, 2, 'add')
    expect(isSelected(selection, 0, 0)).toBe(true)
    expect(isSelected(selection, 2, 2)).toBe(true)
    expect(isSelected(selection, 1, 1)).toBe(false)
  })

  it('selects whole rows and columns', () => {
    let selection = selectRow(emptySelection(), 0, 3, 'replace')
    selection = selectRow(selection, 1, 3, 'extend')
    expect(selection.ranges).toEqual([{ top: 0, bottom: 1, left: 0, right: 2 }])

    selection = selectColumn(emptySelection(), 1, 3, 'replace')
    expect(selection.ranges).toEqual([{ top: 0, bottom: 2, left: 1, right: 1 }])
  })

  it('selects everything, or nothing for an empty grid', () => {
    expect(selectAll(3, 3).ranges).toEqual([{ top: 0, bottom: 2, left: 0, right: 2 }])
    expect(selectAll(0, 3)).toEqual(emptySelection())
  })
})

describe('copy', () => {
  const view = [0, 1, 2]

  it('copies a range as tab-separated text, NULL as an empty cell', () => {
    const selection = selectCell(selectCell(emptySelection(), 0, 1, 'replace'), 1, 2, 'extend')
    expect(selectionToText(selection, view, rows, columns, false)).toBe('Zoé\tLyon\nadam\t')
  })

  it('adds the headers on request', () => {
    const selection = selectColumn(emptySelection(), 0, 3, 'replace')
    expect(selectionToText(selection, view, rows, columns, true)).toBe('id\n10\n2\n1')
  })

  it('follows the sorted view', () => {
    const sorted = visibleRows(rows, noFilter, { column: 0, direction: 'asc' })
    const selection = selectRow(emptySelection(), 0, 3, 'replace')
    expect(selectionToText(selection, sorted, rows, columns, false)).toBe('1\tÉric\tParis')
  })

  it('leaves empty the unselected cells of the frame', () => {
    let selection = selectCell(emptySelection(), 0, 0, 'replace')
    selection = selectCell(selection, 1, 2, 'add')
    expect(selectionToText(selection, view, rows, columns, false)).toBe('10\t\n\t')
  })

  it('copies the raw values, quotes included', () => {
    const quoted: Cell[][] = [['v1.1', '"v1.2"', 'dit "oui"']]
    expect(selectionToText(selectAll(1, 3), [0], quoted, columns, false)).toBe(
      'v1.1\t"v1.2"\tdit "oui"',
    )
  })

  it('writes out the control characters', () => {
    const special: Cell[][] = [['a\tb', 'x\r\ny', '\0\x07\b\v\f\x1b']]
    expect(selectionToText(selectAll(1, 3), [0], special, columns, false)).toBe(
      'a\\tb\tx\\r\\ny\t\\0\\a\\b\\v\\f\\e',
    )
  })

  it('writes in hexadecimal a control character without a known spelling', () => {
    const special: Cell[][] = [['\x01\x1f\x7f\x85', 'é', 'ok']]
    expect(selectionToText(selectAll(1, 3), [0], special, columns, false)).toBe(
      '\\x01\\x1F\\x7F\\x85\té\tok',
    )
  })

  it('leaves the backslashes as they are', () => {
    const paths: Cell[][] = [['C:\\dir', 'a\\tb', '']]
    expect(selectionToText(selectAll(1, 3), [0], paths, columns, false)).toBe('C:\\dir\ta\\tb\t')
  })

  it('also writes out the control characters of column names', () => {
    const selection = selectColumn(emptySelection(), 0, 1, 'replace')
    expect(selectionToText(selection, [0], [['1']], ['a\tb'], true)).toBe('a\\tb\n1')
  })
})

describe('column width', () => {
  it('follows the pointer without going below the minimum', () => {
    expect(resizedWidth(120, 35.6)).toBe(156)
    expect(resizedWidth(120, -30)).toBe(90)
    expect(resizedWidth(120, -500)).toBe(MIN_COLUMN_WIDTH)
  })
})

describe('estimated or fitted width', () => {
  const rows: Cell[][] = [['a'], ['x'.repeat(80)], ...Array.from({ length: 300 }, () => ['b'])]

  it('estimates on the first rows, with a cap', () => {
    expect(estimatedWidth('id', [['1']], 0)).toBe(48)
    expect(estimatedWidth('texte', rows, 0)).toBe(MAX_ESTIMATED_WIDTH)
    // Long value beyond the first 200 rows: ignored by the estimate.
    expect(estimatedWidth('texte', [...rows.slice(2), ['x'.repeat(80)]], 0)).toBe(65)
  })

  it('fits on all the rows, with a higher cap', () => {
    expect(fittedWidth('texte', [...rows.slice(2), ['x'.repeat(80)]], 0)).toBe(80 * 7 + 16)
    expect(fittedWidth('texte', [['x'.repeat(500)]], 0)).toBe(MAX_FITTED_WIDTH)
  })
})

describe('column move', () => {
  it('inserts the column at the drop position', () => {
    // a b c d: b dropped at the far right, then at the far left.
    expect(moveColumn([0, 1, 2, 3], 1, 4)).toEqual({ order: [0, 2, 3, 1], position: 3 })
    expect(moveColumn([0, 1, 2, 3], 1, 0)).toEqual({ order: [1, 0, 2, 3], position: 0 })
    // Dropped just before or just after itself: nothing moves.
    expect(moveColumn([0, 1, 2, 3], 1, 1).order).toEqual([0, 1, 2, 3])
    expect(moveColumn([0, 1, 2, 3], 1, 2).order).toEqual([0, 1, 2, 3])
  })

  it('chooses the position after the middle of the headers', () => {
    const centers = [50, 150, 250]
    expect(dropPosition(centers, 10)).toBe(0)
    expect(dropPosition(centers, 160)).toBe(2)
    expect(dropPosition(centers, 400)).toBe(3)
  })

  it('copies the columns in the displayed order', () => {
    const selection = selectRow(emptySelection(), 0, 3, 'replace')
    expect(selectionToText(selection, [0], rows, columns, true, [2, 0, 1])).toBe(
      'city\tid\tname\nLyon\t10\tZoé',
    )
  })
})

describe('updateView', () => {
  // Rows with duplicates in each column, so that the stability of the sort is tested.
  const data: Cell[][] = Array.from({ length: 1200 }, (_, index) => [
    String((index * 37) % 50),
    `item ${(index * 13) % 40}`,
  ])
  const noFilter: GridFilter = { text: '', column: null }

  /**
   * Feeds the rows to the memory by batches, in one array that grows as the store does,
   * and pairs each view given by the memory with the full computation.
   *
   * @param batch Size of each batch.
   * @param filter Filter of the view.
   * @param sort Sort of the view, `null` for none.
   * @returns The pairs (view of the memory, full computation), one per batch.
   */
  function receiveBy(
    batch: number,
    filter: GridFilter = noFilter,
    sort: SortState | null = null,
  ): [number[], number[]][] {
    const received: Cell[][] = []
    const memory = createViewMemory()
    const pairs: [number[], number[]][] = []
    for (let start = 0; start < data.length; start += batch) {
      received.push(...data.slice(start, start + batch))
      pairs.push([updateView(memory, received, filter, sort), visibleRows(received, filter, sort)])
    }
    return pairs
  }

  it('gives the same view as a full computation, batch after batch', () => {
    const sorts: (SortState | null)[] = [
      null,
      { column: 0, direction: 'asc' },
      { column: 1, direction: 'desc' },
    ]
    for (const sort of sorts) {
      for (const [actual, expected] of receiveBy(100, noFilter, sort)) {
        expect(actual).toEqual(expected)
      }
    }
  })

  it('applies a filter to the rows received since', () => {
    const filter: GridFilter = { text: 'item 1', column: 1 }
    for (const [actual, expected] of receiveBy(100, filter, { column: 0, direction: 'asc' })) {
      expect(actual).toEqual(expected)
    }
  })

  it('starts again on a new array, a new filter or a new sort', () => {
    const memory = createViewMemory()
    updateView(memory, data, noFilter, null)
    const fresh = data.slice(0, 10)
    expect(updateView(memory, fresh, noFilter, null)).toEqual(visibleRows(fresh, noFilter, null))
    const filter: GridFilter = { text: '3', column: 0 }
    expect(updateView(memory, fresh, filter, null)).toEqual(visibleRows(fresh, filter, null))
  })

  it('returns a new array each time, so that Vue sees the change', () => {
    const received = data.slice(0, 5)
    const memory = createViewMemory()
    const first = updateView(memory, received, noFilter, null)
    received.push(...data.slice(5, 6))
    const second = updateView(memory, received, noFilter, null)
    expect(second).not.toBe(first)
    expect(first).toHaveLength(5)
  })
})
