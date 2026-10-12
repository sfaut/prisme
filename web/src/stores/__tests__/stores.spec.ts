import { createPinia, setActivePinia } from 'pinia'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { useConnectionsStore } from '../connections'
import { useSettingsStore } from '../settings'

vi.mock('@/api/connections', () => ({
  listConnections: async () => [
    {
      id: 7,
      name: 'prod',
      engine: 'mysql',
      host: 'db',
      port: 3306,
      username: 'root',
      database_name: '',
      created_at: '2026-10-12T20:00:00.000Z',
    },
  ],
  createConnection: async () => ({ id: 7 }),
  deleteConnection: async () => undefined,
}))

vi.mock('@/api/settings', () => ({
  getSettings: async () => ({ row_limit: 1000, row_limit_choices: [1000, 2000] }),
  updateSettings: async ({ row_limit }: { row_limit: number }) => ({
    row_limit,
    row_limit_choices: [1000, 2000],
  }),
}))

beforeEach(() => {
  setActivePinia(createPinia())
})

describe('connections', () => {
  it('sélectionne la connexion créée', async () => {
    const store = useConnectionsStore()
    await store.create({ name: 'prod', host: 'db', port: 3306, username: 'root', password: 'x' })
    expect(store.selectedId).toBe(7)
    expect(store.selected?.name).toBe('prod')
  })
})

describe('suppression', () => {
  it('désélectionne la connexion supprimée', async () => {
    const store = useConnectionsStore()
    store.selectedId = 7
    await store.remove(7)
    expect(store.selectedId).toBeNull()
  })
})

describe('settings', () => {
  it('charge puis change la limite de lignes', async () => {
    const store = useSettingsStore()
    await store.load()
    expect(store.settings?.row_limit).toBe(1000)
    await store.setRowLimit(2000)
    expect(store.settings?.row_limit).toBe(2000)
  })
})
