import { createPinia, setActivePinia } from 'pinia'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { computed, isReactive } from 'vue'

import type { SettingsUpdate } from '@/api/generated/SettingsUpdate'

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
      show_system_schemas: true,
      server_flavor: null,
      server_version: null,
      created_at: '2026-10-12T20:00:00.000Z',
    },
  ],
  createConnection: async () => ({ id: 7 }),
  probeConnection: async () => ({
    flavor: 'mariadb',
    version: '11.4.2-MariaDB',
    ansi_quotes: false,
  }),
  deleteConnection: async () => undefined,
}))

vi.mock('@/api/explorer', () => ({
  streamCatalog: async (_id: number, onEvent: (event: unknown) => void) => {
    onEvent({ type: 'schemas', schemas: ['app', 'logs'] })
    onEvent({ type: 'done' })
  },
}))

vi.mock('@/api/settings', () => ({
  getSettings: async () => ({
    row_limit: 1000,
    row_limit_choices: [1000, 2000],
    read_only_session: true,
    history_retention_days: 30,
    debug_retention_days: 30,
  }),
  updateSettings: async (update: SettingsUpdate) => ({
    ...update,
    row_limit_choices: [1000, 2000],
  }),
}))

beforeEach(() => {
  setActivePinia(createPinia())
})

describe('connections', () => {
  it('selects the created connection', async () => {
    const store = useConnectionsStore()
    await store.create({ name: 'prod', host: 'db', port: 3306, username: 'root', password: 'x' })
    expect(store.selectedId).toBe(7)
    expect(store.selected?.name).toBe('prod')
  })

  it('detects in the background the server of a connection never connected', async () => {
    const store = useConnectionsStore()
    await store.load()
    await vi.waitFor(() => expect(store.connections[0]?.server_flavor).toBe('mariadb'))
    expect(store.connections[0]?.server_version).toBe('11.4.2-MariaDB')
  })
})

describe('deletion', () => {
  it('unselects the deleted connection', async () => {
    const store = useConnectionsStore()
    store.selectedId = 7
    await store.remove(7)
    expect(store.selectedId).toBeNull()
  })
})

describe('settings', () => {
  it('loads then changes the row limit', async () => {
    const store = useSettingsStore()
    await store.load()
    expect(store.settings?.row_limit).toBe(1000)
    await store.update({ row_limit: 2000 })
    expect(store.settings?.row_limit).toBe(2000)
  })

  it('keeps the settings not changed', async () => {
    const store = useSettingsStore()
    await store.load()
    await store.update({ history_retention_days: 90 })
    expect(store.settings?.history_retention_days).toBe(90)
    expect(store.settings?.debug_retention_days).toBe(30)
    expect(store.settings?.row_limit).toBe(1000)
    expect(store.settings?.read_only_session).toBe(true)
  })
})

describe('catalog', () => {
  it('keeps the catalog outside reactivity and signals its changes', async () => {
    const store = useCatalogStore()
    const names = computed(() => store.catalogOf(3)?.schemas.map((schema) => schema.name) ?? [])
    expect(names.value).toEqual([])
    await store.load(3)
    expect(names.value).toEqual(['app', 'logs'])
    expect(store.catalogOf(3)?.done).toBe(true)
    expect(isReactive(store.catalogOf(3))).toBe(false)
  })
})
