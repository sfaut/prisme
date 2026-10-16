import { createPinia, setActivePinia } from 'pinia'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { useEditorsStore } from '../editors'

const updateEditor = vi.fn<() => Promise<void>>()

vi.mock('@/api/editors', () => ({
  listEditors: async () => [],
  createEditor: async () => ({ id: 1, name: '', connection_id: null, sql: '' }),
  updateEditor: () => updateEditor(),
}))

beforeEach(() => {
  setActivePinia(createPinia())
  updateEditor.mockReset()
  updateEditor.mockResolvedValue(undefined)
})

describe('synchronisation state', () => {
  it('is pending after a change, then synced once the server has saved it', async () => {
    const editors = useEditorsStore()
    await editors.create(null)
    expect(editors.syncState).toBe('synced')

    editors.setSql(1, 'SELECT 1')
    expect(editors.syncState).toBe('pending')

    await vi.waitFor(() => expect(editors.syncState).toBe('synced'))
    expect(updateEditor).toHaveBeenCalledOnce()
  })

  it('is failed after a refused save, until a later save succeeds', async () => {
    const editors = useEditorsStore()
    await editors.create(null)

    updateEditor.mockRejectedValueOnce(new Error('refused'))
    editors.setSql(1, 'SELECT 1')
    await vi.waitFor(() => expect(editors.syncState).toBe('failed'))

    editors.setSql(1, 'SELECT 2')
    await vi.waitFor(() => expect(editors.syncState).toBe('synced'))
  })
})
