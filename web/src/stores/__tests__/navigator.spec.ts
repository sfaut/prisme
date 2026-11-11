import { createPinia, setActivePinia } from 'pinia'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { nextTick } from 'vue'

import { useNavigatorStore } from '../navigator'

beforeEach(() => {
  window.localStorage.clear()
  setActivePinia(createPinia())
})

afterEach(() => {
  vi.useRealTimers()
  window.localStorage.clear()
})

describe('layout of a navigator', () => {
  it('starts open, at the default width, without anything kept', () => {
    expect({ ...useNavigatorStore().layoutOf(1) }).toEqual({ open: true, width: null })
  })

  it('is kept per editor, and the last one given starts a new editor', async () => {
    const navigator = useNavigatorStore()
    navigator.layoutOf(1).open = false
    navigator.layoutOf(1).width = 320
    await nextTick()

    expect(JSON.parse(window.localStorage.getItem('prisme.navigator.layout.1')!)).toEqual({
      open: false,
      width: 320,
    })
    expect({ ...navigator.layoutOf(2) }).toEqual({ open: false, width: 320 })

    // An editor with a layout of its own keeps it.
    navigator.layoutOf(2).open = true
    await nextTick()
    setActivePinia(createPinia())
    expect({ ...useNavigatorStore().layoutOf(1) }).toEqual({ open: false, width: 320 })
  })

  it('stays as first read, whatever layout is given later to another editor', async () => {
    const navigator = useNavigatorStore()
    expect(navigator.layoutOf(1).open).toBe(true)
    navigator.layoutOf(2).open = false
    await nextTick()

    setActivePinia(createPinia())
    expect(useNavigatorStore().layoutOf(1).open).toBe(true)
    expect(useNavigatorStore().layoutOf(3).open).toBe(false)
  })
})

describe('tree of a navigator', () => {
  it('starts with the root of the sources open, as a new tree', () => {
    expect({ ...useNavigatorStore().treeOf(1) }).toEqual({
      expanded: ['sources'],
      scrollTop: 0,
      known: false,
    })
  })

  it('keeps the open nodes at once, and the scroll after its delay', async () => {
    vi.useFakeTimers()
    const navigator = useNavigatorStore()
    navigator.treeOf(1).expanded = ['sources', 'source:2']
    await nextTick()
    navigator.scrollTo(1, 48)
    expect(JSON.parse(window.localStorage.getItem('prisme.navigator.tree.1')!)).toEqual({
      expanded: ['sources', 'source:2'],
      scrollTop: 0,
    })
    vi.advanceTimersByTime(300)
    expect(JSON.parse(window.localStorage.getItem('prisme.navigator.tree.1')!).scrollTop).toBe(48)

    setActivePinia(createPinia())
    expect({ ...useNavigatorStore().treeOf(1) }).toEqual({
      expanded: ['sources', 'source:2'],
      scrollTop: 48,
      known: true,
    })
  })

  it('ignores a kept tree that is unreadable', () => {
    window.localStorage.setItem('prisme.navigator.tree.1', '{"expanded":[1],"scrollTop":0}')
    window.localStorage.setItem('prisme.navigator.tree.2', '{"expanded":[],"scrollTop":-1}')
    window.localStorage.setItem('prisme.navigator.tree.3', 'not json')
    const navigator = useNavigatorStore()
    for (const editorId of [1, 2, 3]) {
      expect(navigator.treeOf(editorId).known).toBe(false)
    }
  })

  it('closes a deleted source and the nodes under it, in every navigator', () => {
    const navigator = useNavigatorStore()
    navigator.treeOf(1).expanded = ['sources', 'source:1', 'schema:1/shop', 'source:11']
    navigator.treeOf(2).expanded = ['sources', 'table:1/shop.customer', 'schema:2/crm']

    navigator.forgetSource(1)

    expect(navigator.treeOf(1).expanded).toEqual(['sources', 'source:11'])
    expect(navigator.treeOf(2).expanded).toEqual(['sources', 'schema:2/crm'])
  })

  it('forgets the navigator of a closed editor', async () => {
    const navigator = useNavigatorStore()
    navigator.layoutOf(1).width = 320
    navigator.treeOf(1).expanded = ['sources', 'source:1']
    await nextTick()

    navigator.forget(1)

    expect(window.localStorage.getItem('prisme.navigator.layout.1')).toBeNull()
    expect(window.localStorage.getItem('prisme.navigator.tree.1')).toBeNull()
    expect(navigator.treeOf(1).known).toBe(false)
  })
})
