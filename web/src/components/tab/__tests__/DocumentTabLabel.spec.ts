import { afterEach, describe, expect, it } from 'vitest'

import { type VueWrapper, mount } from '@vue/test-utils'

import DocumentTabLabel from '../DocumentTabLabel.vue'

let wrapper: VueWrapper | null = null

afterEach(() => {
  wrapper?.unmount()
  wrapper = null
})

describe('DocumentTabLabel', () => {
  it('renames in a field at the size of the label, its width following the name', async () => {
    wrapper = mount(DocumentTabLabel, {
      attachTo: document.body,
      props: { name: null, defaultLabel: 'local' },
    })
    await wrapper.find('span').trigger('dblclick')
    const input = wrapper.find('input')
    expect(input.element.value).toBe('local')
    expect(input.classes()).toContain('text-base')
    expect(input.classes()).not.toContain('text-sm')
    expect(input.classes()).toContain('field-sizing-content')

    await input.setValue('ventes')
    await input.trigger('keydown', { key: 'Enter' })
    expect(wrapper.emitted('rename')).toEqual([['ventes']])
  })

  it('gives back the default label for an empty name, and cancels with Escape', async () => {
    wrapper = mount(DocumentTabLabel, {
      attachTo: document.body,
      props: { name: 'ventes', defaultLabel: 'local' },
    })
    await wrapper.find('span').trigger('dblclick')
    await wrapper.find('input').setValue('  ')
    await wrapper.find('input').trigger('keydown', { key: 'Enter' })
    expect(wrapper.emitted('rename')).toEqual([[null]])

    await wrapper.find('span').trigger('dblclick')
    await wrapper.find('input').trigger('keydown', { key: 'Escape' })
    expect(wrapper.find('input').exists()).toBe(false)
    expect(wrapper.emitted('rename')).toHaveLength(1)
  })
})
