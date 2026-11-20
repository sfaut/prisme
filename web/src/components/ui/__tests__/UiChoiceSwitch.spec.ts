import { Monitor, Moon, Sun } from '@lucide/vue'
import { afterEach, describe, expect, it } from 'vitest'

import { type VueWrapper, mount } from '@vue/test-utils'

import UiChoiceSwitch from '../UiChoiceSwitch.vue'

const ITEMS = [
  { label: 'Clair', value: 'light', icon: Sun },
  { label: 'Système', value: 'system', icon: Monitor },
  { label: 'Sombre', value: 'dark', icon: Moon },
]

let wrapper: VueWrapper | null = null

afterEach(() => {
  wrapper?.unmount()
  wrapper = null
})

/**
 * Mounts the switch on a value, its model following the choices.
 *
 * @param value Checked value.
 */
function mountSwitch(value: string): VueWrapper {
  const mounted = mount(UiChoiceSwitch, {
    attachTo: document.body,
    props: {
      items: ITEMS,
      label: 'Thème',
      modelValue: value,
      'onUpdate:modelValue': (next: string) => mounted.setProps({ modelValue: next }),
    },
  })
  return mounted
}

describe('UiChoiceSwitch', () => {
  it('names the group and each choice, and checks the current one', () => {
    wrapper = mountSwitch('system')
    expect(wrapper.find('[role="radiogroup"]').attributes('aria-label')).toBe('Thème')
    const radios = wrapper.findAll('[role="radio"]')
    expect(radios.map((radio) => radio.attributes('aria-label'))).toEqual([
      'Clair',
      'Système',
      'Sombre',
    ])
    expect(radios.map((radio) => radio.attributes('aria-checked'))).toEqual([
      'false',
      'true',
      'false',
    ])
  })

  it('places the thumb by the rank of the checked choice', async () => {
    wrapper = mountSwitch('light')
    const group = wrapper.find('[role="radiogroup"]')
    expect((group.element as HTMLElement).style.getPropertyValue('--switch-index')).toBe('0')
    await wrapper.findAll('[role="radio"]')[2]!.trigger('click')
    expect(wrapper.emitted('update:modelValue')).toEqual([['dark']])
    expect((group.element as HTMLElement).style.getPropertyValue('--switch-index')).toBe('2')
  })

  it('hides the thumb when no choice is checked', () => {
    wrapper = mountSwitch('unknown')
    expect(wrapper.find('.switch-thumb').exists()).toBe(false)
  })
})
