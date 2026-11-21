import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'
import { defineComponent, nextTick, ref } from 'vue'

import { shownTooltip, tooltipParts, tooltipSideOf, vTooltip } from '../tooltip'

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

  it('puts the tooltip on the right on request', () => {
    const Right = defineComponent({
      directives: { tooltip: vTooltip },
      template:
        '<div><button v-tooltip:right="\'Ouvrir\'">A</button><a v-tooltip="\'B\'">B</a></div>',
    })
    const wrapper = mount(Right)
    const button = wrapper.find('button')
    expect(button.attributes('data-tooltip-side')).toBe('right')
    expect(tooltipSideOf(button.element as HTMLElement)).toBe('right')
    expect(tooltipSideOf(wrapper.find('a').element as HTMLElement)).toBe('top')
  })

  it('keeps the parts of a tooltip apart', () => {
    const Parts = defineComponent({
      directives: { tooltip: vTooltip },
      template: `<button v-tooltip="{ title: 'Arrêter la requête', description: 'Les lignes reçues restent', shortcut: 'Échap' }">S</button>`,
    })
    const button = mount(Parts).find('button')
    expect(button.attributes('data-tooltip')).toBe('Arrêter la requête')
    expect(button.attributes('data-tooltip-description')).toBe('Les lignes reçues restent')
    expect(button.attributes('data-tooltip-shortcut')).toBe('Échap')
  })

  it('gives a string as a title alone, and no other part without a title', () => {
    expect(tooltipParts('Exporter')).toEqual({ title: 'Exporter', description: '', shortcut: '' })
    expect(tooltipParts({ title: '', description: 'Orpheline', shortcut: 'X' })).toEqual({
      title: '',
      description: '',
      shortcut: '',
    })
    expect(tooltipParts(null).title).toBe('')
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
    shownTooltip.title = 'Exécuter'

    text.value = 'Arrêter'
    await nextTick()
    expect(shownTooltip.title).toBe('Arrêter')

    visible.value = false
    await nextTick()
    expect(shownTooltip.element).toBeNull()
  })
})
