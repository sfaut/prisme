import { describe, expect, it } from 'vitest'

import { clearPending, markPending, syncState, trackRead, trackWrite } from '../sync'

describe('synchronisation state', () => {
  it('is idle before any exchange', () => {
    expect(syncState.value).toBe('idle')
  })

  it('is reading while a read runs, synced after it', async () => {
    const read = trackRead(Promise.resolve('read'))
    expect(syncState.value).toBe('reading')
    await expect(read).resolves.toBe('read')
    expect(syncState.value).toBe('synced')
  })

  it('is writing while a write runs, synced after it is accepted', async () => {
    const write = trackWrite(Promise.resolve('saved'))
    expect(syncState.value).toBe('writing')
    await expect(write).resolves.toBe('saved')
    expect(syncState.value).toBe('synced')
  })

  it('shows a write rather than a read running at the same time', async () => {
    const read = trackRead(Promise.resolve(undefined))
    const write = trackWrite(Promise.resolve(undefined))
    expect(syncState.value).toBe('writing')
    await Promise.all([read, write])
  })

  it('is failed after a refused write, and the failure is passed on', async () => {
    await expect(trackWrite(Promise.reject(new Error('refused')))).rejects.toThrow('refused')
    expect(syncState.value).toBe('failed')

    await trackWrite(Promise.resolve(undefined))
    expect(syncState.value).toBe('synced')
  })

  it('stays synced after a refused read, whose failure is passed on', async () => {
    await expect(trackRead(Promise.reject(new Error('unreadable')))).rejects.toThrow('unreadable')
    expect(syncState.value).toBe('synced')
  })

  it('is writing while a change waits to be saved', () => {
    markPending('test:change')
    expect(syncState.value).toBe('writing')
    clearPending('test:change')
    expect(syncState.value).toBe('synced')
  })
})
