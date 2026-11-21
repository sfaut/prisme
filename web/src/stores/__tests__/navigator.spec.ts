import { createPinia, setActivePinia } from 'pinia'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { nextTick } from 'vue'

import { useNavigatorStore } from '../navigator'

/**
 * Saves of the navigators' states sent to the server.
 */
const saveNavigator = vi.hoisted(() =>
  vi.fn<(typeof import('@/api/editors'))['saveNavigator']>(async () => {}),
)

vi.mock('@/api/editors', () => ({ saveNavigator }))

beforeEach(() => {
  vi.useFakeTimers()
  saveNavigator.mockClear()
  window.localStorage.clear()
  setActivePinia(createPinia())
})

afterEach(() => {
  vi.useRealTimers()
  window.localStorage.clear()
})

/**
 * State saved by the navigator of an editor.
 */
const SAVED = {
  open: false,
  width: 320,
  expanded: ['sources', 'source:2'],
  scroll_top: 48,
}

describe('state of a navigator', () => {
  it('starts from the defaults without a saved state', () => {
    const navigator = useNavigatorStore()
    navigator.seed(1, null)
    expect({ ...navigator.layoutOf(1) }).toEqual({ open: true, width: null })
    expect({ ...navigator.treeOf(1) }).toEqual({
      expanded: ['sources'],
      scrollTop: 0,
      known: false,
    })
    expect({ ...navigator.layoutOf(2) }).toEqual({ open: true, width: null })
  })

  it('starts from the state saved with its editor', () => {
    const navigator = useNavigatorStore()
    navigator.seed(1, SAVED)
    expect({ ...navigator.layoutOf(1) }).toEqual({ open: false, width: 320 })
    expect({ ...navigator.treeOf(1) }).toEqual({
      expanded: ['sources', 'source:2'],
      scrollTop: 48,
      known: true,
    })
  })

  it('saves the whole state once, shortly after the last change', async () => {
    const navigator = useNavigatorStore()
    navigator.layoutOf(1).open = false
    navigator.layoutOf(1).width = 320
    navigator.treeOf(1).expanded = ['sources', 'source:2']
    await nextTick()
    navigator.scrollTo(1, 48)
    expect(saveNavigator).not.toHaveBeenCalled()

    vi.advanceTimersByTime(300)

    expect(saveNavigator).toHaveBeenCalledTimes(1)
    expect(saveNavigator).toHaveBeenCalledWith(1, SAVED, { keepalive: false })
  })

  it('saves a waiting state at once before its editor closes, then leaves it', async () => {
    const navigator = useNavigatorStore()
    navigator.layoutOf(1).width = 320
    await nextTick()

    await navigator.flush(1)
    expect(saveNavigator).toHaveBeenCalledTimes(1)
    navigator.forget(1)
    vi.advanceTimersByTime(300)
    expect(saveNavigator).toHaveBeenCalledTimes(1)

    // Reopened, the editor hands over its saved state again.
    navigator.seed(1, SAVED)
    expect(navigator.layoutOf(1).width).toBe(320)
    expect(navigator.treeOf(1).known).toBe(true)
  })

  it('removes the navigators once kept in the browser', () => {
    window.localStorage.setItem('prisme.navigator.tree.1', '{}')
    window.localStorage.setItem('prisme.navigator.layout.1', '{}')
    window.localStorage.setItem('prisme.editor.active', '1')
    useNavigatorStore()
    expect(Object.keys(window.localStorage)).toEqual(['prisme.editor.active'])
  })

  it('closes a deleted source and the nodes under it, in every navigator', () => {
    const navigator = useNavigatorStore()
    navigator.treeOf(1).expanded = ['sources', 'source:1', 'schema:1/shop', 'source:11']
    navigator.treeOf(2).expanded = ['sources', 'table:1/shop.customer', 'schema:2/crm']

    navigator.forgetSource(1)

    expect(navigator.treeOf(1).expanded).toEqual(['sources', 'source:11'])
    expect(navigator.treeOf(2).expanded).toEqual(['sources', 'schema:2/crm'])
  })
})
