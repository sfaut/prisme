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
      ...place,
    })
    await wrapper.find('[aria-label="Utiliser cette base"]').trigger('click')
    expect(wrapper.emitted('act')).toHaveLength(1)
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
    expect(button('running').attributes('data-tooltip')).toContain('arrêtez d’abord les requêtes')
    expect(button('running').attributes('disabled')).toBeDefined()

    const wrapper = mountLine(source, 'ready')
    await wrapper.find('[aria-label="Déconnecter"]').trigger('click')
    expect(wrapper.emitted('disconnect')).toHaveLength(1)
    expect(wrapper.emitted('act')).toBeUndefined()
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
