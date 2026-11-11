import { createPinia, setActivePinia } from 'pinia'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { mount } from '@vue/test-utils'
import { nextTick } from 'vue'

import type { ColumnTypeInfo } from '@/api/generated/ColumnTypeInfo'
import { NO_FILTER, type UserFilter } from '@/api/querySession'
import { useExplorerStore } from '@/stores/explorer'

import ResultGrid from '../ResultGrid.vue'

beforeEach(() => setActivePinia(createPinia()))

/**
 * Mounts the grid.
 *
 * @param columns Names of the columns.
 * @param rows Rows of the result.
 * @param sql Text of the query.
 * @param columnTypes Types of the columns, none by default.
 * @param filter User filter, none by default.
 */
function mountGrid(
  columns: string[],
  rows: (string | null)[][],
  sql = 'SELECT 1',
  columnTypes: ColumnTypeInfo[] = [],
  filter: UserFilter = NO_FILTER,
) {
  const rightAligned = columns.map(() => false)
  return mount(ResultGrid, {
    props: {
      columns,
      rightAligned,
      columnTypes,
      rows,
      rowCount: rows.length,
      sql,
      filter,
    },
  })
}

describe('ResultGrid', () => {
  it('shows the type of each column with an icon, its name on hover', () => {
    const wrapper = mountGrid(['id', 'name'], [['1', 'a']], 'SELECT id, name FROM t', [
      { family: 'integer', name: 'INT UNSIGNED' },
      { family: 'text', name: 'VARCHAR(20)' },
    ])
    // Integer: the "123" mark; text: the "Aa" icon.
    expect(wrapper.find('.grid-type-glyph').text()).toBe('123')
    expect(wrapper.findAll('.grid-type-icon')).toHaveLength(1)
    expect(wrapper.find('.grid-column-type').exists()).toBe(false)
    const titles = wrapper
      .findAll('.grid-column-name')
      .map((name) => name.attributes('data-tooltip'))
    expect(titles[1]).toContain('VARCHAR(20)')
  })

  it('selects everything with a click on the top left corner', async () => {
    const wrapper = mountGrid(['id', 'name'], [['1', 'a']])
    await wrapper.find('.grid-corner').trigger('mousedown')
    const headers = wrapper.findAll('.grid-column-header')
    expect(headers.map((header) => header.classes('selected'))).toEqual([true, true])
  })

  it('unselects a selected column with Ctrl, toggles columns with Alt', async () => {
    HTMLElement.prototype.setPointerCapture = () => {}
    const wrapper = mountGrid(['id', 'name', 'city'], [['1', 'a', 'b']])
    const selected = () =>
      wrapper.findAll('.grid-column-header').map((header) => header.classes('selected'))
    const names = wrapper.findAll('.grid-column-name')
    // jsdom has no PointerEvent: a mouse event of that name, keys set at its creation.
    const press = async (column: number, key: 'ctrlKey' | 'altKey') => {
      names[column]!.element.dispatchEvent(
        new MouseEvent('pointerdown', { bubbles: true, cancelable: true, [key]: true }),
      )
      await nextTick()
    }
    await wrapper.find('.grid-corner').trigger('mousedown')
    await press(1, 'ctrlKey')
    expect(selected()).toEqual([true, false, true])
    // Ctrl on an unselected column adds it back.
    await press(1, 'ctrlKey')
    expect(selected()).toEqual([true, true, true])
    await press(0, 'altKey')
    expect(selected()).toEqual([false, true, true])
    await press(0, 'altKey')
    expect(selected()).toEqual([true, true, true])
  })

  it('fits a column to its content, then gives back its estimated width', async () => {
    const wrapper = mountGrid(['texte'], [['x'.repeat(100)]], 'SELECT texte FROM double_clic')
    // The width of the only column: the template after the row numbers.
    const width = () =>
      (wrapper.find('.grid-header').element as HTMLElement).style.gridTemplateColumns.replace(
        /^max-content /,
        '',
      )
    const estimated = 'var(--grid-estimated-max-width)'
    const fitted = 'var(--grid-fitted-max-width)'
    expect(width()).toContain(estimated)
    await wrapper.find('.grid-resizer').trigger('dblclick')
    expect(width()).toContain(fitted)
    await wrapper.find('.grid-resizer').trigger('dblclick')
    expect(width()).toContain(estimated)
  })

  it('shows the headers', () => {
    const wrapper = mountGrid(
      ['id', 'name'],
      [
        ['1', 'a'],
        ['2', null],
      ],
    )
    const headers = wrapper.findAll('.grid-column-name').map((header) => header.text())
    expect(headers).toEqual(['id', 'name'])
  })

  it('shows the rows that its filter keeps, in all the columns or in one', async () => {
    const wrapper = mountGrid(
      ['id', 'name'],
      [
        ['1', 'a'],
        ['2', null],
      ],
    )
    expect(wrapper.vm.keptCount).toBe(2)
    await wrapper.setProps({ filter: { text: 'null', column: null } })
    expect(wrapper.vm.keptCount).toBe(1)
    await wrapper.setProps({ filter: { text: '1', column: 1 } })
    expect(wrapper.vm.keptCount).toBe(0)
    await wrapper.setProps({ filter: { text: '1', column: 0 } })
    expect(wrapper.vm.keptCount).toBe(1)
  })
})

describe('ResultGrid during a reception', () => {
  it('follows the rows received into the same array', async () => {
    const rows = [['1'], ['2']]
    const wrapper = mountGrid(['id'], rows)
    rows.push(['3'])
    await wrapper.setProps({ rowCount: rows.length })
    // The view grows in place: the grid must still see its new length.
    expect(wrapper.vm.keptCount).toBe(3)
    expect(wrapper.find('.grid-body').attributes('style')).toContain(
      'calc(var(--grid-row-height) * 3)',
    )
  })
})

describe('ResultGrid after a new result', () => {
  it('keeps the sort when the same columns come back, after a reconnection', async () => {
    const wrapper = mountGrid(['id'], [['2'], ['1']])
    await wrapper.find('.grid-sort').trigger('click')
    expect(wrapper.find('.grid-sort').text()).toBe('▲')
    await wrapper.setProps({ columns: ['id'], rows: [['2'], ['1']], rowCount: 2 })
    expect(wrapper.find('.grid-sort').text()).toBe('▲')
  })

  it('drops the sort when the columns change', async () => {
    const wrapper = mountGrid(['id'], [['2'], ['1']])
    await wrapper.find('.grid-sort').trigger('click')
    await wrapper.setProps({ columns: ['other'], rows: [['2'], ['1']], rowCount: 2 })
    expect(wrapper.find('.grid-sort').text()).toBe('⇅')
  })
})

describe('ResultGrid with a large result', () => {
  const rows = Array.from({ length: 5000 }, (_, index) => [`row ${index}`])

  /**
   * Number of rows of the view: the height of the list, at the theme's row height.
   *
   * @param wrapper Mounted grid.
   */
  function viewLength(wrapper: ReturnType<typeof mountGrid>): number {
    const style = wrapper.find('.grid-body').attributes('style') ?? ''
    return Number(/var\(--grid-row-height\) \* (\d+)/.exec(style)?.[1])
  }

  it('takes every row at once without a filter nor a sort', () => {
    expect(viewLength(mountGrid(['id'], rows))).toBe(5000)
  })

  it('computes a filtered view a chunk per frame, up to every kept row', async () => {
    const wrapper = mountGrid(['id'], rows, 'SELECT 1', [], { text: 'row', column: null })
    expect(viewLength(wrapper)).toBe(2000)
    await vi.waitFor(() => expect(viewLength(wrapper)).toBe(5000), { timeout: 5000 })
  })
})

describe('ResultGrid and the data explorer', () => {
  // jsdom has no pointer capture, used by a press on a column name.
  beforeEach(() => {
    HTMLElement.prototype.setPointerCapture = () => {}
  })

  const columns = ['id', 'doc']
  const rows = [
    ['1', '{"a":1}'],
    ['2', '{"b":2}'],
  ]
  const columnTypes: ColumnTypeInfo[] = [
    { family: 'integer', name: 'INT' },
    { family: 'text', name: 'LONGTEXT' },
  ]

  it('opens the explorer, and gives it the selected values of a JSON column', async () => {
    const explorer = useExplorerStore()
    const wrapper = mount(ResultGrid, {
      props: {
        columns,
        rightAligned: [true, false],
        columnTypes,
        rows,
        rowCount: 2,
        sql: 'SELECT id, doc FROM t',
        displayed: true,
        filter: NO_FILTER,
      },
    })
    explorer.layoutOf(0).open = true
    await nextTick()
    expect(explorer.contentOf(0)).toEqual({ state: 'none' })
    await wrapper.findAll('.grid-column-name')[1]!.trigger('pointerdown')
    expect(explorer.contentOf(0)).toEqual({
      state: 'values',
      column: 'doc',
      kind: 'json',
      cells: [
        { row: 1, value: '{"a":1}' },
        { row: 2, value: '{"b":2}' },
      ],
    })
    await wrapper.find('.grid-corner').trigger('mousedown')
    expect(explorer.contentOf(0)).toEqual({ state: 'several' })
    // Off screen (another tab, another editor): the explorer no longer shows it.
    await wrapper.setProps({ displayed: false })
    expect(explorer.contentOf(0)).toEqual({ state: 'none' })
  })

  it('takes a column of type JSON as JSON, scalars included', async () => {
    const explorer = useExplorerStore()
    explorer.layoutOf(0).open = true
    const wrapper = mount(ResultGrid, {
      props: {
        columns: ['value'],
        rightAligned: [false],
        columnTypes: [{ family: 'json', name: 'JSON' }],
        rows: [['1.50']],
        rowCount: 1,
        sql: 'SELECT JSON_EXTRACT(doc, "$.a") AS value FROM t',
        displayed: true,
        filter: NO_FILTER,
      },
    })
    // Without layout, jsdom draws no row: the column is selected by its name.
    await wrapper.find('.grid-column-name').trigger('pointerdown')
    expect(explorer.contentOf(0)).toEqual({
      state: 'values',
      column: 'value',
      kind: 'json',
      cells: [{ row: 1, value: '1.50' }],
    })
  })

  it('gives nothing while it is off screen', async () => {
    const explorer = useExplorerStore()
    explorer.layoutOf(0).open = true
    const wrapper = mount(ResultGrid, {
      props: {
        columns,
        rightAligned: [true, false],
        columnTypes,
        rows,
        rowCount: 2,
        sql: 'SELECT id, doc FROM t',
        filter: NO_FILTER,
      },
    })
    await wrapper.findAll('.grid-column-name')[0]!.trigger('pointerdown')
    expect(explorer.contentOf(0)).toEqual({ state: 'none' })
  })
})
