import { createPinia, setActivePinia } from 'pinia'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { mount } from '@vue/test-utils'

import type { ConnectionSummary } from '@/api/generated/ConnectionSummary'

import ObjectExplorer from '../ObjectExplorer.vue'

/**
 * Connexions renvoyées par le serveur simulé.
 */
let savedConnections: ConnectionSummary[] = []

vi.mock('@/api/connections', () => ({
  listConnections: async () => savedConnections,
}))

vi.mock('@/api/explorer', () => ({
  streamCatalog: async (_: number, onEvent: (event: unknown) => void) => {
    onEvent({ type: 'schemas', schemas: ['a', 'b', 'c'] })
    onEvent({ type: 'done' })
  },
}))

/**
 * Donne une taille aux éléments : sans elle, jsdom ne mesure rien et le
 * défilement virtuel de l'arbre n'affiche aucun nœud.
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
  vi.stubGlobal('matchMedia', () => ({
    matches: false,
    addEventListener() {},
    removeEventListener() {},
  }))
}

/**
 * Connexion enregistrée, pour remplir la liste des sources.
 *
 * @param id Identifiant de la connexion.
 * @param name Nom de la connexion.
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
    created_at: '2026-10-13T00:00:00Z',
  }
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
  it('range les sources sous une racine et ouvre celle de la connexion choisie', async () => {
    savedConnections = [connection(1, 'Origen 2'), connection(2, 'Autre')]
    const wrapper = mount(ObjectExplorer, {
      props: { selectedId: 1, currentDatabase: null },
      attachTo: document.body,
    })
    const texts = () => wrapper.findAll('.n-tree-node-content__text').map((node) => node.text())
    await vi.waitFor(() =>
      expect(texts()).toEqual(['Sources de données', 'Origen 2', 'a', 'b', 'c', 'Autre']),
    )
    wrapper.unmount()
  })

  it('propose un menu sur la racine et un autre sur chaque source', async () => {
    savedConnections = [connection(1, 'Origen 2')]
    const wrapper = mount(ObjectExplorer, {
      props: { selectedId: null, currentDatabase: null },
      attachTo: document.body,
    })
    await vi.waitFor(() => expect(wrapper.findAll('.node-menu')).toHaveLength(2))
    const [root, source] = wrapper.findAll('.node-menu')
    await root?.trigger('click')
    await vi.waitFor(() => expect(document.body.textContent).toContain('Ajouter une connexion'))
    expect(document.body.textContent).toContain('Tout rafraîchir')
    await source?.trigger('click')
    await vi.waitFor(() => expect(document.body.textContent).toContain('Modifier la connexion'))
    expect(document.body.textContent).toContain('Supprimer la connexion')
    expect(document.body.textContent).toContain('Tester')
    wrapper.unmount()
  })

  it('choisit et ouvre une source d’un clic sur sa ligne', async () => {
    savedConnections = [connection(1, 'Origen 2'), connection(2, 'Autre')]
    const wrapper = mount(ObjectExplorer, {
      props: { selectedId: 1, currentDatabase: null },
      attachTo: document.body,
    })
    const node = (label: string) =>
      wrapper
        .findAll('.n-tree-node')
        .find((item) => item.find('.n-tree-node-content__text').text() === label)
    await vi.waitFor(() => expect(node('Autre')).toBeDefined())
    await node('Autre')?.trigger('click')
    expect(wrapper.emitted('select')).toEqual([[2]])
    const texts = () => wrapper.findAll('.n-tree-node-content__text').map((item) => item.text())
    // Ouverte : sa première base suit la source.
    await vi.waitFor(() => expect(texts()[texts().indexOf('Autre') + 1]).toBe('a'))
    wrapper.unmount()
  })

  it('suit la base en cours, même en revenant sur une base déjà choisie', async () => {
    savedConnections = [connection(1, 'Origen 2'), connection(2, 'Autre')]
    const wrapper = mount(ObjectExplorer, {
      props: { selectedId: 1, currentDatabase: 'a' },
      attachTo: document.body,
    })
    const bold = () => wrapper.findAll('.current-database').map((node) => node.text())
    await vi.waitFor(() => expect(bold()).toEqual(['a']))

    for (const database of ['b', 'a', 'c', 'a']) {
      await wrapper.setProps({ currentDatabase: database })
      await vi.waitFor(() => expect(bold()).toEqual([database]))
    }
    wrapper.unmount()
  })
})
