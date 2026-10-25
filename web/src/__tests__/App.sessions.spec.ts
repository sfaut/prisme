import { createPinia, setActivePinia } from 'pinia'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { defineComponent, h, onMounted } from 'vue'

import { mount } from '@vue/test-utils'
import App from '../App.vue'
import type { ConnectionSummary } from '@/api/generated/ConnectionSummary'
import { useConnectionsStore } from '@/stores/connections'
import { useEditorsStore } from '@/stores/editors'

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
 * Database each mounted cell opens its session on (each mount opens one),
 * `null` for that of the connection.
 */
let openings: (string | null)[] = []

/**
 * Databases changed by `USE` on an open session.
 */
let changes: string[] = []

/**
 * Cell whose session ends closed, as after "Déconnecter".
 */
const ClosedCell = defineComponent({
  props: { openingDatabase: { type: String, default: null } },
  emits: ['status', 'database'],
  setup(props, { emit, expose }) {
    onMounted(() => {
      openings.push(props.openingDatabase)
      emit('status', 'closed')
    })
    expose({
      changeDatabaseWhenReady: (database: string) => changes.push(database),
      showTableWhenReady: () => {},
    })
    return () => null
  },
})

/**
 * Number of sessions closed by "Déconnecter".
 */
let disconnects = 0

/**
 * Number of cells mounted with a transaction open.
 */
let transactionCells = 0

/**
 * Cell whose session is ready, with a transaction open.
 */
const TransactionCell = defineComponent({
  emits: ['status', 'database', 'transaction'],
  setup(_, { emit, expose }) {
    onMounted(() => {
      emit('status', 'ready')
      emit('transaction', true)
      transactionCells += 1
    })
    expose({
      disconnect: async () => {
        disconnects += 1
      },
    })
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
      h('button', { id: 'select-other', onClick: () => emit('select', 2) }),
      h('button', { id: 'use', onClick: () => emit('use-database', 1, 'shop') }),
      h('button', { id: 'disconnect', onClick: () => emit('disconnect') }),
    ]
  },
})

beforeEach(() => {
  setActivePinia(createPinia())
  openings = []
  changes = []
  disconnects = 0
  transactionCells = 0
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
    await vi.waitFor(() => expect(openings).toEqual([null]))

    await wrapper.find('#select').trigger('click')
    await vi.waitFor(() => expect(openings).toEqual([null, null]))
    wrapper.unmount()
  })

  it('opens the session on the chosen database at once, without USE after', async () => {
    const wrapper = mount(App, {
      global: { stubs: { QueryCell: ClosedCell, ObjectNavigator: Navigator } },
    })
    useConnectionsStore().connections = [{ id: 1, name: 'local' } as ConnectionSummary]
    await vi.waitFor(() => expect(openings).toEqual([null]))

    await wrapper.find('#use').trigger('click')
    await vi.waitFor(() => expect(openings).toEqual([null, 'shop']))
    expect(changes).toEqual([])

    // Forgotten once used: the next opening is on the connection's database.
    await wrapper.find('#select').trigger('click')
    await vi.waitFor(() => expect(openings).toEqual([null, 'shop', null]))
    wrapper.unmount()
  })
})

describe('App, open transaction', () => {
  /**
   * Button of the confirmation window, found by its label.
   *
   * @param label Label of the button.
   */
  function dialogButton(label: string): HTMLButtonElement | undefined {
    return [...document.querySelectorAll<HTMLButtonElement>('[role="dialog"] button')].find(
      (button) => button.textContent?.trim() === label,
    )
  }

  it('asks for a confirmation before disconnecting the session', async () => {
    const wrapper = mount(App, {
      attachTo: document.body,
      global: { stubs: { QueryCell: TransactionCell, ObjectNavigator: Navigator } },
    })
    useConnectionsStore().connections = [{ id: 1, name: 'local' } as ConnectionSummary]
    await vi.waitFor(() => expect(transactionCells).toBe(1))

    await wrapper.find('#disconnect').trigger('click')
    await vi.waitFor(() => expect(dialogButton('Annuler')).toBeDefined())
    dialogButton('Annuler')!.click()
    await vi.waitFor(() => expect(dialogButton('Annuler')).toBeUndefined())
    expect(disconnects).toBe(0)

    await wrapper.find('#disconnect').trigger('click')
    await vi.waitFor(() => expect(dialogButton('Déconnecter et annuler')).toBeDefined())
    dialogButton('Déconnecter et annuler')!.click()
    await vi.waitFor(() => expect(disconnects).toBe(1))
    wrapper.unmount()
  })

  it('keeps the source of the editor when the change is not confirmed', async () => {
    const wrapper = mount(App, {
      attachTo: document.body,
      global: { stubs: { QueryCell: TransactionCell, ObjectNavigator: Navigator } },
    })
    useConnectionsStore().connections = [
      { id: 1, name: 'local' } as ConnectionSummary,
      { id: 2, name: 'other' } as ConnectionSummary,
    ]
    await vi.waitFor(() => expect(transactionCells).toBe(1))

    await wrapper.find('#select-other').trigger('click')
    await vi.waitFor(() => expect(dialogButton('Changer et annuler')).toBeDefined())
    dialogButton('Annuler')!.click()
    await vi.waitFor(() => expect(dialogButton('Annuler')).toBeUndefined())
    expect(useEditorsStore().active?.connectionId).toBe(1)

    await wrapper.find('#select-other').trigger('click')
    await vi.waitFor(() => expect(dialogButton('Changer et annuler')).toBeDefined())
    dialogButton('Changer et annuler')!.click()
    await vi.waitFor(() => expect(useEditorsStore().active?.connectionId).toBe(2))
    wrapper.unmount()
  })
})
