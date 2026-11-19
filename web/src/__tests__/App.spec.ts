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

/**
 * Id of the next editor created.
 */
const editorIds = vi.hoisted(() => ({ next: 1 }))

// No saved editor: the interface creates one, without a connection.
vi.mock('@/api/editors', () => ({
  listEditors: async () => [],
  createEditor: async () => ({ id: editorIds.next++, name: '', connection_id: null, sql: '' }),
}))

beforeEach(() => {
  editorIds.next = 1
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

  it('puts the editors’ tabs above the side panels, and the console under them', async () => {
    const wrapper = mount(App)
    const frame = wrapper.find('main')
    await vi.waitFor(() => expect(frame.find('[id^="navigator-panel-"]').exists()).toBe(true))
    const tabs = frame.find('[aria-label="Éditeurs"]').element
    const navigator = frame.find('[id^="navigator-panel-"]').element
    expect(tabs.compareDocumentPosition(navigator) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy()
    expect(frame.find('[id^="explorer-panel-"]').exists()).toBe(true)
    expect(frame.find('#console-panel').exists()).toBe(false)
    expect(wrapper.find('#console-panel').exists()).toBe(true)
    wrapper.unmount()
  })

  it('opens and closes the side panels from the activity bar', async () => {
    window.localStorage.clear()
    const wrapper = mount(App)
    const objects = () => wrapper.find('aside').element.style.display !== 'none'
    const button = (label: string) => wrapper.find(`nav button[aria-label="${label}"]`)
    await vi.waitFor(() => expect(wrapper.find('aside').exists()).toBe(true))
    expect(objects()).toBe(true)
    expect(button('Navigateur d’objets').attributes('aria-pressed')).toBe('true')
    expect(button('Navigateur d’objets').attributes('data-tooltip')).toBe(
      'Fermer le navigateur d’objets (Ctrl-K puis N)',
    )
    // The data explorer is closed by default: no trace of it but its button.
    expect(button('Explorateur de données').attributes('aria-pressed')).toBe('false')
    expect(wrapper.find('[aria-controls^="explorer-panel-"]').exists()).toBe(false)

    await button('Navigateur d’objets').trigger('click')
    expect(objects()).toBe(false)
    expect(wrapper.find('[aria-controls^="navigator-panel-"]').exists()).toBe(false)
    expect(JSON.parse(window.localStorage.getItem('prisme.navigator.layout.1')!).open).toBe(false)
    await button('Navigateur d’objets').trigger('click')
    expect(objects()).toBe(true)

    await button('Explorateur de données').trigger('click')
    expect(wrapper.find('aside h2').exists()).toBe(true)
    expect(wrapper.findAll('aside h2').map((title) => title.text())).toContain(
      'Explorateur de données',
    )
    wrapper.unmount()
  })

  it('shows the filters of the console on request, in both tabs', async () => {
    window.localStorage.clear()
    const wrapper = mount(App)
    await vi.waitFor(() => expect(wrapper.text()).toContain('Aucune requête envoyée'))
    // The button of the filters: the only one pressed or released in the console.
    const toggle = () => wrapper.find('#console-panel [aria-pressed]')
    expect(toggle().attributes('aria-label')).toBe('Filtrer')
    expect(wrapper.find('[aria-label="Action"]').exists()).toBe(false)

    await toggle().trigger('click')
    expect(toggle().attributes('aria-pressed')).toBe('true')
    expect(wrapper.find('[aria-label="Action"]').exists()).toBe(true)
    expect(window.localStorage.getItem('prisme.console.filtersShown')).toBe('true')
    const debugTab = wrapper.findAll('[role="tab"]').find((tab) => tab.text() === 'Debug')!
    await debugTab.trigger('mousedown', { button: 0 })
    await vi.waitFor(() => expect(wrapper.find('[aria-label="Niveau"]').exists()).toBe(true))

    // Hidden, an active filter still shows on the button.
    await wrapper.find('[aria-label="Niveau"] button').trigger('click')
    await toggle().trigger('click')
    expect(wrapper.find('[aria-label="Niveau"]').exists()).toBe(false)
    expect(toggle().attributes('aria-label')).toBe('Filtrer (filtres actifs)')
    expect(toggle().classes()).toContain('text-primary')
    wrapper.unmount()
  })

  it('opens the console on its history, and closes it from the activity bar', async () => {
    window.localStorage.clear()
    const wrapper = mount(App)
    await vi.waitFor(() => expect(wrapper.text()).toContain('Aucune requête envoyée'))
    // The button that clears the tab shown: nothing to clear in an empty list.
    expect(wrapper.find('[aria-label="Effacer l’historique"]').attributes('disabled')).toBeDefined()
    const debugTab = wrapper.findAll('[role="tab"]').find((tab) => tab.text() === 'Debug')!
    await debugTab.trigger('mousedown', { button: 0 })
    await vi.waitFor(() => expect(wrapper.text()).toContain('Aucun message pour le moment'))
    expect(wrapper.find('[aria-label="Effacer le debug"]').exists()).toBe(true)
    expect(window.localStorage.getItem('prisme.console.tab')).toBe('debug')
    const button = wrapper.find('nav button[aria-label="Console"]')
    expect(button.attributes('aria-pressed')).toBe('true')
    await button.trigger('click')
    expect(wrapper.text()).not.toContain('Aucune requête envoyée')
    expect(wrapper.find('[aria-controls="console-panel"]').exists()).toBe(false)
    expect(window.localStorage.getItem('prisme.console.open')).toBe('false')
    await wrapper.find('nav button[aria-label="Console"]').trigger('click')
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

  it('opens a new editor by the "Nouveau" button of the top bar', async () => {
    const wrapper = mount(App, { attachTo: document.body })
    const tabs = () => wrapper.findAll('[aria-label="Éditeurs"] [role="tab"]')
    await vi.waitFor(() => expect(tabs()).toHaveLength(1))
    const button = wrapper.findAll('header button').find((item) => item.text() === 'Nouveau')
    // At the right of the bar, before the exports.
    expect(button!.element.parentElement).toBe(wrapper.find('header').element.lastElementChild)
    expect(button!.classes()).toContain('bg-primary')
    await button!.trigger('click')
    await vi.waitFor(() => expect(tabs()).toHaveLength(2))
    wrapper.unmount()
  })
})
