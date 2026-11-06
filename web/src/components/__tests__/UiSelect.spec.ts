import { afterEach, describe, expect, it } from 'vitest'

import { type VueWrapper, mount } from '@vue/test-utils'

import UiSelect from '../ui/UiSelect.vue'

let wrapper: VueWrapper | null = null

afterEach(() => {
  wrapper?.unmount()
  wrapper = null
})

describe('UiSelect', () => {
  it('gives its class to its button, as Reka UI draws no element around it', () => {
    wrapper = mount(UiSelect, {
      attachTo: document.body,
      props: { items: [{ label: 'Clair', value: 'light' }], label: 'Thème', modelValue: 'light' },
      attrs: { class: 'w-38' },
    })
    const button = wrapper.find('button[aria-label="Thème"]')
    expect(button.classes()).toContain('w-38')
    // Its own classes stay.
    expect(button.classes()).toContain('rounded-lg')
  })
})
