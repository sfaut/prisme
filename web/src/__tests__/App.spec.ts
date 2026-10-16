import { createPinia, setActivePinia } from 'pinia'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { mount } from '@vue/test-utils'
import App from '../App.vue'

vi.mock('@/api/connections', () => ({
  listConnections: async () => [],
}))

// No saved editor: the interface creates one, without a connection.
vi.mock('@/api/editors', () => ({
  listEditors: async () => [],
  createEditor: async () => ({ id: 1, name: '', connection_id: null, sql: '' }),
}))

beforeEach(() => {
  setActivePinia(createPinia())
  // The explorer's tree (virtual scrolling) queries the media queries, missing
  // from jsdom. Without layout, it shows no node anyway.
  vi.stubGlobal('matchMedia', () => ({
    matches: false,
    addEventListener() {},
    removeEventListener() {},
  }))
})

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('App', () => {
  it('shows the name of the application and invites to choose a source', async () => {
    const wrapper = mount(App)
    expect(wrapper.text()).toContain('Prisme')
    await vi.waitFor(() =>
      expect(wrapper.text()).toContain('Double-cliquez sur une source de données'),
    )
  })
})
