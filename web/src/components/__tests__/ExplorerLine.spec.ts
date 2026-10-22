import { describe, expect, it } from 'vitest'

import { mount } from '@vue/test-utils'

import ExplorerLine from '../ExplorerLine.vue'
import type { ExplorerNode } from '../explorerNodes'

/**
 * Mounts the line of a node.
 *
 * @param node Node shown on the line.
 */
function mountLine(node: ExplorerNode) {
  return mount(ExplorerLine, { props: { node, expanded: false, menu: null } })
}

const place = { connectionId: 1, schema: 'shop' }

describe('ExplorerLine', () => {
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
