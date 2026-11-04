import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { nextTick } from 'vue'

import PivotGrid from '../PivotGrid.vue'
import {
  FIRST_LAYOUT,
  LABEL_ORDER,
  type PivotOrder,
  type PivotShape,
  type PivotWindow,
} from '../pivotEngine'
import { targetKey } from '../pivotWidths'

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

/**
 * Lengths of the texts of its columns: the values of 2026 the longest, the total the longest
 * header.
 */
const LENGTHS = { values: [1, 30, 2], labels: [4, 4, 5] }

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
      lengths: LENGTHS,
      measureLabels: ['SUM(montant)'],
      rowNames: ['region'],
      columnNames: ['annee'],
      order: LABEL_ORDER,
      layout: FIRST_LAYOUT,
      repeatLabels: false,
      columnWidths: [],
      positions: new Map(),
      levelWidths: [],
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

describe('the lines of PivotGrid', () => {
  it('leaves the line on the left of the first column to the labels', async () => {
    const grid = await mountGrid()
    const cells = grid.findAll('.pivot-cell')
    const firsts = cells.filter((cell) => cell.classes('first'))
    expect(firsts.length).toBeGreaterThan(0)
    expect(cells.filter((cell) => cell.attributes('data-column') === '0')).toEqual(firsts)
    const heads = grid.findAll('.pivot-head[data-column="0"]')
    expect(heads.length).toBeGreaterThan(0)
    expect(heads.every((head) => head.classes('first'))).toBe(true)
    expect(grid.findAll('.pivot-head[data-column="1"].first')).toHaveLength(0)
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

describe('the widths in PivotGrid', () => {
  // jsdom has no pointer capture, used by a press on a handle.
  beforeEach(() => {
    HTMLElement.prototype.setPointerCapture = () => {}
  })

  /**
   * Columns of the grid of the first line.
   *
   * @param grid Table mounted.
   */
  function tracks(grid: VueWrapper): string {
    return grid.find<HTMLElement>('.pivot-body .pivot-line').element.style.gridTemplateColumns
  }

  /**
   * Moves the pointer on a handle: `trigger` of Vue Test Utils cannot give a position to an
   * event of jsdom.
   *
   * @param grid Table mounted.
   * @param selector Handle.
   * @param type Event.
   * @param clientX Position of the pointer, in pixels.
   */
  async function pointer(grid: VueWrapper, selector: string, type: string, clientX: number) {
    grid.find(selector).element.dispatchEvent(new MouseEvent(type, { bubbles: true, clientX }))
    await nextTick()
  }

  /**
   * Drags a handle by a distance, without releasing it.
   *
   * @param grid Table mounted.
   * @param selector Handle.
   * @param distance Move of the pointer, in pixels.
   */
  async function drag(grid: VueWrapper, selector: string, distance: number) {
    await pointer(grid, selector, 'pointerdown', 100)
    await pointer(grid, selector, 'pointermove', 100 + distance)
  }

  const COLUMN_2026 = { path: ['2026'], measure: 0 }

  it('draws a column with the width given to it, found by its values', async () => {
    const grid = await mountGrid({
      columnWidths: [{ ...COLUMN_2026, width: 50 }],
      positions: new Map([[targetKey(COLUMN_2026), 1]]),
    })
    // Without a stylesheet: characters of 7.2 pixels, 15 beside the text, steps of 3 pixels. The
    // column of 2025 fits its header and its sort button, (6 × 7.2 + 15) / 3 = 20 steps, that of
    // the total too, 22 steps; that of 2026 takes 50 steps in place of 77.
    expect(tracks(grid)).toContain(
      'calc(var(--spacing) * 20) calc(var(--spacing) * 50) calc(var(--spacing) * 22)',
    )
    expect(grid.find<HTMLElement>('.pivot-body').element.style.inlineSize).toContain(
      'var(--spacing) * 92',
    )
    await grid.setProps({ positions: new Map() })
    expect(tracks(grid)).toContain(
      'calc(var(--spacing) * 20) calc(var(--spacing) * 77) calc(var(--spacing) * 22)',
    )
  })

  it('follows the drag, and gives the width reached at the release', async () => {
    const grid = await mountGrid()
    expect(grid.findAll('.pivot-head .pivot-resizer')).toHaveLength(3)
    await drag(grid, '.pivot-head:nth-of-type(3) .pivot-resizer', 30)
    // 77 steps of 3 pixels by default, 231 pixels, plus 30.
    expect(tracks(grid)).toContain('calc(var(--spacing) * 87)')
    expect(grid.emitted('resizeColumn')).toBeUndefined()
    await grid.find('.pivot-head:nth-of-type(3) .pivot-resizer').trigger('pointerup')
    expect(grid.emitted('resizeColumn')).toEqual([[COLUMN_2026, 87]])
    // The width finds its column before the engine gives its position again.
    await grid.setProps({ columnWidths: [{ ...COLUMN_2026, width: 87 }] })
    expect(tracks(grid)).toContain('calc(var(--spacing) * 87)')
  })

  it('gives no width to a click without a move', async () => {
    const grid = await mountGrid()
    await pointer(grid, '.pivot-head .pivot-resizer', 'pointerdown', 100)
    await pointer(grid, '.pivot-head .pivot-resizer', 'pointerup', 100)
    expect(grid.emitted('resizeColumn')).toBeUndefined()
  })

  it('fits a column with a double click, then gives back its default width', async () => {
    const grid = await mountGrid()
    await grid.find('.pivot-head:nth-of-type(3) .pivot-resizer').trigger('dblclick')
    // Without a layout, each text takes the width it has: 77 steps.
    expect(grid.emitted('resizeColumn')).toEqual([[COLUMN_2026, 77]])
    await grid.setProps({ columnWidths: [{ ...COLUMN_2026, width: 77 }] })
    await grid.find('.pivot-head:nth-of-type(3) .pivot-resizer').trigger('dblclick')
    expect(grid.emitted('resizeColumn')?.[1]).toEqual([COLUMN_2026, null])
  })

  it('resizes the headers of a single column only, a measure included', async () => {
    const window: PivotWindow = {
      lines: [{ depth: 0, label: null, path: [], shared: 0 }],
      columns: [
        { node: 1, path: [[1, '2025']], total: false, measure: 0 },
        { node: 1, path: [[1, '2025']], total: false, measure: 1 },
      ],
      cells: [['1', '2']],
    }
    const grid = await mountGrid({
      shape: { ...SHAPE, lines: 1, columns: 2 },
      lengths: { values: [1, 1], labels: [4, 4] },
      measureLabels: ['SUM(montant)', 'COUNT(*)'],
      read: () => Promise.resolve(window),
    })
    expect(grid.find('.pivot-head.value .pivot-resizer').exists()).toBe(false)
    expect(grid.findAll('.pivot-head.measure .pivot-resizer')).toHaveLength(2)
    await drag(grid, '.pivot-head.measure:last-of-type .pivot-resizer', 3)
    await grid.find('.pivot-head.measure:last-of-type .pivot-resizer').trigger('pointerup')
    // COUNT(*) heads its column: 10 characters with its sort button, 29 steps, plus 3 pixels.
    expect(grid.emitted('resizeColumn')).toEqual([[{ path: ['2025'], measure: 1 }, 30]])
  })

  it('resizes the labels in a single column, or the column of a level', async () => {
    const compact = await mountGrid({ labelsWidth: 70 })
    expect(tracks(compact)).toMatch(/^calc\(var\(--spacing\) \* 70\) /)
    expect(compact.findAll('.pivot-corner .pivot-resizer')).toHaveLength(1)
    await drag(compact, '.pivot-corner .pivot-resizer', 30)
    await compact.find('.pivot-corner .pivot-resizer').trigger('pointerup')
    expect(compact.emitted('resizeLabels')).toEqual([[null, 80]])
    compact.unmount()

    const window: PivotWindow = {
      ...WINDOW,
      lines: [{ depth: 2, label: 'Arras', path: ['Nord', 'Arras'], shared: 0 }],
      cells: [['1', '2', '3']],
    }
    const outline = await mountGrid({
      shape: { ...SHAPE, lines: 1, rowLevels: 2 },
      rowNames: ['region', 'ville'],
      layout: { ...FIRST_LAYOUT, form: 'outline' },
      levelWidths: [40],
      read: () => Promise.resolve(window),
    })
    const cells = outline.find<HTMLElement>('.pivot-label-cells').element.style
    expect(cells.gridTemplateColumns).toBe('calc(var(--spacing) * 40) minmax(0, 1fr)')
    expect(outline.findAll('.pivot-corner .pivot-resizer')).toHaveLength(2)
    await drag(outline, '.pivot-corner .pivot-resizer:last-of-type', 30)
    await outline.find('.pivot-corner .pivot-resizer:last-of-type').trigger('pointerup')
    // The second level: 120 pixels by default, plus 30.
    expect(outline.emitted('resizeLabels')).toEqual([[1, 50]])
  })
})
