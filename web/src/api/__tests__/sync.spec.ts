import { describe, expect, it } from 'vitest'

import { clearPending, markPending, syncState, trackWrite } from '../sync'

describe('synchronisation state', () => {
  it('is pending while a write runs, synced after it is accepted', async () => {
    const write = trackWrite(Promise.resolve('saved'))
    expect(syncState.value).toBe('pending')
    await expect(write).resolves.toBe('saved')
    expect(syncState.value).toBe('synced')
  })

  it('is failed after a refused write, and the failure is passed on', async () => {
    await expect(trackWrite(Promise.reject(new Error('refused')))).rejects.toThrow('refused')
    expect(syncState.value).toBe('failed')

    await trackWrite(Promise.resolve(undefined))
    expect(syncState.value).toBe('synced')
  })

  it('is pending while a change waits to be saved', () => {
    markPending('test:change')
    expect(syncState.value).toBe('pending')
    clearPending('test:change')
    expect(syncState.value).toBe('synced')
  })
})
