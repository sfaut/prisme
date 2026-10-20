import { createPinia, setActivePinia } from 'pinia'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { mount } from '@vue/test-utils'
import { nextTick } from 'vue'

import type { ColumnTypeInfo } from '@/api/generated/ColumnTypeInfo'
import { useDataExplorerStore } from '@/stores/dataExplorer'

import ResultGrid from '../ResultGrid.vue'

beforeEach(() => setActivePinia(createPinia()))

/**
 * Mounts the grid.
 *
 * @param columns Names of the columns.
 * @param rows Rows of the result.
 * @param sql Text of the query.
 * @param columnTypes Types of the columns, none by default.
 */
function mountGrid(
  columns: string[],
  rows: (string | null)[][],
  sql = 'SELECT 1',
  columnTypes: ColumnTypeInfo[] = [],
) {
  const rightAligned = columns.map(() => false)
  return mount(ResultGrid, {
    props: { columns, rightAligned, columnTypes, rows, rowCount: rows.length, sql },
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
    const titles = wrapper.findAll('.grid-column-name').map((name) => name.attributes('title'))
    expect(titles[1]).toContain('VARCHAR(20)')
  })

  it('selects everything with a click on the top left corner', async () => {
    const wrapper = mountGrid(['id', 'name'], [['1', 'a']])
    await wrapper.find('.grid-corner').trigger('mousedown')
    const headers = wrapper.findAll('.grid-column-header')
    expect(headers.map((header) => header.classes('selected'))).toEqual([true, true])
  })

  it('fits a column to its content, then gives back its estimated width', async () => {
    const wrapper = mountGrid(['texte'], [['x'.repeat(100)]], 'SELECT texte FROM double_clic')
    const width = () =>
      (wrapper.find('.grid-header').element as HTMLElement).style.gridTemplateColumns.split(' ')[1]
    expect(width()).toBe('360px')
    await wrapper.find('.grid-resizer').trigger('dblclick')
    expect(width()).toBe('716px')
    await wrapper.find('.grid-resizer').trigger('dblclick')
    expect(width()).toBe('360px')
  })

  it('shows the headers and the number of rows', () => {
    const wrapper = mountGrid(
      ['id', 'name'],
      [
        ['1', 'a'],
        ['2', null],
      ],
    )
    const headers = wrapper.findAll('.grid-column-name').map((header) => header.text())
    expect(headers).toEqual(['id', 'name'])
    expect(wrapper.find('.grid-count').text()).toBe('2 lignes')
  })

  it('filters the rows', async () => {
    const wrapper = mountGrid(
      ['id', 'name'],
      [
        ['1', 'a'],
        ['2', null],
      ],
    )
    // Collapsed at first: the button of the bottom bar expands the form.
    expect(wrapper.find('.grid-user-filter').exists()).toBe(false)
    await wrapper.find('.grid-user-filter-toggle').trigger('click')
    await wrapper.find('.grid-user-filter input').setValue('null')
    expect(wrapper.find('.grid-count').text()).toBe('1 / 2 lignes')
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
  it('ends with every row in the view, computed a chunk per frame', async () => {
    const rows = Array.from({ length: 5000 }, (_, index) => [String(index)])
    const wrapper = mountGrid(['id'], rows)
    // The height of the list is the number of rows of the view, at 22 px each (default).
    await vi.waitFor(
      () => expect(wrapper.find('.grid-body').attributes('style')).toContain('height: 110000px'),
      { timeout: 5000 },
    )
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
    const explorer = useDataExplorerStore()
    const wrapper = mount(ResultGrid, {
      props: {
        columns,
        rightAligned: [true, false],
        columnTypes,
        rows,
        rowCount: 2,
        sql: 'SELECT id, doc FROM t',
        displayed: true,
      },
    })
    explorer.open = true
    await nextTick()
    expect(explorer.content).toEqual({ state: 'none' })
    await wrapper.findAll('.grid-column-name')[1]!.trigger('pointerdown')
    expect(explorer.content).toEqual({
      state: 'values',
      column: 'doc',
      kind: 'json',
      cells: [
        { row: 1, value: '{"a":1}' },
        { row: 2, value: '{"b":2}' },
      ],
    })
    await wrapper.find('.grid-corner').trigger('mousedown')
    expect(explorer.content).toEqual({ state: 'several' })
    // Off screen (another tab, another editor): the explorer no longer shows it.
    await wrapper.setProps({ displayed: false })
    expect(explorer.content).toEqual({ state: 'none' })
  })

  it('takes a column of type JSON as JSON, scalars included', async () => {
    const explorer = useDataExplorerStore()
    explorer.open = true
    const wrapper = mount(ResultGrid, {
      props: {
        columns: ['value'],
        rightAligned: [false],
        columnTypes: [{ family: 'json', name: 'JSON' }],
        rows: [['1.50']],
        rowCount: 1,
        sql: 'SELECT JSON_EXTRACT(doc, "$.a") AS value FROM t',
        displayed: true,
      },
    })
    // Without layout, jsdom draws no row: the column is selected by its name.
    await wrapper.find('.grid-column-name').trigger('pointerdown')
    expect(explorer.content).toEqual({
      state: 'values',
      column: 'value',
      kind: 'json',
      cells: [{ row: 1, value: '1.50' }],
    })
  })

  it('gives nothing while it is off screen', async () => {
    const explorer = useDataExplorerStore()
    explorer.open = true
    const wrapper = mount(ResultGrid, {
      props: {
        columns,
        rightAligned: [true, false],
        columnTypes,
        rows,
        rowCount: 2,
        sql: 'SELECT id, doc FROM t',
      },
    })
    await wrapper.findAll('.grid-column-name')[0]!.trigger('pointerdown')
    expect(explorer.content).toEqual({ state: 'none' })
  })
})

describe('ResultGrid filter', () => {
  it('keeps the filter text when the same columns come back', async () => {
    const wrapper = mountGrid(['id'], [['1'], ['2']])
    await wrapper.find('.grid-user-filter-toggle').trigger('click')
    await wrapper.find('.grid-user-filter input').setValue('2')
    await wrapper.setProps({ columns: ['id'], rows: [['1'], ['2']], rowCount: 2 })
    expect((wrapper.find('.grid-user-filter input').element as HTMLInputElement).value).toBe('2')
  })
})
