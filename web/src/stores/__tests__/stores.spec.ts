import { createPinia, setActivePinia } from 'pinia'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { computed, isReactive } from 'vue'

import { useCatalogStore } from '../catalog'
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
      init_sql: '',
      session_timeout_minutes: 5,
      server_flavor: null,
      created_at: '2026-10-12T20:00:00.000Z',
    },
  ],
  createConnection: async () => ({ id: 7 }),
  deleteConnection: async () => undefined,
}))

vi.mock('@/api/explorer', () => ({
  streamCatalog: async (_id: number, onEvent: (event: unknown) => void) => {
    onEvent({ type: 'schemas', schemas: ['app', 'logs'] })
    onEvent({ type: 'done' })
  },
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

describe('catalog', () => {
  it('garde le catalogue hors de la réactivité et signale ses changements', async () => {
    const store = useCatalogStore()
    const names = computed(() => store.catalogOf(3)?.schemas.map((schema) => schema.name) ?? [])
    expect(names.value).toEqual([])
    await store.load(3)
    expect(names.value).toEqual(['app', 'logs'])
    expect(store.catalogOf(3)?.done).toBe(true)
    expect(isReactive(store.catalogOf(3))).toBe(false)
  })
})
