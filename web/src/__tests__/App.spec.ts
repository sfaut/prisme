import { createPinia, setActivePinia } from 'pinia'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { mount } from '@vue/test-utils'
import App from '../App.vue'

vi.mock('@/api/connections', () => ({
  listConnections: async () => [],
}))

vi.mock('@/api/version', () => ({
  getVersion: async () => ({ version: '1.2.3' }),
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

  it('shows the version of the server beneath the name', async () => {
    const wrapper = mount(App)
    await vi.waitFor(() =>
      expect(wrapper.find('header [title="Version de Prisme"]').text()).toBe('v1.2.3'),
    )
  })

  it('puts the options button in the top bar, above the explorer', async () => {
    const wrapper = mount(App)
    const bar = wrapper.find('header')
    expect(bar.find('[aria-label="Options"]').exists()).toBe(true)
    expect(bar.find('[aria-label="Thème"]').exists()).toBe(true)
  })
})
