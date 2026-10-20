import { createPinia, setActivePinia } from 'pinia'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import type { QueryEntry } from '@/api/generated/QueryEntry'

import { MAX_ENTRIES, useConsoleStore } from '../console'

const listHistory = vi.fn<(before?: number, limit?: number) => Promise<QueryEntry[]>>()

vi.mock('@/api/console', () => ({
  listHistory: (before?: number, limit?: number) => listHistory(before, limit),
  followConsole: () => () => {},
}))

/**
 * Query of the history numbered `id`.
 */
function entry(id: number): QueryEntry {
  return {
    id,
    started_at: '2026-10-20T12:40:00.000Z',
    action: 'run',
    connection_name: '',
    sql: `SELECT ${id}`,
    duration_us: 0,
    transfer_us: 0,
    returns_rows: true,
    row_count: 1,
    affected_rows: 0,
    truncated: false,
    error: '',
  }
}

beforeEach(() => {
  setActivePinia(createPinia())
  listHistory.mockReset()
})

describe('console store', () => {
  it('reads the history, keeping the queries received meanwhile', async () => {
    const store = useConsoleStore()
    store.add(entry(3))
    listHistory.mockResolvedValueOnce([entry(1), entry(2)])
    await store.reload()
    expect(store.entries.map((e) => e.id)).toEqual([1, 2, 3])
    expect(store.hasOlder).toBe(false)
  })

  it('reads the older queries before the first one shown', async () => {
    const store = useConsoleStore()
    store.add(entry(10))
    listHistory.mockResolvedValueOnce([entry(8), entry(9)])
    await store.loadOlder()
    expect(listHistory).toHaveBeenCalledWith(10, 500)
    expect(store.entries.map((e) => e.id)).toEqual([8, 9, 10])
  })

  it('keeps the last queries only, and ignores a repeated one', () => {
    const store = useConsoleStore()
    for (let id = 1; id <= MAX_ENTRIES + 2; id++) {
      store.add(entry(id))
    }
    store.add(entry(MAX_ENTRIES + 2))
    expect(store.entries).toHaveLength(MAX_ENTRIES)
    expect(store.entries[0]!.id).toBe(3)
    expect(store.hasOlder).toBe(true)
  })
})
