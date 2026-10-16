import { createPinia, setActivePinia } from 'pinia'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { nextTick } from 'vue'

import { mount } from '@vue/test-utils'

import type { ConnectionSummary } from '@/api/generated/ConnectionSummary'

import ObjectExplorer from '../ObjectExplorer.vue'

/**
 * Connections returned by the simulated server.
 */
let savedConnections: ConnectionSummary[] = []

vi.mock('@/api/connections', () => ({
  listConnections: async () => savedConnections,
  probeConnection: async () => ({ flavor: 'mysql', version: '8.4.0', ansi_quotes: false }),
}))

vi.mock('@/api/explorer', () => ({
  streamCatalog: async (_: number, onEvent: (event: unknown) => void) => {
    onEvent({ type: 'schemas', schemas: ['a', 'b', 'c'] })
    onEvent({ type: 'done' })
  },
}))

/**
 * Gives a size to the elements: without it, jsdom measures nothing and the
 * tree's virtual scrolling shows no node.
 */
function giveLayout(): void {
  const size = { width: 300, height: 500, top: 0, left: 0, right: 300, bottom: 500, x: 0, y: 0 }
  vi.spyOn(HTMLElement.prototype, 'offsetHeight', 'get').mockReturnValue(500)
  vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockReturnValue({
    ...size,
    toJSON: () => size,
  })
  vi.stubGlobal(
    'ResizeObserver',
    class {
      constructor(private readonly callback: (entries: unknown[]) => void) {}
      observe(target: Element) {
        this.callback([
          { target, contentRect: size, borderBoxSize: [{ inlineSize: 300, blockSize: 500 }] },
        ])
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
    host: 'localhost',
    port: 3306,
    username: 'root',
    database_name: '',
    init_sql: '',
    session_timeout_minutes: 5,
    show_system_schemas: true,
    server_flavor: null,
    created_at: '2026-10-13T00:00:00Z',
  }
}

/**
 * Mounts the explorer, attached to the page (the menus open in `body`).
 *
 * @param props Properties of the explorer.
 */
function mountExplorer(props: { selectedId: number | null; currentDatabase: string | null }) {
  return mount(ObjectExplorer, { props, attachTo: document.body })
}

/**
 * Labels of the displayed lines.
 *
 * @param wrapper Mounted explorer.
 */
function labels(wrapper: ReturnType<typeof mountExplorer>): string[] {
  return wrapper.findAll('.tree-label').map((node) => node.text())
}

/**
 * Line of the tree whose label is given.
 *
 * @param wrapper Mounted explorer.
 * @param label Searched label.
 */
function row(wrapper: ReturnType<typeof mountExplorer>, label: string) {
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

beforeEach(() => {
  setActivePinia(createPinia())
  giveLayout()
})

afterEach(() => {
  vi.restoreAllMocks()
  vi.unstubAllGlobals()
})

describe('ObjectExplorer', () => {
  it('puts the sources under a root and opens that of the chosen connection', async () => {
    savedConnections = [connection(1, 'Origen 2'), connection(2, 'Autre')]
    const wrapper = mountExplorer({ selectedId: 1, currentDatabase: null })
    await vi.waitFor(() =>
      expect(labels(wrapper)).toEqual([
        'Sources de données',
        'Origen 2',
        'a',
        'b',
        'c',
        'Autre',
        'Notebooks',
      ]),
    )
    wrapper.unmount()
  })

  it('offers a menu on the root and another one on each source', async () => {
    savedConnections = [connection(1, 'Origen 2')]
    const wrapper = mountExplorer({ selectedId: null, currentDatabase: null })
    await vi.waitFor(() => expect(wrapper.findAll('.node-menu')).toHaveLength(3))
    const [root, source] = wrapper.findAll('.node-menu')
    const options = () =>
      [...document.querySelectorAll('[role="menuitem"]')].map((item) => item.textContent?.trim())
    await click(root, 1)
    await vi.waitFor(() => expect(options()).toEqual(['Ajouter une connexion', 'Tout rafraîchir']))
    await click(root, 1)
    await click(source, 1)
    await vi.waitFor(() =>
      expect(options()).toEqual(['Modifier', 'Rafraîchir', 'Tester', 'Supprimer']),
    )
    wrapper.unmount()
  })

  it('opens a source with a click on its line and chooses it with a double click', async () => {
    savedConnections = [connection(1, 'Origen 2'), connection(2, 'Autre')]
    const wrapper = mountExplorer({ selectedId: 1, currentDatabase: null })
    await vi.waitFor(() => expect(row(wrapper, 'Autre')).toBeDefined())
    await click(row(wrapper, 'Autre'), 1)
    expect(wrapper.emitted('select')).toBeUndefined()
    // Open: its first database follows the source.
    const texts = () => labels(wrapper)
    await vi.waitFor(() => expect(texts()[texts().indexOf('Autre') + 1]).toBe('a'))
    // Second click of a double click: the source is chosen.
    await click(row(wrapper, 'Autre'), 2)
    expect(wrapper.emitted('select')).toEqual([[2]])
    // Double click on a database: it becomes the current database of its source.
    await click(row(wrapper, 'a'), 2)
    expect(wrapper.emitted('use-database')).toEqual([[1, 'a']])
    wrapper.unmount()
  })

  it('follows the current database, even when going back to a database already chosen', async () => {
    savedConnections = [connection(1, 'Origen 2'), connection(2, 'Autre')]
    const wrapper = mountExplorer({ selectedId: 1, currentDatabase: 'a' })
    const bold = () => wrapper.findAll('.current-database').map((node) => node.text())
    await vi.waitFor(() => expect(bold()).toEqual(['a']))

    for (const database of ['b', 'a', 'c', 'a']) {
      await wrapper.setProps({ currentDatabase: database })
      await vi.waitFor(() => expect(bold()).toEqual([database]))
    }
    wrapper.unmount()
  })
})
