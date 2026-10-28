import { describe, expect, it } from 'vitest'

import { mount } from '@vue/test-utils'

import type { QueryTab, RefreshState } from '@/api/querySession'

import ResultTabActions from '../ResultTabActions.vue'
import { finishedTab } from './resultTabs'

/**
 * Mounts the buttons of a tab.
 *
 * @param tab Tab shown.
 * @param refresh Refresh state of the tab.
 */
function mountActions(tab: QueryTab, refresh: RefreshState = 'idle') {
  return mount(ResultTabActions, { props: { tab, refresh } })
}

describe('ResultTabActions', () => {
  it('refreshes the tab, or stops its refresh', async () => {
    const tab = finishedTab(['id'], [['1']])
    const idle = mountActions(tab)
    await idle.find('[aria-label="Rafraîchir"]').trigger('click')
    expect(idle.emitted('refresh')).toHaveLength(1)

    const running = mountActions(tab, 'running')
    await running.find('[aria-label="Arrêter le rafraîchissement"]').trigger('click')
    expect(running.emitted('stop-refresh')).toHaveLength(1)

    const unavailable = mountActions(tab, 'unavailable')
    expect(unavailable.find('[aria-label="Rafraîchir"]').attributes('disabled')).toBeDefined()
  })

  it('refreshes a tab without a grid, which has no filter button', () => {
    const wrapper = mountActions(finishedTab([]))
    expect(wrapper.find('[aria-label="Rafraîchir"]').exists()).toBe(true)
    expect(wrapper.find('[aria-pressed]').exists()).toBe(false)
  })

  it('expands the filter, and shows it pressed, then active', async () => {
    const tab = finishedTab(['id'], [['1']])
    const wrapper = mountActions(tab)
    const button = () => wrapper.find('[aria-pressed]')
    expect(button().attributes('aria-label')).toBe('Afficher le filtre')
    await button().trigger('click')
    expect(wrapper.emitted('toggle-filter')).toHaveLength(1)

    await wrapper.setProps({ tab: { ...tab, filterOpen: true } })
    expect(button().attributes('aria-pressed')).toBe('true')
    expect(button().attributes('aria-label')).toBe('Masquer le filtre')

    await wrapper.setProps({ tab: { ...tab, filter: { text: 'a', column: null } } })
    expect(button().attributes('aria-label')).toBe('Afficher le filtre (filtre actif)')
  })
})
