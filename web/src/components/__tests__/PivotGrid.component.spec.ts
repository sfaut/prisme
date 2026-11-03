import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import PivotGrid from '../PivotGrid.vue'
import {
  FIRST_LAYOUT,
  LABEL_ORDER,
  type PivotOrder,
  type PivotShape,
  type PivotWindow,
} from '../pivotEngine'

/**
 * Table of two regions by two years, with its totals.
 */
const SHAPE: PivotShape = {
  lines: 3,
  columns: 3,
  rowLevels: 1,
  columnLevels: 1,
  groups: 4,
  rows: 10,
}

/**
 * Whole window of the table.
 */
const WINDOW: PivotWindow = {
  lines: [
    { depth: 1, label: 'Nord', path: ['Nord'], shared: 0 },
    { depth: 1, label: null, path: [null], shared: 0 },
    { depth: 0, label: null, path: [], shared: 0 },
  ],
  columns: [
    { node: 1, path: [[1, '2025']], total: false, measure: 0 },
    { node: 2, path: [[2, '2026']], total: false, measure: 0 },
    { node: 0, path: [], total: true, measure: 0 },
  ],
  cells: [
    ['1', '2', '3'],
    ['4', '5', '9'],
    ['5', '7', '12'],
  ],
}

let wrapper: VueWrapper | null = null

beforeEach(() => {
  vi.stubGlobal(
    'ResizeObserver',
    class {
      observe() {}
      disconnect() {}
    },
  )
})

afterEach(() => {
  wrapper?.unmount()
  wrapper = null
  vi.unstubAllGlobals()
})

/**
 * Mounts the table, and waits for its window.
 *
 * @param props Properties that differ from those of the table of the regions.
 */
async function mountGrid(props: Record<string, unknown> = {}) {
  const mounted = mount(PivotGrid, {
    attachTo: document.body,
    props: {
      table: 1,
      shape: SHAPE,
      measureLabels: ['SUM(montant)'],
      rowNames: ['region'],
      columnNames: ['annee'],
      order: LABEL_ORDER,
      layout: FIRST_LAYOUT,
      repeatLabels: false,
      read: () => Promise.resolve(WINDOW),
      ...props,
    },
  })
  wrapper = mounted
  await flushPromises()
  return mounted
}

describe('the sort buttons of PivotGrid', () => {
  it('asks for the sort of a level by its name, on its axis', async () => {
    const grid = await mountGrid()
    const buttons = grid.findAll('.pivot-corner .pivot-sort')
    expect(buttons.map((button) => button.attributes('aria-label'))).toEqual([
      'Trier par region',
      'Trier par annee',
    ])
    await buttons[0]?.trigger('click')
    await buttons[1]?.trigger('click')
    expect(grid.emitted('sortLevel')).toEqual([
      ['rows', 0],
      ['columns', 0],
    ])
  })

  it('ranks the lines by the header of a column, a total included', async () => {
    const grid = await mountGrid()
    const buttons = grid.findAll('.pivot-head .pivot-sort')
    expect(buttons).toHaveLength(3)
    await buttons[1]?.trigger('click')
    await buttons[2]?.trigger('click')
    expect(grid.emitted('sortValues')).toEqual([
      ['rows', { path: ['2026'], measure: 0 }],
      ['rows', { path: [], measure: 0 }],
    ])
  })

  it('ranks the columns by the label of a line, NULL included', async () => {
    const grid = await mountGrid()
    const buttons = grid.findAll('.pivot-label .pivot-sort')
    expect(buttons).toHaveLength(3)
    await buttons[1]?.trigger('click')
    expect(grid.emitted('sortValues')).toEqual([['columns', { path: [null], measure: 0 }]])
  })

  it('reads the order: the direction of a level, the values that rank an axis', async () => {
    const grid = await mountGrid()
    const marks = (selector: string) =>
      grid.findAll(`${selector} .pivot-sort`).map((button) => button.text())
    expect(marks('.pivot-corner')).toEqual(['▲', '▲'])
    expect(marks('.pivot-head')).toEqual(['⇅', '⇅', '⇅'])
    expect(marks('.pivot-label')).toEqual(['⇅', '⇅', '⇅'])

    const order: PivotOrder = {
      rows: { descending: [], values: { path: ['2026'], measure: 0, descending: true } },
      columns: { descending: [true], values: { path: [null], measure: 0, descending: false } },
    }
    await grid.setProps({ order })
    expect(marks('.pivot-corner')).toEqual(['⇅', '⇅'])
    expect(marks('.pivot-head')).toEqual(['⇅', '▼', '⇅'])
    expect(marks('.pivot-label')).toEqual(['⇅', '▲', '⇅'])
    expect(grid.findAll('.pivot-label .pivot-sort')[1]?.classes()).toContain('active')

    await grid.setProps({
      order: {
        rows: { descending: [true], values: null },
        columns: { descending: [], values: null },
      },
    })
    expect(marks('.pivot-corner')).toEqual(['▼', '▲'])
    expect(grid.find('.pivot-corner .pivot-sort').classes()).toContain('active')
  })

  it('has no button to rank what has only one place to be', async () => {
    const noColumnLevels = await mountGrid({
      shape: { ...SHAPE, columns: 1, columnLevels: 0 },
      columnNames: [],
      read: () =>
        Promise.resolve({
          ...WINDOW,
          columns: [{ node: 0, path: [], total: false, measure: 0 }],
          cells: [['3'], ['9'], ['12']],
        }),
    })
    expect(noColumnLevels.findAll('.pivot-label .pivot-sort')).toHaveLength(0)
    expect(noColumnLevels.findAll('.pivot-head .pivot-sort')).toHaveLength(1)
    noColumnLevels.unmount()

    const noRowLevels = await mountGrid({
      shape: { ...SHAPE, lines: 1, rowLevels: 0 },
      rowNames: [],
      read: () =>
        Promise.resolve({
          ...WINDOW,
          lines: [{ depth: 0, label: null, path: [], shared: 0 }],
          cells: [['5', '7', '12']],
        }),
    })
    expect(noRowLevels.findAll('.pivot-head .pivot-sort')).toHaveLength(0)
    expect(noRowLevels.findAll('.pivot-label .pivot-sort')).toHaveLength(1)
    noRowLevels.unmount()

    const noMeasures = await mountGrid({
      measureLabels: [],
      read: () =>
        Promise.resolve({
          ...WINDOW,
          columns: WINDOW.columns.map((column) => ({ ...column, measure: null })),
          cells: [
            [false, false, false],
            [false, false, false],
            [false, false, false],
          ],
        }),
    })
    expect(noMeasures.findAll('.pivot-head .pivot-sort')).toHaveLength(0)
    expect(noMeasures.findAll('.pivot-label .pivot-sort')).toHaveLength(0)
    expect(noMeasures.findAll('.pivot-corner .pivot-sort')).toHaveLength(2)
    wrapper = null
    noMeasures.unmount()
  })
})

describe('the window of PivotGrid', () => {
  it('keeps the previous window while a table of the same room is read', async () => {
    let answer: (window: PivotWindow) => void = () => undefined
    const grid = await mountGrid()
    const cells = () => grid.findAll('.pivot-cell').map((cell) => cell.text())
    expect(cells().slice(0, 3)).toEqual(['1', '2', '3'])

    await grid.setProps({
      table: 2,
      read: () => new Promise<PivotWindow>((resolve) => (answer = resolve)),
    })
    expect(cells().slice(0, 3)).toEqual(['1', '2', '3'])

    answer({
      ...WINDOW,
      cells: [
        ['4', '5', '9'],
        ['1', '2', '3'],
        ['5', '7', '12'],
      ],
    })
    await flushPromises()
    expect(cells().slice(0, 3)).toEqual(['4', '5', '9'])
  })

  it('leaves the previous window when the new table takes another room', async () => {
    const grid = await mountGrid()
    await grid.setProps({
      table: 2,
      shape: { ...SHAPE, lines: 5 },
      read: () => new Promise<PivotWindow>(() => undefined),
    })
    expect(grid.findAll('.pivot-cell')).toHaveLength(0)
  })
})

describe('the forms of the lines in PivotGrid', () => {
  /**
   * Table of two levels of rows, in a form that keeps the group above its members.
   */
  const TOWNS_SHAPE: PivotShape = { ...SHAPE, lines: 4, columns: 1, rowLevels: 2, columnLevels: 0 }

  /**
   * Whole window of that table, in outline form.
   */
  const TOWNS: PivotWindow = {
    lines: [
      { depth: 1, label: 'Nord', path: ['Nord'], shared: 0 },
      { depth: 2, label: 'Arras', path: ['Nord', 'Arras'], shared: 1 },
      { depth: 2, label: 'Lille', path: ['Nord', 'Lille'], shared: 1 },
      { depth: 0, label: null, path: [], shared: 0 },
    ],
    columns: [{ node: 0, path: [], total: false, measure: 0 }],
    cells: [['470'], ['100'], ['370'], ['470']],
  }

  /**
   * Labels of each line, column by column.
   *
   * @param grid Table mounted.
   */
  function labels(grid: VueWrapper): string[][] {
    return grid
      .findAll('.pivot-label-cells')
      .map((cells) => cells.findAll('.pivot-label-text').map((cell) => cell.text()))
  }

  /**
   * Mounts the table of the towns.
   *
   * @param props Properties that differ.
   */
  function mountTowns(props: Record<string, unknown> = {}) {
    return mountGrid({
      shape: TOWNS_SHAPE,
      rowNames: ['region', 'ville'],
      columnNames: [],
      read: () => Promise.resolve(TOWNS),
      ...props,
    })
  }

  it('indents one column of labels in compact form', async () => {
    const grid = await mountTowns()
    expect(labels(grid)).toEqual([['Nord'], ['Arras'], ['Lille'], ['Total']])
    const indents = grid
      .findAll<HTMLElement>('.pivot-label')
      .map((label) => label.element.style.getPropertyValue('--indent'))
    expect(indents).toEqual(['0', '1', '1', '0'])
  })

  it('gives each level its column in outline form', async () => {
    const grid = await mountTowns({ layout: { ...FIRST_LAYOUT, form: 'outline' } })
    expect(labels(grid)).toEqual([
      ['Nord', ''],
      ['', 'Arras'],
      ['', 'Lille'],
      ['Total', ''],
    ])
    const indents = grid
      .findAll<HTMLElement>('.pivot-label')
      .map((label) => label.element.style.getPropertyValue('--indent'))
    expect(indents).toEqual(['0', '0', '0', '0'])
    expect(grid.find('.pivot-corner-names').classes()).toContain('levels')
  })

  it('repeats the labels as asked, without reading the table again', async () => {
    const read = vi.fn<() => Promise<PivotWindow>>(() => Promise.resolve(TOWNS))
    const grid = await mountTowns({ layout: { ...FIRST_LAYOUT, form: 'outline' }, read })
    const reads = read.mock.calls.length
    await grid.setProps({ repeatLabels: true })
    expect(labels(grid)[1]).toEqual(['Nord', 'Arras'])
    expect(read).toHaveBeenCalledTimes(reads)
  })

  it('draws NULL in a column of its own, and an empty column without it', async () => {
    const window: PivotWindow = {
      ...TOWNS,
      lines: [
        { depth: 2, label: 'x', path: [null, 'x'], shared: 0 },
        { depth: 2, label: 'y', path: [null, 'y'], shared: 1 },
      ],
    }
    const grid = await mountTowns({
      layout: { ...FIRST_LAYOUT, form: 'tabular' },
      read: () => Promise.resolve(window),
    })
    const first = grid.findAll('.pivot-label-cells')[0]?.findAll('.pivot-label-text')
    expect(first?.[0]?.text()).toBe('NULL')
    expect(first?.[0]?.classes()).toContain('null')
    expect(labels(grid)[1]).toEqual(['', 'y'])
    expect(grid.findAll('.pivot-label-cells')[1]?.find('.null').exists()).toBe(false)
  })

  it('offers no sort on the line of a group without its subtotal', async () => {
    const window: PivotWindow = {
      ...TOWNS,
      columns: [{ node: 1, path: [[1, '2025']], total: false, measure: 0 }],
    }
    const grid = await mountTowns({
      shape: { ...TOWNS_SHAPE, columnLevels: 1 },
      layout: {
        ...FIRST_LAYOUT,
        form: 'outline',
        rows: { grandTotal: true, subtotals: [false, true] },
      },
      read: () => Promise.resolve(window),
    })
    const sorts = grid.findAll('.pivot-label').map((label) => label.find('.pivot-sort').exists())
    expect(sorts).toEqual([false, true, true, true])
  })

  it('keeps a single column of labels in place, several only with room for the values', async () => {
    const compact = await mountTowns()
    expect(compact.find('.pivot-scroller').classes()).toContain('frozen')
    compact.unmount()
    // The size of the table is not known yet, or too small.
    const outline = await mountTowns({ layout: { ...FIRST_LAYOUT, form: 'outline' } })
    expect(outline.find('.pivot-scroller').classes()).not.toContain('frozen')
  })
})
