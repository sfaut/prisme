import { TabsList, TabsRoot } from 'reka-ui'
import { afterEach, describe, expect, it } from 'vitest'
import { defineComponent, h } from 'vue'

import { type VueWrapper, mount } from '@vue/test-utils'

import type { TabProgress } from '@/api/querySession'

import DocumentTab from '../DocumentTab.vue'
import { useTabDrag } from '../tabDrag'

let wrapper: VueWrapper | null = null

afterEach(() => {
  wrapper?.unmount()
  wrapper = null
})

/**
 * Mounts a tab alone in its bar, with a progress.
 *
 * @param progress Progress of the tab's work, absent for none.
 * @param fraction Completed share.
 */
function mountTab(progress?: TabProgress, fraction?: number | null): VueWrapper {
  const Bar = defineComponent({
    setup() {
      const drag = useTabDrag(() => {})
      return () =>
        h(TabsRoot, { modelValue: 1 }, () =>
          h(TabsList, () =>
            h(
              DocumentTab,
              {
                value: 1,
                index: 0,
                count: 1,
                drag,
                closeLabel: 'Fermer',
                closeTooltip: 'Fermer',
                progress,
                fraction,
              },
              () => 'onglet',
            ),
          ),
        )
    },
  })
  return mount(Bar, { attachTo: document.body })
}

describe('DocumentTab', () => {
  it('shows neither dot nor bar without progress', () => {
    wrapper = mountTab()
    expect(wrapper.find('.progress-dot').exists()).toBe(false)
    expect(wrapper.find('.tab-progress').exists()).toBe(false)
  })

  it('shows the dot, without a bar, while the work waits for its turn', () => {
    wrapper = mountTab('waiting', 0)
    expect(wrapper.find('.progress-dot').classes()).toContain('waiting')
    expect(wrapper.find('.tab-progress').exists()).toBe(false)
  })

  it('fills the bar by the completed share, indeterminate when unknown', () => {
    wrapper = mountTab('loading', 0.5)
    expect(wrapper.find('.progress-dot').classes()).toContain('loading')
    const bar = wrapper.find('.tab-progress-bar')
    expect(bar.classes()).toContain('primary')
    expect(bar.attributes('style')).toContain('scaleX(0.5)')
    wrapper.unmount()

    wrapper = mountTab('executing', null)
    expect(wrapper.find('.tab-progress-bar').classes()).toContain('indeterminate')
  })

  it('colors the bar once the work has ended', () => {
    wrapper = mountTab('done', 1)
    expect(wrapper.find('.tab-progress-bar').classes()).toContain('success')
    wrapper.unmount()

    wrapper = mountTab('error', 1)
    expect(wrapper.find('.tab-progress-bar').classes()).toContain('error')
  })
})
