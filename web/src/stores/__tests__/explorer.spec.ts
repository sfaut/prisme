import { createPinia, setActivePinia } from 'pinia'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { nextTick } from 'vue'

import { useExplorerStore } from '../explorer'

beforeEach(() => {
  window.localStorage.clear()
  setActivePinia(createPinia())
})

afterEach(() => {
  window.localStorage.clear()
})

describe('layout of an explorer', () => {
  it('starts closed, at the default width, without anything kept', () => {
    expect({ ...useExplorerStore().layoutOf(1) }).toEqual({ open: false, width: null })
  })

  it('is kept per editor, and the last width given starts a new editor, closed', async () => {
    const explorer = useExplorerStore()
    explorer.layoutOf(1).open = true
    explorer.layoutOf(1).width = 480
    await nextTick()

    expect(JSON.parse(window.localStorage.getItem('prisme.explorer.layout.1')!)).toEqual({
      open: true,
      width: 480,
    })
    expect({ ...explorer.layoutOf(2) }).toEqual({ open: false, width: 480 })
    setActivePinia(createPinia())
    expect({ ...useExplorerStore().layoutOf(1) }).toEqual({ open: true, width: 480 })
  })

  it('stays as first read, whatever width is given later to another editor', async () => {
    const explorer = useExplorerStore()
    expect(explorer.layoutOf(1).width).toBeNull()
    explorer.layoutOf(2).width = 480
    await nextTick()

    setActivePinia(createPinia())
    expect(useExplorerStore().layoutOf(1).width).toBeNull()
    expect(useExplorerStore().layoutOf(3).width).toBe(480)
  })

  it('forgets the explorer of a closed editor', async () => {
    const explorer = useExplorerStore()
    explorer.layoutOf(1).open = true
    await nextTick()
    explorer.show(1, Symbol('grid'), { state: 'several' })

    explorer.forget(1)

    expect(window.localStorage.getItem('prisme.explorer.layout.1')).toBeNull()
    expect(explorer.layoutOf(1).open).toBe(false)
    expect(explorer.contentOf(1)).toEqual({ state: 'none' })
  })
})

describe('content of an explorer', () => {
  it('is that of the grid of its editor, withdrawn only by that grid', () => {
    const explorer = useExplorerStore()
    const first = Symbol('first grid')
    const second = Symbol('second grid')
    explorer.show(1, first, { state: 'several' })
    explorer.show(2, second, { state: 'scalar', column: 'name' })

    expect(explorer.contentOf(1)).toEqual({ state: 'several' })
    expect(explorer.contentOf(2)).toEqual({ state: 'scalar', column: 'name' })

    explorer.release(1, second)
    expect(explorer.contentOf(1)).toEqual({ state: 'several' })
    explorer.release(1, first)
    expect(explorer.contentOf(1)).toEqual({ state: 'none' })
    expect(explorer.contentOf(2)).toEqual({ state: 'scalar', column: 'name' })
  })
})
