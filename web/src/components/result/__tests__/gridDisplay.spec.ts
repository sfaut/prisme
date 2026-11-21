import { describe, expect, it } from 'vitest'

import { type GridLayout, type QueryTab, NO_FILTER } from '@/api/querySession'

import { gridDocument, startingLayout } from '../gridDisplay'

/**
 * Layout saved on the columns `a`, `b`, `c`.
 */
const SAVED: GridLayout = {
  columns: ['a', 'b', 'c'],
  widths: [null, '12rem', null],
  order: [2, 0, 1],
  sort: { column: 1, direction: 'desc' },
  scroll_top: 220,
  scroll_left: 40,
}

describe('startingLayout', () => {
  it('opens with the saved layout on the same columns', () => {
    expect(startingLayout(SAVED, ['a', 'b', 'c'])).toEqual({
      widths: { 1: '12rem' },
      order: [2, 0, 1],
      sort: { column: 1, direction: 'desc' },
      scroll: { top: 220, left: 40 },
    })
  })

  it('opens with the defaults on other columns, or without a saved layout', () => {
    const defaults = { widths: {}, order: [0, 1], sort: null, scroll: { top: 0, left: 0 } }
    expect(startingLayout(SAVED, ['a', 'b'])).toEqual(defaults)
    expect(startingLayout(null, ['a', 'b'])).toEqual(defaults)
  })

  it('drops an order that is not one of the columns', () => {
    expect(startingLayout({ ...SAVED, order: [0, 0, 1] }, ['a', 'b', 'c']).order).toEqual([0, 1, 2])
  })
})

describe('gridDocument', () => {
  it('joins the layout of the grid and the filter of the tab', () => {
    const tab = {
      grid: SAVED,
      filter: { text: 'paris', column: 0 },
      filterOpen: true,
      result: null,
    } as unknown as QueryTab
    expect(gridDocument(tab)).toEqual({
      ...SAVED,
      filter: { text: 'paris', column: 0 },
      filter_open: true,
    })
  })

  it('saves the defaults on the columns of the result without a layout', () => {
    const tab = {
      grid: null,
      filter: NO_FILTER,
      filterOpen: true,
      result: { columns: ['x'] },
    } as unknown as QueryTab
    expect(gridDocument(tab)).toEqual({
      columns: ['x'],
      widths: [],
      order: [],
      sort: null,
      scroll_top: 0,
      scroll_left: 0,
      filter: { text: '', column: null },
      filter_open: true,
    })
  })
})
