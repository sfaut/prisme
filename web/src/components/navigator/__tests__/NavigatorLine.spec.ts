import { describe, expect, it } from 'vitest'

import { mount } from '@vue/test-utils'

import type { SessionStatus } from '@/api/querySession'

import NavigatorLine from '../NavigatorLine.vue'
import type { NavigatorNode } from '../navigatorNodes'

/**
 * Mounts the line of a node.
 *
 * @param node Node shown on the line.
 * @param session Stage of the active editor's session on the node's source.
 */
function mountLine(node: NavigatorNode, session: SessionStatus | null = null) {
  return mount(NavigatorLine, { props: { node, expanded: false, menu: null, session } })
}

const place = { connectionId: 1, schema: 'shop' }

describe('NavigatorLine', () => {
  it('gives a database a button that makes it the current database', async () => {
    const wrapper = mountLine({
      key: 'db',
      label: 'shop',
      isLeaf: false,
      role: 'database',
      usable: true,
      ...place,
    })
    await wrapper.find('[aria-label="Utiliser cette base"]').trigger('click')
    expect(wrapper.emitted('act')).toHaveLength(1)
  })

  it('gives no such button to a database of a source that does not change it', () => {
    const wrapper = mountLine({
      key: 'db',
      label: 'main',
      isLeaf: false,
      role: 'database',
      usable: false,
      ...place,
    })
    expect(wrapper.find('[aria-label="Utiliser cette base"]').exists()).toBe(false)
  })

  it('gives a table a button that shows its content', async () => {
    const wrapper = mountLine({
      key: 'table',
      label: 'orders',
      isLeaf: false,
      entry: 'table',
      table: 'orders',
      ...place,
    })
    await wrapper.find('[aria-label="Afficher le contenu"]').trigger('click')
    expect(wrapper.emitted('act')).toHaveLength(1)
  })

  it('gives a "Tables" folder its two orders, the chosen one pressed', async () => {
    const wrapper = mountLine({
      key: 'folder:tables:1/shop',
      label: 'Tables (2)',
      isLeaf: false,
      entry: 'folder',
      tableSort: 'name',
      ...place,
    })
    const byName = wrapper.find('[aria-label="Trier par nom"]')
    const byRows = wrapper.find('[aria-label="Trier par nombre de lignes"]')
    expect(byName.attributes('aria-pressed')).toBe('true')
    expect(byRows.attributes('aria-pressed')).toBe('false')
    // Sorted by name, both wait for the hover.
    expect(byRows.classes()).toContain('invisible')

    await byName.trigger('click')
    expect(wrapper.emitted('sort-tables')).toBeUndefined()
    await byRows.trigger('click')
    expect(wrapper.emitted('sort-tables')).toEqual([['rows']])

    // Sorted by row count, its button stays shown; the other waits for the hover.
    await wrapper.setProps({ node: { ...wrapper.props('node'), tableSort: 'rows' } })
    expect(byRows.classes()).not.toContain('invisible')
    expect(byName.classes()).toContain('invisible')
    await byName.trigger('click')
    expect(wrapper.emitted('sort-tables')).toEqual([['rows'], ['name']])
  })

  it('gives no order to another folder', () => {
    const wrapper = mountLine({
      key: 'folder:views:1/shop',
      label: 'Vues (2)',
      isLeaf: false,
      entry: 'folder',
      ...place,
    })
    expect(wrapper.find('[role="group"]').exists()).toBe(false)
  })

  it('gives an index a button that writes the operations on it', async () => {
    const wrapper = mountLine({
      key: 'index',
      label: 'client_id (client_id)',
      isLeaf: true,
      entry: 'index',
      index: { table: 'orders', name: 'client_id' },
      ...place,
    })
    await wrapper.find('[aria-label="Opérations sur l’index"]').trigger('click')
    expect(wrapper.emitted('act')).toHaveLength(1)
  })

  it('gives a source a disconnect button, enabled once its session is ready', async () => {
    const source: NavigatorNode = {
      key: 'source',
      label: 'visual',
      isLeaf: false,
      role: 'source',
      connectionId: 1,
      schema: '',
    }
    const button = (session: SessionStatus | null) =>
      mountLine(source, session).find('[aria-label="Déconnecter"]')
    expect(button(null).attributes('disabled')).toBeDefined()
    expect(button('connecting').attributes('disabled')).toBeDefined()
    expect(button('running').attributes('data-tooltip-description')).toContain(
      'Arrêtez d’abord les requêtes',
    )
    expect(button('running').attributes('disabled')).toBeDefined()

    const wrapper = mountLine(source, 'ready')
    await wrapper.find('[aria-label="Déconnecter"]').trigger('click')
    expect(wrapper.emitted('disconnect')).toHaveLength(1)
    expect(wrapper.emitted('act')).toBeUndefined()
  })

  it('shows a spinning circle in place of the icon of a source while its catalog arrives', () => {
    const icon = (receiving: boolean) =>
      mountLine({
        key: 'source',
        label: 'visual',
        isLeaf: false,
        role: 'source',
        receiving,
        connectionId: 1,
        schema: '',
      }).find('svg')
    expect(icon(false).classes()).toContain('lucide-server')
    expect(icon(true).classes()).toEqual(
      expect.arrayContaining(['lucide-loader-circle', 'animate-spin']),
    )
    expect(icon(true).attributes('aria-label')).toBe('Chargement')
  })

  it('gives the current database the disconnect button instead of its plug', async () => {
    const wrapper = mountLine(
      { key: 'db', label: 'shop', isLeaf: false, role: 'database', current: true, ...place },
      'ready',
    )
    expect(wrapper.find('[aria-label="Utiliser cette base"]').exists()).toBe(false)
    await wrapper.find('[aria-label="Déconnecter"]').trigger('click')
    expect(wrapper.emitted('disconnect')).toHaveLength(1)
  })

  it('gives a column one icon: a key, a link or the column icon', () => {
    const icon = (primaryKey: boolean, foreignKey: boolean) =>
      mountLine({
        key: 'column',
        label: 'id',
        isLeaf: true,
        entry: 'column',
        column: { name: 'id', details: 'INT', nameWidth: 2, primaryKey, indexed: true, foreignKey },
        ...place,
      }).findAll('svg')
    const classes = (primaryKey: boolean, foreignKey: boolean) =>
      icon(primaryKey, foreignKey).map((svg) =>
        svg.classes().find((name) => name.startsWith('lucide-')),
      )
    expect(classes(true, false)).toEqual(['lucide-key-round'])
    expect(classes(false, true)).toEqual(['lucide-link'])
    expect(classes(true, true)).toEqual(['lucide-key-round'])
    expect(classes(false, false)).toEqual(['lucide-columns-3'])
    expect(icon(true, false)[0]?.attributes('aria-label')).toBe('Clef primaire')
    expect(icon(false, true)[0]?.attributes('aria-label')).toBe('Clef étrangère')
  })

  it('gives no button to the other nodes', () => {
    const wrapper = mountLine({
      key: 'column',
      label: 'id',
      isLeaf: true,
      entry: 'column',
      ...place,
    })
    expect(wrapper.find('button').exists()).toBe(false)
  })
})
