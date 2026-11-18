import { describe, expect, it } from 'vitest'

import { mount } from '@vue/test-utils'
import { TabsList, TabsRoot } from 'reka-ui'
import { defineComponent, h } from 'vue'

import type { QueryTab } from '@/api/querySession'

import ResultTab from '../ResultTab.vue'
import { useTabDrag } from '@/components/tab/tabDrag'
import { finishedTab } from './resultTabs'

/**
 * Mounts a result tab in its bar.
 *
 * @param tab Query of the tab.
 * @param stoppable Can its query be stopped?
 */
function mountTab(tab: QueryTab, stoppable = false) {
  return mount(
    defineComponent({
      setup() {
        const drag = useTabDrag(() => {})
        return () =>
          h(TabsRoot, { modelValue: tab.id }, () =>
            h(TabsList, null, () =>
              h(ResultTab, { tab, defaultLabel: '1', index: 0, count: 1, drag, stoppable }),
            ),
          )
      },
    }),
  )
}

describe('ResultTab', () => {
  it('scales its progress bar by the completed share, without changing its width', () => {
    // 250 rows received of the 1,000 expected.
    const rows = Array.from({ length: 250 }, (_, index) => [String(index)])
    const tab = { ...finishedTab(['id'], rows), done: false }
    const style = mountTab(tab).find('.tab-progress-bar').attributes('style')
    expect(style).toContain('scaleX(0.25)')
    expect(style).not.toMatch(/inline-size|width/)
  })

  it('offers to stop its query while it can be stopped', async () => {
    const tab = { ...finishedTab(['id']), done: false }
    expect(mountTab(tab).find('[aria-label="Arrêter la requête"]').exists()).toBe(false)

    const wrapper = mountTab(tab, true)
    await wrapper.find('[aria-label="Arrêter la requête"]').trigger('click')
    expect(wrapper.findComponent(ResultTab).emitted('stop')).toHaveLength(1)
  })

  it('shows a stopped query in the warning color', () => {
    const tab = { ...finishedTab(['id']), error: 'Requête interrompue.', interrupted: true }
    const wrapper = mountTab(tab)
    expect(wrapper.find('.progress-dot').classes()).toContain('stopped')
    expect(wrapper.find('.tab-progress-bar').classes()).toContain('warning')
  })
})
