import { createPinia, setActivePinia } from 'pinia'
import { beforeEach, describe, expect, it } from 'vitest'

import { mount } from '@vue/test-utils'

import type { QueryTab } from '@/api/querySession'

import QueryTabView from '../QueryTabView.vue'
import ResultFilter from '../ResultFilter.vue'
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
