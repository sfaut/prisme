import { createPinia, setActivePinia } from 'pinia'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { useEditorDisplaysStore } from '../editorDisplays'
import { useExplorerStore } from '../explorer'

vi.mock('@/api/editors', () => ({ saveEditorDisplay: async () => {} }))

beforeEach(() => {
  window.localStorage.clear()
  setActivePinia(createPinia())
})

afterEach(() => {
  window.localStorage.clear()
})

describe('panel of an explorer', () => {
  it('starts closed, at the default width', () => {
    expect({ ...useExplorerStore().layoutOf(1) }).toEqual({ open: false, size: null })
  })

  it('is the panel of the display of its editor', () => {
    const explorer = useExplorerStore()
    explorer.layoutOf(1).open = true
    expect(useEditorDisplaysStore().displayOf(1).panels.explorer.open).toBe(true)
    expect(explorer.layoutOf(2).open).toBe(false)
  })

  it('forgets the content of the explorer of a closed editor', () => {
    const explorer = useExplorerStore()
    explorer.show(1, Symbol('grid'), { state: 'several' })

    explorer.forget(1)

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
