import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'
import { defineComponent, nextTick, ref } from 'vue'

import { shownTooltip, vTooltip } from '../ui/tooltip'

/**
 * Button whose tooltip is `text`, shown while `visible`.
 */
const text = ref<string | undefined>('Exécuter')
const visible = ref(true)
const Host = defineComponent({
  directives: { tooltip: vTooltip },
  setup: () => ({ text, visible }),
  template: '<div><button v-if="visible" v-tooltip="text">Go</button></div>',
})

describe('v-tooltip', () => {
  it('gives the element its text, follows its changes and takes it away when empty', async () => {
    text.value = 'Exécuter'
    const wrapper = mount(Host)
    const button = wrapper.find('button')
    expect(button.attributes('data-tooltip')).toBe('Exécuter')

    text.value = 'Arrêter'
    await nextTick()
    expect(button.attributes('data-tooltip')).toBe('Arrêter')

    text.value = undefined
    await nextTick()
    expect(button.attributes('data-tooltip')).toBeUndefined()
  })

  it('cuts a long text', () => {
    text.value = 'x'.repeat(5000)
    const wrapper = mount(Host)
    expect(wrapper.find('button').attributes('data-tooltip')).toHaveLength(1001)
  })

  it('updates the shown tooltip, and hides it with its element', async () => {
    text.value = 'Exécuter'
    visible.value = true
    const wrapper = mount(Host)
    shownTooltip.element = wrapper.find('button').element as HTMLElement
    shownTooltip.text = 'Exécuter'

    text.value = 'Arrêter'
    await nextTick()
    expect(shownTooltip.text).toBe('Arrêter')

    visible.value = false
    await nextTick()
    expect(shownTooltip.element).toBeNull()
  })
})
