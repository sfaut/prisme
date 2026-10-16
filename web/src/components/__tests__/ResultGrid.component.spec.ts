import { describe, expect, it, vi } from 'vitest'

import { mount } from '@vue/test-utils'

import type { ColumnTypeInfo } from '@/api/generated/ColumnTypeInfo'

import ResultGrid from '../ResultGrid.vue'

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
    expect(wrapper.find('.grid-filter').exists()).toBe(false)
    await wrapper.find('.grid-filter-toggle').trigger('click')
    await wrapper.find('.grid-filter input').setValue('null')
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

describe('ResultGrid filter', () => {
  it('keeps the filter text when the same columns come back', async () => {
    const wrapper = mountGrid(['id'], [['1'], ['2']])
    await wrapper.find('.grid-filter-toggle').trigger('click')
    await wrapper.find('.grid-filter input').setValue('2')
    await wrapper.setProps({ columns: ['id'], rows: [['1'], ['2']], rowCount: 2 })
    expect((wrapper.find('.grid-filter input').element as HTMLInputElement).value).toBe('2')
  })
})
