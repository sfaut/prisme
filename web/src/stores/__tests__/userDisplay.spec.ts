import { createPinia, setActivePinia } from 'pinia'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { nextTick } from 'vue'

import type { UserDisplay } from '@/api/generated/UserDisplay'
import { colorMode } from '@/utils/colorMode'
import { SAVE_DELAY } from '@/utils/deferredSave'

import { defaultUserDisplay, useUserDisplayStore } from '../userDisplay'

/**
 * Display settings read from the server.
 */
const getUserDisplay = vi.hoisted(() => vi.fn<(typeof import('@/api/display'))['getUserDisplay']>())

/**
 * Saves of the display settings sent to the server.
 */
const saveUserDisplay = vi.hoisted(() =>
  vi.fn<(typeof import('@/api/display'))['saveUserDisplay']>(async () => {}),
)

vi.mock('@/api/display', () => ({ getUserDisplay, saveUserDisplay }))

/**
 * Display settings saved by the user.
 */
function saved(): UserDisplay {
  const display = defaultUserDisplay()
  display.theme = 'dark'
  display.active_editor = 4
  display.console.tab = 'debug'
  display.explorer.wrap = true
  return display
}

beforeEach(() => {
  vi.useFakeTimers()
  getUserDisplay.mockReset()
  saveUserDisplay.mockClear()
  colorMode.value = 'system'
  window.localStorage.clear()
  setActivePinia(createPinia())
})

afterEach(() => {
  // The theme of the page is common to the tests: the store lets it go.
  useUserDisplayStore().$dispose()
  vi.useRealTimers()
  colorMode.value = 'system'
  window.localStorage.clear()
})

describe('display settings of the user', () => {
  it('reads the settings, applies their theme, and saves nothing for the reading', async () => {
    getUserDisplay.mockResolvedValueOnce(saved())
    const store = useUserDisplayStore()
    await store.load()
    await nextTick()

    expect(store.display).toEqual(saved())
    expect(colorMode.value).toBe('dark')
    expect(window.localStorage.getItem('prisme.app.colorMode')).toBe('dark')
    vi.advanceTimersByTime(SAVE_DELAY)
    expect(saveUserDisplay).not.toHaveBeenCalled()
  })

  it('saves the whole settings shortly after a change, the theme of the page included', async () => {
    getUserDisplay.mockResolvedValueOnce(defaultUserDisplay())
    const store = useUserDisplayStore()
    await store.load()
    store.display.console.all_editors = true
    colorMode.value = 'light'
    await nextTick()

    vi.advanceTimersByTime(SAVE_DELAY)
    expect(saveUserDisplay).toHaveBeenCalledTimes(1)
    expect(saveUserDisplay.mock.calls[0]![0]).toMatchObject({
      theme: 'light',
      console: { all_editors: true },
    })
  })

  it('keeps the defaults and saves nothing when the settings cannot be read', async () => {
    getUserDisplay.mockRejectedValueOnce(new Error('offline'))
    const store = useUserDisplayStore()
    await store.load()
    store.display.explorer.wrap = true
    await nextTick()

    vi.advanceTimersByTime(SAVE_DELAY)
    expect(store.loaded).toBe(false)
    expect(saveUserDisplay).not.toHaveBeenCalled()
  })

  it('removes the settings once kept in the browser, but the copy of the theme', () => {
    for (const key of [
      'prisme.editor.active',
      'prisme.console.tab',
      'prisme.console.historyFilter',
      'prisme.explorer.wrap',
      'prisme.app.colorMode',
    ]) {
      window.localStorage.setItem(key, 'x')
    }
    useUserDisplayStore()
    expect(Object.keys(window.localStorage)).toEqual(['prisme.app.colorMode'])
  })
})
