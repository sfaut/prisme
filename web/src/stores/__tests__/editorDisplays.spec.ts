import { createPinia, setActivePinia } from 'pinia'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { nextTick } from 'vue'

import type { EditorDisplay } from '@/api/generated/EditorDisplay'
import { SAVE_DELAY } from '@/utils/deferredSave'

import { defaultEditorDisplay, useEditorDisplaysStore } from '../editorDisplays'

/**
 * Saves of the displays sent to the server.
 */
const saveEditorDisplay = vi.hoisted(() =>
  vi.fn<(typeof import('@/api/editors'))['saveEditorDisplay']>(async () => {}),
)

vi.mock('@/api/editors', () => ({ saveEditorDisplay }))

beforeEach(() => {
  vi.useFakeTimers()
  saveEditorDisplay.mockClear()
  window.localStorage.clear()
  setActivePinia(createPinia())
})

afterEach(() => {
  vi.useRealTimers()
  window.localStorage.clear()
})

/**
 * Display saved by an editor.
 */
function saved(): EditorDisplay {
  const display = defaultEditorDisplay()
  display.panels.navigator = { open: false, size: 320 }
  display.panels.console = { open: false, size: 200 }
  display.navigator = {
    expanded: ['source:2'],
    scroll_top: 48,
    tables_by_rows: ['folder:tables:2/shop'],
  }
  return display
}

describe('display of an editor', () => {
  it('starts from the defaults without a saved display', () => {
    const displays = useEditorDisplaysStore()
    displays.seed(1, null)
    expect(displays.displayOf(1)).toEqual(defaultEditorDisplay())
    expect(displays.isSaved(1)).toBe(false)
    expect(displays.displayOf(2)).toEqual(defaultEditorDisplay())
  })

  it('starts from the display saved with its editor', () => {
    const displays = useEditorDisplaysStore()
    displays.seed(1, saved())
    expect(displays.displayOf(1)).toEqual(saved())
    expect(displays.isSaved(1)).toBe(true)
  })

  it('saves the whole display once, shortly after the last change', async () => {
    const displays = useEditorDisplaysStore()
    const display = displays.displayOf(1)
    display.panels.navigator = { open: false, size: 320 }
    display.panels.console.open = false
    display.panels.console.size = 200
    display.navigator.expanded = ['source:2']
    display.navigator.tables_by_rows = ['folder:tables:2/shop']
    await nextTick()
    display.navigator.scroll_top = 48
    await nextTick()
    expect(saveEditorDisplay).not.toHaveBeenCalled()

    vi.advanceTimersByTime(SAVE_DELAY)

    expect(saveEditorDisplay).toHaveBeenCalledTimes(1)
    expect(saveEditorDisplay).toHaveBeenCalledWith(1, saved(), { keepalive: false })
  })

  it('saves a waiting display at once before its editor closes, then leaves it', async () => {
    const displays = useEditorDisplaysStore()
    displays.displayOf(1).panels.navigator.size = 320
    await nextTick()

    await displays.flush(1)
    expect(saveEditorDisplay).toHaveBeenCalledTimes(1)
    displays.forget(1)
    vi.advanceTimersByTime(SAVE_DELAY)
    expect(saveEditorDisplay).toHaveBeenCalledTimes(1)

    // Reopened, the editor hands over its saved display again.
    displays.seed(1, saved())
    expect(displays.displayOf(1).panels.navigator.size).toBe(320)
  })

  it('removes the displays once kept in the browser', () => {
    window.localStorage.setItem('prisme.navigator.tree.1', '{}')
    window.localStorage.setItem('prisme.editor.layout.1', '{}')
    window.localStorage.setItem('prisme.explorer.layout.1', '{}')
    window.localStorage.setItem('prisme.editor.active', '1')
    useEditorDisplaysStore()
    expect(Object.keys(window.localStorage)).toEqual(['prisme.editor.active'])
  })

  it('resets the panels and the navigator in place, the SQL position kept', () => {
    const store = useEditorDisplaysStore()
    store.seed(1, saved())
    const display = store.displayOf(1)
    const tree = display.navigator
    display.sql = { anchor: 4, head: 4, top: 0, folds: [] }
    const listener = vi.fn<() => void>()
    const stop = store.onReset(1, listener)

    store.reset(1, 2)
    const fresh = defaultEditorDisplay()
    expect(display.panels).toEqual(fresh.panels)
    expect(display.navigator).toEqual({ ...fresh.navigator, expanded: ['source:2'] })
    expect(display.navigator).toBe(tree)
    expect(display.sql.anchor).toBe(4)
    expect(listener).toHaveBeenCalledOnce()

    stop()
    store.reset(1, null)
    expect(display.navigator.expanded).toEqual([])
    expect(listener).toHaveBeenCalledOnce()
  })
})
