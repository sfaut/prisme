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

vi.mock('@/api/profile', () => ({
  getProfile: async () => ({ email: 'ada@example.com', first_name: 'Ada', last_name: 'Lovelace' }),
}))

// Empty history, without events.
vi.mock('@/api/console', () => ({
  listHistory: async () => [],
  listDebug: async () => [],
  clearHistory: async () => {},
  clearDebug: async () => {},
  logNotification: () => {},
  followConsole: () => () => {},
}))

// No export, without events.
vi.mock('@/api/exports', () => ({
  listExports: async () => ({
    exports: [],
    orphans: { count: 0, bytes: 0 },
  }),
  followExports: () => () => {},
}))

// No saved editor: the interface creates one, without a connection.
vi.mock('@/api/editors', () => ({
  listEditors: async () => [],
  createEditor: async () => ({ id: 1, name: '', connection_id: null, sql: '' }),
}))

beforeEach(() => {
  setActivePinia(createPinia())
  // The navigator's tree (virtual scrolling) queries the media queries, missing
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
      expect(wrapper.text()).toContain(
        'Faites Ctrl+clic (Cmd+clic sous macOS) sur une source de données',
      ),
    )
  })

  it('shows the version of the server next to the name', async () => {
    const wrapper = mount(App)
    await vi.waitFor(() =>
      expect(wrapper.find('header [data-tooltip="Version de Prisme"]').text()).toBe('v1.2.3'),
    )
  })

  it('folds the side panels on their rail, and opens them again', async () => {
    window.localStorage.clear()
    const wrapper = mount(App)
    const objects = () => wrapper.find('aside').element.style.display !== 'none'
    expect(objects()).toBe(true)
    // The data explorer is closed by default: only its rail shows.
    expect(wrapper.find('[aria-label="Déplier : Explorateur de données"]').exists()).toBe(true)

    await wrapper.find('[aria-label="Replier : Navigateur d’objets"]').trigger('click')
    expect(objects()).toBe(false)
    expect(window.localStorage.getItem('prisme.objectsOpen')).toBe('false')
    await wrapper.find('[aria-label="Déplier : Navigateur d’objets"]').trigger('click')
    expect(objects()).toBe(true)

    await wrapper.find('[aria-label="Déplier : Explorateur de données"]').trigger('click')
    expect(wrapper.find('[aria-label="Replier : Explorateur de données"]').exists()).toBe(true)
    wrapper.unmount()
  })

  it('shows the filters of the console on request, in both tabs', async () => {
    window.localStorage.clear()
    const wrapper = mount(App)
    await vi.waitFor(() => expect(wrapper.text()).toContain('Aucune requête envoyée'))
    const toggle = () => wrapper.find('[aria-pressed]')
    expect(toggle().attributes('aria-label')).toBe('Afficher les filtres')
    expect(wrapper.find('[aria-label="Action"]').exists()).toBe(false)

    await toggle().trigger('click')
    expect(toggle().attributes('aria-pressed')).toBe('true')
    expect(wrapper.find('[aria-label="Action"]').exists()).toBe(true)
    expect(window.localStorage.getItem('prisme.consoleFiltersShown')).toBe('true')
    const debugTab = wrapper.findAll('[role="tab"]').find((tab) => tab.text() === 'Debug')!
    await debugTab.trigger('mousedown', { button: 0 })
    await vi.waitFor(() => expect(wrapper.find('[aria-label="Niveau"]').exists()).toBe(true))

    // Hidden, an active filter still shows on the button.
    await wrapper.find('[aria-label="Niveau"] button').trigger('click')
    await toggle().trigger('click')
    expect(wrapper.find('[aria-label="Niveau"]').exists()).toBe(false)
    expect(toggle().attributes('aria-label')).toBe('Afficher les filtres (filtres actifs)')
    expect(toggle().classes()).toContain('text-primary')
    wrapper.unmount()
  })

  it('opens the console on its history, and folds it on its rail', async () => {
    window.localStorage.clear()
    const wrapper = mount(App)
    await vi.waitFor(() => expect(wrapper.text()).toContain('Aucune requête envoyée'))
    // The fold button comes last, at the right end of the header, after the
    // button that clears the tab shown.
    const fold = wrapper.find('[aria-label="Replier : Console"]')
    const clear = wrapper.find('[aria-label="Effacer l’historique"]')
    expect(
      fold.element.compareDocumentPosition(clear.element) & Node.DOCUMENT_POSITION_PRECEDING,
    ).toBeTruthy()
    // The button that clears the tab shown: nothing to clear in an empty list.
    expect(wrapper.find('[aria-label="Effacer l’historique"]').attributes('disabled')).toBeDefined()
    const debugTab = wrapper.findAll('[role="tab"]').find((tab) => tab.text() === 'Debug')!
    await debugTab.trigger('mousedown', { button: 0 })
    await vi.waitFor(() => expect(wrapper.text()).toContain('Aucun message pour le moment'))
    expect(wrapper.find('[aria-label="Effacer le debug"]').exists()).toBe(true)
    expect(window.localStorage.getItem('prisme.consoleTab')).toBe('debug')
    await wrapper.find('[aria-label="Replier : Console"]').trigger('click')
    expect(wrapper.text()).not.toContain('Aucune requête envoyée')
    expect(window.localStorage.getItem('prisme.consoleOpen')).toBe('false')
    await wrapper.find('[aria-label="Déplier : Console"]').trigger('click')
    await vi.waitFor(() => expect(wrapper.text()).toContain('Aucun message pour le moment'))
    wrapper.unmount()
  })

  it('puts the account avatar, with the initials, in the top bar', async () => {
    const wrapper = mount(App)
    const bar = wrapper.find('header')
    await vi.waitFor(() => expect(bar.find('[aria-label="Compte"]').text()).toBe('AL'))
    expect(bar.find('[aria-label="Compte"]').attributes('data-tooltip')).toBe(
      'Compte : Ada Lovelace',
    )
    expect(bar.find('[aria-label="Thème"]').exists()).toBe(true)
  })
})
