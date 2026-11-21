import { createPinia, setActivePinia } from 'pinia'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { nextTick } from 'vue'

import { syncState } from '@/api/sync'
import { useEditorsStore } from '../editors'
import { useNavigatorStore } from '../navigator'

/**
 * Answers the next save with `status` (204 accepted, 500 refused).
 */
let saveStatus = 204

beforeEach(() => {
  setActivePinia(createPinia())
  saveStatus = 204
  vi.stubGlobal(
    'fetch',
    vi.fn(async (_path: string, init: RequestInit) => {
      if (init.method === 'POST') {
        return new Response(
          JSON.stringify({ id: 1, name: '', connection_id: null, sql: '', navigator: null }),
        )
      }
      if (saveStatus === 204) {
        return new Response(null, { status: 204 })
      }
      return new Response('{"error":"refused"}', { status: saveStatus })
    }),
  )
})

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('synchronisation state of the editors', () => {
  it('is writing after a change, then synced once the server has saved it', async () => {
    const editors = useEditorsStore()
    await editors.create(null)
    expect(syncState.value).toBe('synced')

    editors.setSql(1, 'SELECT 1')
    expect(syncState.value).toBe('writing')

    await vi.waitFor(() => expect(syncState.value).toBe('synced'))
  })

  it('is failed after a refused save, until a later save succeeds', async () => {
    const editors = useEditorsStore()
    await editors.create(null)

    saveStatus = 500
    editors.setSql(1, 'SELECT 1')
    await vi.waitFor(() => expect(syncState.value).toBe('failed'))

    saveStatus = 204
    editors.setSql(1, 'SELECT 2')
    await vi.waitFor(() => expect(syncState.value).toBe('synced'))
  })
})

describe('closing an editor', () => {
  it('forgets the layout of its cell', async () => {
    const editors = useEditorsStore()
    await editors.create(null)
    window.localStorage.setItem('prisme.editor.layout.1', '{}')
    window.localStorage.setItem('prisme.editor.layout.2', '{}')

    await editors.close(1)

    expect(window.localStorage.getItem('prisme.editor.layout.1')).toBeNull()
    expect(window.localStorage.getItem('prisme.editor.layout.2')).toBe('{}')
    window.localStorage.clear()
  })

  it('forgets its explorer', async () => {
    const editors = useEditorsStore()
    await editors.create(null)
    window.localStorage.setItem('prisme.explorer.layout.1', '{}')

    await editors.close(1)

    expect(window.localStorage.getItem('prisme.explorer.layout.1')).toBeNull()
    window.localStorage.clear()
  })

  it('saves its navigator before closing, which keeps it for a reopening', async () => {
    const editors = useEditorsStore()
    await editors.create(null)
    useNavigatorStore().layoutOf(1).width = 320
    await nextTick()

    await editors.close(1)

    const calls = vi.mocked(fetch).mock.calls.map(([path, init]) => `${init!.method} ${path}`)
    const saved = calls.indexOf('PUT /api/editors/1/navigator')
    expect(saved).toBeGreaterThanOrEqual(0)
    expect(saved).toBeLessThan(calls.indexOf('DELETE /api/editors/1'))
  })
})

describe('saves when the page closes', () => {
  it('sends the text waiting for its delay at once, outliving the page', async () => {
    const editors = useEditorsStore()
    await editors.create(null)

    editors.setSql(1, 'SELECT 3')
    window.dispatchEvent(new Event('pagehide'))

    expect(fetch).toHaveBeenLastCalledWith(
      '/api/editors/1',
      expect.objectContaining({ method: 'PUT', keepalive: true }),
    )
    await vi.waitFor(() => expect(syncState.value).toBe('synced'))
  })
})
