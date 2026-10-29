import { createPinia, setActivePinia } from 'pinia'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import type { ExportHandlers } from '@/api/exports'
import type { ExportInfo } from '@/api/generated/ExportInfo'
import type { ExportList } from '@/api/generated/ExportList'
import type { ToastAction } from '@/components/ui/toasts'

import { useExportsStore } from '../exports'

const listExports = vi.fn<() => Promise<ExportList>>()
const removeExport = vi.fn<(id: number) => Promise<void>>()
const notify = vi.fn<(title: string, tone?: string, action?: ToastAction) => void>()
const saveUrl = vi.fn<(url: string) => void>()

/**
 * Receivers given to `followExports` by the store.
 */
let handlers: ExportHandlers | null = null

vi.mock('@/api/exports', () => ({
  listExports: () => listExports(),
  removeExport: (id: number) => removeExport(id),
  exportFileUrl: (id: number) => `/api/exports/${id}/file`,
  followExports: (given: ExportHandlers) => {
    handlers = given
    return () => {}
  },
}))

vi.mock('@/components/ui/toasts', () => ({
  notify: (title: string, tone?: string, action?: ToastAction) => notify(title, tone, action),
}))

vi.mock('@/components/download', () => ({
  saveUrl: (url: string) => saveUrl(url),
}))

/**
 * Export `id`, running unless `changes` say otherwise.
 *
 * @param id Id of the export.
 * @param changes Fields that differ.
 */
function entry(id: number, changes: Partial<ExportInfo> = {}): ExportInfo {
  return {
    id,
    name: `export-${id}.csv`,
    connection_name: 'local',
    sql: 'SELECT seq FROM seq_1_to_3000000',
    state: 'running',
    rows: 0,
    bytes: 3,
    expected_rows: null,
    created_at: '2026-10-29T10:00:00.000Z',
    ended_at: null,
    error: null,
    file_missing: false,
    ...changes,
  }
}

/**
 * List read from the server.
 *
 * @param exports Exports, the most recent first.
 */
function list(...exports: ExportInfo[]): ExportList {
  return { exports, orphans: { count: 0, bytes: 0 } }
}

beforeEach(() => {
  setActivePinia(createPinia())
  listExports.mockReset()
  removeExport.mockReset()
  notify.mockReset()
  saveUrl.mockReset()
  handlers = null
})

/**
 * Store following the exports, its first list read.
 *
 * @param exports Exports of the first list.
 */
async function followed(...exports: ExportInfo[]) {
  listExports.mockResolvedValue(list(...exports))
  const store = useExportsStore()
  store.start()
  await vi.waitFor(() => expect(listExports).toHaveBeenCalled())
  await Promise.resolve()
  return store
}

describe('exports store', () => {
  it('follows the progress, and announces the end with the download', async () => {
    const store = await followed(entry(1))
    listExports.mockClear()

    handlers?.onEvent({ type: 'changed', export: entry(1, { rows: 500, bytes: 4000 }) })
    expect(store.exports[0]?.rows).toBe(500)
    expect(store.runningCount).toBe(1)
    expect(store.usage).toEqual({ count: 1, bytes: 4000 })
    expect(notify).not.toHaveBeenCalled()

    handlers?.onEvent({ type: 'changed', export: entry(1, { state: 'ready', rows: 1000 }) })
    expect(store.runningCount).toBe(0)
    expect(listExports).not.toHaveBeenCalled()
    expect(notify).toHaveBeenCalledWith('Export prêt : export-1.csv, 1 000 lignes.', 'success', {
      label: 'Télécharger',
      alternative: expect.any(String),
      run: expect.any(Function),
    })
    notify.mock.calls[0]?.[2]?.run()
    expect(saveUrl).toHaveBeenCalledWith('/api/exports/1/file')
  })

  it('announces a failure with its cause, and nothing for a stop', async () => {
    await followed(entry(1), entry(2))
    handlers?.onEvent({
      type: 'changed',
      export: entry(1, { state: 'failed', error: { code: 'export_not_read' } }),
    })
    handlers?.onEvent({ type: 'changed', export: entry(2, { state: 'cancelled' }) })
    expect(notify.mock.calls).toEqual([
      [
        'Export échoué : export-1.csv.\nExport refusé : seule une lecture (SELECT, SHOW…) a des lignes à exporter.',
        'error',
        undefined,
      ],
    ])
  })

  it('never brings an ended export back to running', async () => {
    const store = await followed()
    handlers?.onEvent({ type: 'changed', export: entry(1, { state: 'ready' }) })
    // Late answer of the start.
    handlers?.onEvent({ type: 'changed', export: entry(1) })
    expect(store.exports.map((e) => e.state)).toEqual(['ready'])
  })

  it('applies again the events received while the list is read', async () => {
    const store = await followed(entry(1), entry(2))
    let answer: (value: ExportList) => void = () => {}
    listExports.mockReturnValueOnce(new Promise((resolve) => (answer = resolve)))
    handlers?.onGap()
    handlers?.onEvent({ type: 'changed', export: entry(1, { state: 'ready' }) })
    handlers?.onEvent({ type: 'removed', id: 2 })
    // The list, read before both events.
    answer(list(entry(1), entry(2)))
    await vi.waitFor(() =>
      expect(store.exports.map((e) => [e.id, e.state])).toEqual([[1, 'ready']]),
    )
  })

  it('removes an export, its file no longer counted', async () => {
    const store = await followed(entry(1, { state: 'ready' }), entry(2, { file_missing: true }))
    expect(store.usage).toEqual({ count: 1, bytes: 3 })
    removeExport.mockResolvedValue()
    await store.remove(1)
    handlers?.onEvent({ type: 'removed', id: 1 })
    expect(store.exports.map((e) => e.id)).toEqual([2])
    expect(store.usage).toEqual({ count: 0, bytes: 0 })
  })
})
