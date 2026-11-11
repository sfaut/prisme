import { describe, expect, it } from 'vitest'

import { mount } from '@vue/test-utils'

import type { UserFilter } from '@/api/querySession'

import ResultFilter from '../ResultFilter.vue'

/**
 * Mounts the form on a filter, in the page so that it can take the focus.
 *
 * @param filter User filter.
 */
function mountFilter(filter: UserFilter) {
  return mount(ResultFilter, {
    props: { modelValue: filter, columns: ['id', 'name'] },
    attachTo: document.body,
  })
}

describe('ResultFilter', () => {
  it('gives the focus to its field when it appears', () => {
    const wrapper = mountFilter({ text: '', column: null })
    expect(document.activeElement).toBe(wrapper.find('input').element)
    wrapper.unmount()
  })

  it('replaces the filter with the typed text, its column kept', async () => {
    const wrapper = mountFilter({ text: '', column: 1 })
    await wrapper.find('input').setValue('ada')
    expect(wrapper.emitted('update:modelValue')).toEqual([[{ text: 'ada', column: 1 }]])
    wrapper.unmount()
  })

  it('empties the text by its button, shown only with a text', async () => {
    const empty = mountFilter({ text: '', column: null })
    expect(empty.find('[aria-label="Vider le filtre"]').exists()).toBe(false)
    empty.unmount()

    const wrapper = mountFilter({ text: 'ada', column: null })
    await wrapper.find('[aria-label="Vider le filtre"]').trigger('click')
    expect(wrapper.emitted('update:modelValue')).toEqual([[{ text: '', column: null }]])
    wrapper.unmount()
  })
})
