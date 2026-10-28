import { createPinia, setActivePinia } from 'pinia'
import { beforeEach, describe, expect, it } from 'vitest'

import { mount } from '@vue/test-utils'

import { type QueryTab, formatInstant } from '@/api/querySession'

import QueryTabView from '../QueryTabView.vue'
import ResultFilter from '../ResultFilter.vue'
import UiStatusBar from '../ui/UiStatusBar.vue'
import { finishedTab } from './resultTabs'

beforeEach(() => setActivePinia(createPinia()))

/**
 * Mounts the content of a tab, with its filter.
 *
 * @param tab Tab of the query.
 */
function mountTab(tab: QueryTab) {
  return mount(QueryTabView, { props: { tab, displayed: false, filter: tab.filter } })
}

describe('QueryTabView', () => {
  it('shows the status of the result in its bar, one item each', async () => {
    const tab = finishedTab(['id'], [['1'], ['2']])
    const wrapper = mountTab(tab)
    const items = () =>
      wrapper
        .findComponent(UiStatusBar)
        .findAll('span')
        .map((item) => item.text())
    const instant = formatInstant(1_792_500_000_000)
    expect(items()).toEqual([
      '2 lignes',
      'exécution 2 ms',
      'transfert 3 ms',
      '120 o',
      '40,0 ko/s',
      instant,
    ])

    await wrapper.setProps({ filter: { text: '2', column: null } })
    expect(items()[0]).toBe('1 / 2 lignes')
  })

  it('shows the durations alone without a grid', () => {
    const wrapper = mountTab(finishedTab([]))
    const items = wrapper.findComponent(UiStatusBar).findAll('span')
    expect(items.map((item) => item.text())).toEqual(['exécution 3 ms'])
  })

  it('shows the form of the filter while it is expanded, and the grid under it', async () => {
    const tab = finishedTab(['id'], [['1']])
    const wrapper = mountTab(tab)
    expect(wrapper.findComponent(ResultFilter).exists()).toBe(false)
    expect(wrapper.find('.grid-scroller').classes()).toContain('attached')

    await wrapper.setProps({ tab: { ...tab, filterOpen: true } })
    expect(wrapper.findComponent(ResultFilter).exists()).toBe(true)
    expect(wrapper.find('.grid-scroller').classes()).not.toContain('attached')
  })

  it('makes a targeted filter global for new columns, and keeps it for the same ones', async () => {
    const tab = { ...finishedTab(['id'], [['1']]), filter: { text: 'a', column: 0 } }
    const wrapper = mountTab(tab)

    await wrapper.setProps({ tab: { ...tab, result: finishedTab(['id'], [['2']]).result } })
    expect(wrapper.emitted('update:filter')).toBeUndefined()

    await wrapper.setProps({ tab: { ...tab, result: finishedTab(['name'], [['2']]).result } })
    expect(wrapper.emitted('update:filter')).toEqual([[{ text: 'a', column: null }]])
  })
})
