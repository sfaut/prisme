import { createPinia, setActivePinia } from 'pinia'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { syncState } from '@/api/sync'
import { useEditorsStore } from '../editors'

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
        return new Response(JSON.stringify({ id: 1, name: '', connection_id: null, sql: '' }))
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
    window.localStorage.setItem('reka:prisme.cellLayout.1', '{}')
    window.localStorage.setItem('reka:prisme.cellLayout.2', '{}')

    await editors.close(1)

    expect(window.localStorage.getItem('reka:prisme.cellLayout.1')).toBeNull()
    expect(window.localStorage.getItem('reka:prisme.cellLayout.2')).toBe('{}')
    window.localStorage.clear()
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
