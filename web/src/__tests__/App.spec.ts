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

// Empty history, without events.
vi.mock('@/api/console', () => ({
  listHistory: async () => [],
  followConsole: () => () => {},
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

  it('folds the side panels on their rail, and opens them again', async () => {
    window.localStorage.clear()
    const wrapper = mount(App)
    const objects = () => wrapper.find('aside').element.style.display !== 'none'
    expect(objects()).toBe(true)
    // The data explorer is closed by default: only its rail shows.
    expect(wrapper.find('[aria-label="Déplier : Explorateur de données"]').exists()).toBe(true)

    await wrapper.find('[aria-label="Replier : Explorateur d’objets"]').trigger('click')
    expect(objects()).toBe(false)
    expect(window.localStorage.getItem('prisme.objectsOpen')).toBe('false')
    await wrapper.find('[aria-label="Déplier : Explorateur d’objets"]').trigger('click')
    expect(objects()).toBe(true)

    await wrapper.find('[aria-label="Déplier : Explorateur de données"]').trigger('click')
    expect(wrapper.find('[aria-label="Replier : Explorateur de données"]').exists()).toBe(true)
    wrapper.unmount()
  })

  it('opens the console on its history, and folds it on its rail', async () => {
    window.localStorage.clear()
    const wrapper = mount(App)
    await vi.waitFor(() => expect(wrapper.text()).toContain('Aucune requête envoyée'))
    await wrapper.find('[aria-label="Replier : Console"]').trigger('click')
    expect(wrapper.text()).not.toContain('Aucune requête envoyée')
    expect(window.localStorage.getItem('prisme.consoleOpen')).toBe('false')
    await wrapper.find('[aria-label="Déplier : Console"]').trigger('click')
    await vi.waitFor(() => expect(wrapper.text()).toContain('Aucune requête envoyée'))
    wrapper.unmount()
  })

  it('puts the options button in the top bar, above the explorer', async () => {
    const wrapper = mount(App)
    const bar = wrapper.find('header')
    expect(bar.find('[aria-label="Options"]').exists()).toBe(true)
    expect(bar.find('[aria-label="Thème"]').exists()).toBe(true)
  })
})
