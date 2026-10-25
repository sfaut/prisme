import { createPinia, setActivePinia } from 'pinia'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { nextTick } from 'vue'

import type { HistoryFilter } from '@/api/console'
import type { Action } from '@/api/generated/Action'
import type { QueryEntry } from '@/api/generated/QueryEntry'

import { MAX_ENTRIES, matches, useConsoleStore } from '../console'

const listHistory =
  vi.fn<(filter: HistoryFilter, before?: number, limit?: number) => Promise<QueryEntry[]>>()
const clearHistory = vi.fn<() => Promise<void>>()

vi.mock('@/api/console', () => ({
  listHistory: (filter: HistoryFilter, before?: number, limit?: number) =>
    listHistory({ action: [...filter.action] }, before, limit),
  listDebug: async () => [],
  clearHistory: () => clearHistory(),
  clearDebug: async () => {},
  followConsole: () => () => {},
}))

/**
 * Query of the history numbered `id`, sent by `action`.
 */
function entry(id: number, action: Action = 'run'): QueryEntry {
  return {
    id,
    started_at: '2026-10-20T12:40:00.000Z',
    action,
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
  window.localStorage.clear()
  setActivePinia(createPinia())
  listHistory.mockReset()
  clearHistory.mockReset()
})

describe('console store', () => {
  it('reads the history, keeping the queries received meanwhile', async () => {
    const store = useConsoleStore().history
    store.add(entry(3))
    listHistory.mockResolvedValueOnce([entry(1), entry(2)])
    await store.reload()
    expect(store.entries.map((e) => e.id)).toEqual([1, 2, 3])
    expect(store.hasOlder).toBe(false)
  })

  it('reads the older queries before the first one shown', async () => {
    const store = useConsoleStore().history
    store.add(entry(10))
    listHistory.mockResolvedValueOnce([entry(8), entry(9)])
    await store.loadOlder()
    expect(listHistory).toHaveBeenCalledWith({ action: [] }, 10, 500)
    expect(store.entries.map((e) => e.id)).toEqual([8, 9, 10])
  })

  it('clears the history on the server, then empties it', async () => {
    const store = useConsoleStore().history
    store.add(entry(1))
    clearHistory.mockRejectedValueOnce(new Error('unreachable'))
    await store.clear()
    expect(store.entries).toHaveLength(1)
    expect(store.error).toBe('unreachable')
    clearHistory.mockResolvedValueOnce()
    await store.clear()
    expect(store.entries).toHaveLength(0)
    expect(store.hasOlder).toBe(false)
    expect(store.error).toBeNull()
  })

  it('keeps the last queries only, and ignores a repeated one', () => {
    const store = useConsoleStore().history
    for (let id = 1; id <= MAX_ENTRIES + 2; id++) {
      store.add(entry(id))
    }
    store.add(entry(MAX_ENTRIES + 2))
    expect(store.entries).toHaveLength(MAX_ENTRIES)
    expect(store.entries[0]!.id).toBe(3)
    expect(store.hasOlder).toBe(true)
  })

  it('reads the queries of the actions kept, and drops the others received', async () => {
    const store = useConsoleStore()
    listHistory.mockResolvedValue([entry(2, 'navigator')])
    store.history.add(entry(1, 'run'))
    store.historyFilter.action = ['navigator']
    await nextTick()
    await vi.waitFor(() => expect(store.history.entries.map((e) => e.id)).toEqual([2]))
    expect(listHistory).toHaveBeenCalledWith({ action: ['navigator'] }, undefined, 500)
    expect(store.history.refilters).toBe(1)

    store.history.add(entry(3, 'run'))
    store.history.add(entry(4, 'navigator'))
    expect(store.history.entries.map((e) => e.id)).toEqual([2, 4])
    expect(JSON.parse(window.localStorage.getItem('prisme.consoleHistoryFilter')!)).toEqual({
      action: ['navigator'],
    })
  })

  it('keeps the queries shown that a new filter keeps, and ignores an earlier read', async () => {
    const store = useConsoleStore()
    let answerEarlier: (entries: QueryEntry[]) => void = () => {}
    listHistory.mockReturnValueOnce(new Promise((resolve) => (answerEarlier = resolve)))
    const earlier = store.history.reload()
    store.history.add(entry(1, 'run'))
    store.history.add(entry(2, 'probe'))

    let answer: (entries: QueryEntry[]) => void = () => {}
    listHistory.mockReturnValueOnce(new Promise((resolve) => (answer = resolve)))
    store.historyFilter.action = ['probe']
    await nextTick()
    expect(store.history.entries.map((e) => e.id)).toEqual([2])

    answerEarlier([entry(1, 'run')])
    await earlier
    expect(store.history.entries.map((e) => e.id)).toEqual([2])
    answer([entry(2, 'probe')])
    await vi.waitFor(() => expect(listHistory).toHaveBeenCalledTimes(2))
    expect(store.history.entries.map((e) => e.id)).toEqual([2])
  })

  it('forgets the remembered actions that no longer exist', () => {
    window.localStorage.setItem(
      'prisme.consoleHistoryFilter',
      JSON.stringify({ action: ['run', 'explorer', 3] }),
    )
    expect(useConsoleStore().historyFilter).toEqual({ action: ['run'] })
  })

  it('keeps an element whose fields are among the values kept', () => {
    const line = { level: 'error', source: 'interface' }
    expect(matches({ level: [], source: [] }, line)).toBe(true)
    expect(matches({ level: ['error', 'warn'] }, line)).toBe(true)
    expect(matches({ level: ['error'], source: ['server'] }, line)).toBe(false)
  })
})
