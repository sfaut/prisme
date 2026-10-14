import { createPinia, setActivePinia } from 'pinia'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { nextTick } from 'vue'

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
    init_sql: '',
    session_timeout_minutes: 5,
    created_at: '2026-10-13T00:00:00Z',
  }
}

/**
 * Monte l'explorateur, attaché à la page (les menus s'ouvrent dans `body`).
 *
 * @param props Propriétés de l'explorateur.
 */
function mountExplorer(props: { selectedId: number | null; currentDatabase: string | null }) {
  return mount(ObjectExplorer, { props, attachTo: document.body })
}

/**
 * Libellés des lignes affichées.
 *
 * @param wrapper Explorateur monté.
 */
function labels(wrapper: ReturnType<typeof mountExplorer>): string[] {
  return wrapper.findAll('.tree-label').map((node) => node.text())
}

/**
 * Ligne de l'arbre dont le libellé est donné.
 *
 * @param wrapper Explorateur monté.
 * @param label Libellé cherché.
 */
function row(wrapper: ReturnType<typeof mountExplorer>, label: string) {
  return wrapper.findAll('.tree-row').find((item) => item.find('.tree-label').text() === label)
}

/**
 * Clique sur un élément. `detail` vaut 2 pour le second clic d'un double clic.
 *
 * @param element Élément cliqué.
 * @param detail Rang du clic dans la série.
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
  it('range les sources sous une racine et ouvre celle de la connexion choisie', async () => {
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

  it('propose un menu sur la racine et un autre sur chaque source', async () => {
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

  it('ouvre une source d’un clic sur sa ligne et la choisit d’un double clic', async () => {
    savedConnections = [connection(1, 'Origen 2'), connection(2, 'Autre')]
    const wrapper = mountExplorer({ selectedId: 1, currentDatabase: null })
    await vi.waitFor(() => expect(row(wrapper, 'Autre')).toBeDefined())
    await click(row(wrapper, 'Autre'), 1)
    expect(wrapper.emitted('select')).toBeUndefined()
    // Ouverte : sa première base suit la source.
    const texts = () => labels(wrapper)
    await vi.waitFor(() => expect(texts()[texts().indexOf('Autre') + 1]).toBe('a'))
    // Second clic d'un double clic : la source est choisie.
    await click(row(wrapper, 'Autre'), 2)
    expect(wrapper.emitted('select')).toEqual([[2]])
    // Double clic sur une base : elle devient la base en cours de sa source.
    await click(row(wrapper, 'a'), 2)
    expect(wrapper.emitted('use-database')).toEqual([[1, 'a']])
    wrapper.unmount()
  })

  it('suit la base en cours, même en revenant sur une base déjà choisie', async () => {
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
