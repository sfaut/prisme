import { createPinia, setActivePinia } from 'pinia'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { defineComponent, h, onMounted } from 'vue'

import { mount } from '@vue/test-utils'
import App from '../App.vue'
import type { ConnectionSummary } from '@/api/generated/ConnectionSummary'
import { useConnectionsStore } from '@/stores/connections'

vi.mock('@/api/version', () => ({
  getVersion: async () => ({ version: '1.2.3' }),
}))

vi.mock('@/api/profile', () => ({
  getProfile: async () => ({ email: 'ada@example.com', first_name: 'Ada', last_name: 'Lovelace' }),
}))

vi.mock('@/api/console', () => ({
  listHistory: async () => [],
  listDebug: async () => [],
  clearHistory: async () => {},
  clearDebug: async () => {},
  logNotification: () => {},
  followConsole: () => () => {},
}))

// One saved editor, on connection 1.
vi.mock('@/api/editors', () => ({
  listEditors: async () => [{ id: 1, name: '', connection_id: 1, sql: '' }],
  updateEditor: async () => {},
}))

/**
 * Number of cells mounted: each mount opens a session.
 */
let mounts = 0

/**
 * Cell whose session ends closed, as after "Déconnecter".
 */
const ClosedCell = defineComponent({
  emits: ['status', 'database'],
  setup(_, { emit, expose }) {
    onMounted(() => {
      mounts += 1
      emit('status', 'closed')
    })
    expose({ changeDatabaseWhenReady: () => {}, showTableWhenReady: () => {} })
    return () => null
  },
})

/**
 * Navigator reduced to the events of a source and of a database.
 */
const Navigator = defineComponent({
  emits: ['select', 'use-database', 'open-table', 'index-operations', 'disconnect'],
  setup(_, { emit }) {
    return () => [
      h('button', { id: 'select', onClick: () => emit('select', 1) }),
      h('button', { id: 'use', onClick: () => emit('use-database', 1, 'shop') }),
    ]
  },
})

beforeEach(() => {
  setActivePinia(createPinia())
  mounts = 0
})

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('App, closed session', () => {
  it('reopens the closed session when its source is chosen again in the navigator', async () => {
    const wrapper = mount(App, {
      global: { stubs: { QueryCell: ClosedCell, ObjectNavigator: Navigator } },
    })
    useConnectionsStore().connections = [{ id: 1, name: 'local' } as ConnectionSummary]
    await vi.waitFor(() => expect(mounts).toBe(1))

    await wrapper.find('#select').trigger('click')
    await vi.waitFor(() => expect(mounts).toBe(2))

    await wrapper.find('#use').trigger('click')
    await vi.waitFor(() => expect(mounts).toBe(3))
    wrapper.unmount()
  })
})
