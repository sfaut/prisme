import { createPinia, setActivePinia } from 'pinia'
import { MYSQL_SOURCE } from '@/api/__tests__/capabilities'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { nextTick } from 'vue'

import { mount } from '@vue/test-utils'

import type { ConnectionSummary } from '@/api/generated/ConnectionSummary'
import { useConnectionsStore } from '@/stores/connections'

import NavigatorPanel from '../NavigatorPanel.vue'

/**
 * Connections returned by the simulated server.
 */
let savedConnections: ConnectionSummary[] = []

vi.mock('@/api/connections', () => ({
  listConnections: async () => savedConnections,
  probeConnection: async () => ({
    flavor: 'mysql',
    version: '8.4.0',
    ansi_quotes: false,
    no_backslash_escapes: false,
    oldest_supported: null,
  }),
}))

/**
 * Databases returned by the simulated server.
 */
let schemaNames = ['a', 'b', 'c']

/**
 * Connections whose catalog was read from the simulated server, in order.
 */
let streamed: number[] = []

vi.mock('@/api/navigator', () => ({
  streamCatalog: async (connectionId: number, onEvent: (event: unknown) => void) => {
    streamed.push(connectionId)
    onEvent({
      type: 'namespaces',
      source: MYSQL_SOURCE,
      namespaces: schemaNames.map((name) => ({ namespace: [name], system: false })),
    })
    onEvent({ type: 'done' })
  },
}))

/**
 * Gives a size to the elements: without it, jsdom measures nothing and the
 * tree's virtual scrolling shows no node.
 */
function giveLayout(): void {
  const size = { width: 300, height: 500, top: 0, left: 0, right: 300, bottom: 500, x: 0, y: 0 }
  // The virtual scrolling measures its scrolling element by `clientHeight`.
  vi.spyOn(HTMLElement.prototype, 'clientWidth', 'get').mockReturnValue(300)
  vi.spyOn(HTMLElement.prototype, 'clientHeight', 'get').mockReturnValue(500)
  vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockReturnValue({
    ...size,
    toJSON: () => size,
  })
  vi.stubGlobal(
    'ResizeObserver',
    class {
      constructor(private readonly callback: (entries: unknown[]) => void) {}
      observe(target: Element) {
        this.callback([{ target, contentRect: size }])
      }
      unobserve() {}
      disconnect() {}
    },
  )
  vi.stubGlobal(
    'IntersectionObserver',
    class {
      observe() {}
      unobserve() {}
      disconnect() {}
    },
  )
  vi.stubGlobal('matchMedia', () => ({
    matches: false,
    addEventListener() {},
    removeEventListener() {},
  }))
}

/**
 * Saved connection, to fill the list of sources.
 *
 * @param id Id of the connection.
 * @param name Name of the connection.
 */
function connection(id: number, name: string): ConnectionSummary {
  return {
    id,
    name,
    engine: 'mysql',
    settings: {
      family: 'server',
      host: 'localhost',
      port: 3306,
      username: 'root',
      database_name: '',
    },
    init_sql: '',
    session_timeout_minutes: 5,
    show_system_schemas: true,
    probe: null,
    created_at: '2026-10-13T00:00:00Z',
  }
}

/**
 * Mounts the navigator of an editor, attached to the page (the menus open in
 * `body`): by default, that of the active editor 1.
 *
 * @param props Properties of the navigator.
 */
function mountNavigator(props: {
  selectedId: number | null
  currentDatabase: string | null
  editorId?: number
  active?: boolean
}) {
  return mount(NavigatorPanel, {
    props: { editorId: 1, active: true, ...props, sessionStatus: null },
    attachTo: document.body,
  })
}

/**
 * Labels of the displayed lines.
 *
 * @param wrapper Mounted navigator.
 */
function labels(wrapper: ReturnType<typeof mountNavigator>): string[] {
  return wrapper.findAll('.tree-label').map((node) => node.text())
}

/**
 * Line of the tree whose label is given.
 *
 * @param wrapper Mounted navigator.
 * @param label Searched label.
 */
function row(wrapper: ReturnType<typeof mountNavigator>, label: string) {
  return wrapper.findAll('.tree-row').find((item) => item.find('.tree-label').text() === label)
}

/**
 * Clicks on an element. `detail` is 2 for the second click of a double click.
 *
 * @param element Clicked element.
 * @param detail Rank of the click in the series.
 */
async function click(element: { element: Element } | undefined, detail: number): Promise<void> {
  element?.element.dispatchEvent(new MouseEvent('click', { bubbles: true, detail }))
  await nextTick()
}

/**
 * Clicks on an element with Ctrl held, or Cmd (`metaKey`) as on macOS.
 *
 * @param element Clicked element.
 * @param key Modifier held.
 */
async function modifiedClick(
  element: { element: Element } | undefined,
  key: 'ctrlKey' | 'metaKey' = 'ctrlKey',
): Promise<void> {
  element?.element.dispatchEvent(new MouseEvent('click', { bubbles: true, detail: 1, [key]: true }))
  await nextTick()
}

beforeEach(() => {
  setActivePinia(createPinia())
  window.localStorage.clear()
  giveLayout()
  schemaNames = ['a', 'b', 'c']
  streamed = []
})

afterEach(() => {
  vi.restoreAllMocks()
  vi.unstubAllGlobals()
})

describe('NavigatorPanel', () => {
  it('puts the sources at the root and opens that of the chosen connection', async () => {
    savedConnections = [connection(1, 'Origen 2'), connection(2, 'Autre')]
    const wrapper = mountNavigator({ selectedId: 1, currentDatabase: null })
    expect(wrapper.find('h2').text()).toBe('Navigateur d’objets')
    await vi.waitFor(() => expect(labels(wrapper)).toEqual(['Origen 2', 'a', 'b', 'c', 'Autre']))
    wrapper.unmount()
  })

  it('says so when there is no connection', async () => {
    savedConnections = []
    const wrapper = mountNavigator({ selectedId: null, currentDatabase: null })
    await vi.waitFor(() => expect(wrapper.text()).toContain('Aucune connexion.'))
    expect(wrapper.find('[role="tree"]').exists()).toBe(false)
    wrapper.unmount()
  })

  it('offers a button on the title and a menu on each source', async () => {
    savedConnections = [connection(1, 'Origen 2')]
    const wrapper = mountNavigator({ selectedId: null, currentDatabase: null })
    await vi.waitFor(() => expect(wrapper.findAll('.node-menu')).toHaveLength(1))
    expect(wrapper.find('[aria-label="Rafraîchir"]').exists()).toBe(true)
    // Connections are added in the connections manager, from the top bar.
    expect(wrapper.find('[aria-label="Ajouter une connexion"]').exists()).toBe(false)
    const connections = useConnectionsStore()
    const options = () =>
      [...document.querySelectorAll('[role="menuitem"]')].map((item) => item.textContent?.trim())
    await click(wrapper.find('.node-menu'), 1)
    await vi.waitFor(() =>
      expect(options()).toEqual(['Modifier', 'Rafraîchir', 'Tester', 'Déconnecter', 'Supprimer']),
    )
    document.querySelector<HTMLElement>('[role="menuitem"]')?.click()
    await vi.waitFor(() => expect(connections.manager).toMatchObject({ open: true, target: 1 }))
    wrapper.unmount()
  })

  it('opens a source with a click on its line and chooses it with a Ctrl+click', async () => {
    savedConnections = [connection(1, 'Origen 2'), connection(2, 'Autre')]
    const wrapper = mountNavigator({ selectedId: 1, currentDatabase: null })
    await vi.waitFor(() => expect(row(wrapper, 'Autre')).toBeDefined())
    await click(row(wrapper, 'Autre'), 1)
    expect(wrapper.emitted('select')).toBeUndefined()
    // Open: its first database follows the source.
    const texts = () => labels(wrapper)
    await vi.waitFor(() => expect(texts()[texts().indexOf('Autre') + 1]).toBe('a'))
    // Ctrl+click: the source is chosen, and stays open.
    await modifiedClick(row(wrapper, 'Autre'))
    expect(wrapper.emitted('select')).toEqual([[2]])
    expect(row(wrapper, 'a')).toBeDefined()
    // Cmd+click on a database: it becomes the current database of its source,
    // and stays closed.
    await modifiedClick(row(wrapper, 'a'), 'metaKey')
    expect(wrapper.emitted('use-database')).toEqual([[1, 'a']])
    expect(row(wrapper, 'a')?.attributes('aria-expanded')).toBe('false')
    // A double click is two clicks: the database opens and closes, nothing more.
    await click(row(wrapper, 'a'), 1)
    await click(row(wrapper, 'a'), 2)
    expect(wrapper.emitted('use-database')).toHaveLength(1)
    // Enter on the focused line acts on it.
    await row(wrapper, 'a')?.trigger('keydown', { key: 'Enter' })
    expect(wrapper.emitted('use-database')).toEqual([
      [1, 'a'],
      [1, 'a'],
    ])
    // The button of the line too, without opening it.
    await row(wrapper, 'a')?.find('[aria-label="Utiliser cette base"]').trigger('click')
    expect(wrapper.emitted('use-database')).toHaveLength(3)
    expect(row(wrapper, 'a')?.attributes('aria-expanded')).toBe('false')
    wrapper.unmount()
  })

  it('pins the hierarchy of the top row while the tree scrolls', async () => {
    savedConnections = [connection(1, 'Origen 2')]
    schemaNames = Array.from({ length: 40 }, (_, index) => `db${index}`)
    const wrapper = mountNavigator({ selectedId: 1, currentDatabase: null })
    await vi.waitFor(() => expect(row(wrapper, 'db0')).toBeDefined())
    const pinned = () =>
      wrapper.findAll('[aria-hidden="true"] .tree-label').map((node) => node.text())

    expect(pinned()).toEqual([])
    // Ten rows down: the first row under the pinned one is a database of the
    // source, so the pinned row is the source.
    const tree = wrapper.find('[role="tree"]')
    Object.defineProperty(tree.element, 'scrollTop', { value: 10 * 24, configurable: true })
    await tree.trigger('scroll')
    expect(pinned()).toEqual(['Origen 2'])

    // Back to the top: nothing is pinned any more.
    Object.defineProperty(tree.element, 'scrollTop', { value: 0, configurable: true })
    await tree.trigger('scroll')
    expect(pinned()).toEqual([])
    wrapper.unmount()
  })

  it('opens and closes a pinned line with a click', async () => {
    savedConnections = [connection(1, 'Origen 2')]
    schemaNames = Array.from({ length: 40 }, (_, index) => `db${index}`)
    const wrapper = mountNavigator({ selectedId: 1, currentDatabase: null })
    await vi.waitFor(() => expect(row(wrapper, 'db0')).toBeDefined())
    const tree = wrapper.find('[role="tree"]')
    Object.defineProperty(tree.element, 'scrollTop', { value: 10 * 24, configurable: true })
    await tree.trigger('scroll')

    // The source is pinned under the root: its click closes it, and the pinned
    // rows follow the tree, which no longer holds any database under it.
    const pinnedSource = wrapper
      .findAll('[aria-hidden="true"] .tree-row')
      .find((item) => item.text().startsWith('Origen 2'))
    expect(pinnedSource).toBeDefined()
    await click(pinnedSource, 1)
    expect(wrapper.findAll('[aria-hidden="true"] .tree-row')).toHaveLength(0)
    expect(wrapper.emitted('select')).toBeUndefined()
    wrapper.unmount()
  })

  it('moves to the line whose label starts with the letters typed', async () => {
    savedConnections = [connection(1, 'Origen 2')]
    const wrapper = mountNavigator({ selectedId: 1, currentDatabase: null })
    await vi.waitFor(() => expect(row(wrapper, 'c')).toBeDefined())
    ;(row(wrapper, 'a')?.element as HTMLElement | undefined)?.focus()
    await row(wrapper, 'a')?.trigger('keydown', { key: 'c' })
    await vi.waitFor(() => expect(document.activeElement).toBe(row(wrapper, 'c')?.element))
    wrapper.unmount()
  })

  it('follows the current database, even when going back to a database already chosen', async () => {
    savedConnections = [connection(1, 'Origen 2'), connection(2, 'Autre')]
    const wrapper = mountNavigator({ selectedId: 1, currentDatabase: 'a' })
    const bold = () => wrapper.findAll('.current-database').map((node) => node.text())
    await vi.waitFor(() => expect(bold()).toEqual(['a']))

    for (const database of ['b', 'a', 'c', 'a']) {
      await wrapper.setProps({ currentDatabase: database })
      await vi.waitFor(() => expect(bold()).toEqual([database]))
    }
    wrapper.unmount()
  })

  it('shows the source of the editor in bold, and follows it', async () => {
    savedConnections = [connection(1, 'Origen 2'), connection(2, 'Autre')]
    const wrapper = mountNavigator({ selectedId: 1, currentDatabase: null })
    const bold = () => wrapper.findAll('.chosen-source').map((node) => node.text())
    await vi.waitFor(() => expect(bold()).toEqual(['Origen 2']))
    await wrapper.setProps({ selectedId: 2 })
    await vi.waitFor(() => expect(bold()).toEqual(['Autre']))
    await wrapper.setProps({ selectedId: null })
    await vi.waitFor(() => expect(bold()).toEqual([]))
    wrapper.unmount()
  })

  it('keeps the open nodes of each editor', async () => {
    savedConnections = [connection(1, 'Origen 2'), connection(2, 'Autre')]
    const first = mountNavigator({ selectedId: 1, currentDatabase: null })
    await vi.waitFor(() => expect(row(first, 'a')).toBeDefined())
    // The navigator of another editor, on another source, opens only that one.
    const second = mountNavigator({ selectedId: 2, currentDatabase: null, editorId: 2 })
    const texts = () => labels(second)
    await vi.waitFor(() => expect(texts()[texts().indexOf('Autre') + 1]).toBe('a'))
    expect(texts()[texts().indexOf('Origen 2') + 1]).toBe('Autre')
    second.unmount()

    // The source closed by the user stays closed when its editor comes back.
    await click(row(first, 'Origen 2'), 1)
    expect(row(first, 'a')).toBeUndefined()
    first.unmount()
    const again = mountNavigator({ selectedId: 1, currentDatabase: null })
    await vi.waitFor(() => expect(row(again, 'Origen 2')).toBeDefined())
    expect(row(again, 'a')).toBeUndefined()
    again.unmount()
  })

  it('opens the source of the editor when its connection changes', async () => {
    savedConnections = [connection(1, 'Origen 2'), connection(2, 'Autre')]
    const wrapper = mountNavigator({ selectedId: null, currentDatabase: null })
    await vi.waitFor(() => expect(row(wrapper, 'Autre')).toBeDefined())
    expect(row(wrapper, 'a')).toBeUndefined()
    await wrapper.setProps({ selectedId: 2 })
    const texts = () => labels(wrapper)
    await vi.waitFor(() => expect(texts()[texts().indexOf('Autre') + 1]).toBe('a'))
    wrapper.unmount()
  })

  it('reads no catalog while its editor is not active', async () => {
    savedConnections = [connection(1, 'Origen 2')]
    const wrapper = mountNavigator({ selectedId: 1, currentDatabase: null, active: false })
    await vi.waitFor(() => expect(wrapper.find('h2').exists()).toBe(true))
    await nextTick()
    expect(streamed).toEqual([])
    expect(labels(wrapper)).toEqual([])

    await wrapper.setProps({ active: true })
    await vi.waitFor(() => expect(row(wrapper, 'a')).toBeDefined())
    expect(streamed).toEqual([1])
    wrapper.unmount()
  })
})
