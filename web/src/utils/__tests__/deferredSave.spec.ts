import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { syncState } from '@/api/sync'

import { SAVE_DELAY, deferredSave, removeStoredKeys } from '../deferredSave'

beforeEach(() => {
  vi.useFakeTimers()
})

afterEach(() => {
  vi.useRealTimers()
  window.localStorage.clear()
})

describe('deferredSave', () => {
  it('sends once, shortly after the last change', () => {
    const send = vi.fn<(key: number, keepalive: boolean) => Promise<void>>(async () => {})
    const saves = deferredSave(send)
    saves.schedule(1)
    vi.advanceTimersByTime(SAVE_DELAY - 1)
    saves.schedule(1)
    saves.schedule(2)
    vi.advanceTimersByTime(SAVE_DELAY - 1)
    expect(send).not.toHaveBeenCalled()

    vi.advanceTimersByTime(1)
    expect(send.mock.calls).toEqual([
      [1, false],
      [2, false],
    ])
  })

  it('sends a waiting save at once on demand, and nothing without one', async () => {
    const send = vi.fn<(key: number, keepalive: boolean) => Promise<void>>(async () => {})
    const saves = deferredSave(send)
    await saves.flush(1)
    expect(send).not.toHaveBeenCalled()

    saves.schedule(1)
    await saves.flush(1)
    vi.advanceTimersByTime(SAVE_DELAY)
    expect(send.mock.calls).toEqual([[1, false]])
  })

  it('shows a change as being saved from its change to the end of its save', async () => {
    let answer = () => {}
    const send = vi.fn<(key: number, keepalive: boolean) => Promise<void>>(
      () => new Promise<void>((resolve) => (answer = resolve)),
    )
    const saves = deferredSave(send)
    saves.schedule(1)
    expect(syncState.value).toBe('writing')
    vi.advanceTimersByTime(SAVE_DELAY)
    expect(syncState.value).toBe('writing')

    // A change made during the save stays declared after it.
    saves.schedule(1)
    answer()
    await vi.waitFor(() => expect(send).toHaveBeenCalledOnce())
    await Promise.resolve()
    expect(syncState.value).toBe('writing')

    saves.cancel(1)
    expect(syncState.value).not.toBe('writing')
  })

  it('drops a cancelled save', () => {
    const send = vi.fn<(key: number, keepalive: boolean) => Promise<void>>(async () => {})
    const saves = deferredSave(send)
    saves.schedule(1)
    saves.cancel(1)
    vi.advanceTimersByTime(SAVE_DELAY)
    expect(send).not.toHaveBeenCalled()
  })

  it('sends the waiting saves as the page closes, beyond its life', () => {
    const send = vi.fn<(key: number, keepalive: boolean) => Promise<void>>(async () => {})
    const saves = deferredSave(send)
    saves.schedule(1)
    window.dispatchEvent(new Event('pagehide'))
    expect(send.mock.calls).toEqual([[1, true]])
  })

  it('outlives a failed save', async () => {
    const send = vi.fn<(key: number, keepalive: boolean) => Promise<void>>(async () => {
      throw new Error('offline')
    })
    const saves = deferredSave(send)
    saves.schedule(1)
    await expect(saves.flush(1)).resolves.toBeUndefined()
  })
})

describe('removeStoredKeys', () => {
  it('removes the matching keys only', () => {
    window.localStorage.setItem('prisme.old.1', 'x')
    window.localStorage.setItem('prisme.kept', 'y')
    removeStoredKeys((key) => key.startsWith('prisme.old.'))
    expect(Object.keys(window.localStorage)).toEqual(['prisme.kept'])
  })
})
