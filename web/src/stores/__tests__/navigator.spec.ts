import { createPinia, setActivePinia } from 'pinia'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { nextTick } from 'vue'

import { defaultEditorDisplay, useEditorDisplaysStore } from '../editorDisplays'
import { useNavigatorStore } from '../navigator'

vi.mock('@/api/editors', () => ({ saveEditorDisplay: async () => {} }))

beforeEach(() => {
  setActivePinia(createPinia())
})

describe('navigator of an editor', () => {
  it('is the navigator of the display of its editor', () => {
    const displays = useEditorDisplaysStore()
    const navigator = useNavigatorStore()
    navigator.layoutOf(1).open = false
    navigator.scrollTo(1, 48)
    expect(displays.displayOf(1).panels.navigator.open).toBe(false)
    expect(displays.displayOf(1).navigator.scroll_top).toBe(48)
  })

  it('is new until its display is saved or changed', async () => {
    const displays = useEditorDisplaysStore()
    const navigator = useNavigatorStore()
    displays.seed(2, defaultEditorDisplay())
    expect(navigator.isNew(1)).toBe(true)
    expect(navigator.isNew(2)).toBe(false)

    navigator.treeOf(1).expanded = ['sources', 'source:1']
    await nextTick()
    expect(navigator.isNew(1)).toBe(false)
  })

  it('sorts a "Tables" folder by row count, then by name again', () => {
    const navigator = useNavigatorStore()
    navigator.sortTables(1, 'folder:tables:1/shop', 'rows')
    navigator.sortTables(1, 'folder:tables:1/crm', 'rows')
    navigator.sortTables(1, 'folder:tables:1/shop', 'rows')
    expect(navigator.treeOf(1).tables_by_rows).toEqual([
      'folder:tables:1/shop',
      'folder:tables:1/crm',
    ])

    navigator.sortTables(1, 'folder:tables:1/shop', 'name')
    expect(navigator.treeOf(1).tables_by_rows).toEqual(['folder:tables:1/crm'])
  })

  it('closes a deleted source and the nodes under it, and forgets its sorts, in every navigator', () => {
    const navigator = useNavigatorStore()
    navigator.treeOf(1).expanded = ['sources', 'source:1', 'schema:1/shop', 'source:11']
    navigator.treeOf(2).expanded = ['sources', 'table:1/shop.customer', 'schema:2/crm']
    navigator.sortTables(2, 'folder:tables:1/shop', 'rows')
    navigator.sortTables(2, 'folder:tables:2/crm', 'rows')

    navigator.forgetSource(1)

    expect(navigator.treeOf(1).expanded).toEqual(['sources', 'source:11'])
    expect(navigator.treeOf(2).expanded).toEqual(['sources', 'schema:2/crm'])
    expect(navigator.treeOf(2).tables_by_rows).toEqual(['folder:tables:2/crm'])
  })
})
