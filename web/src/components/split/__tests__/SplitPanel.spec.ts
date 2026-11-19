import { afterEach, describe, expect, it } from 'vitest'
import { defineComponent, h, nextTick, ref } from 'vue'

import { type VueWrapper, mount } from '@vue/test-utils'

import SplitGroup from '../SplitGroup.vue'
import SplitHandle from '../SplitHandle.vue'
import SplitPanel from '../SplitPanel.vue'

let wrapper: VueWrapper | null = null

afterEach(() => {
  wrapper?.unmount()
  wrapper = null
})

/**
 * Style of a panel alone in a vertical group.
 *
 * @param props Properties of the panel.
 */
function panelStyle(props: InstanceType<typeof SplitPanel>['$props']): CSSStyleDeclaration {
  const Group = defineComponent({
    setup: () => () => h(SplitGroup, { direction: 'vertical' }, () => h(SplitPanel, props)),
  })
  wrapper = mount(Group)
  return (wrapper.find('#panel').element as HTMLElement).style
}

describe('SplitPanel', () => {
  it('fills the room left by the others', () => {
    expect(panelStyle({ id: 'panel', fill: true, size: 200 }).flex).toBe('1 1 0px')
  })

  it('takes its kept size, otherwise its default size', () => {
    expect(panelStyle({ id: 'panel', size: 200, defaultSize: '10rem' }).flex).toBe('0 1 200px')
    wrapper?.unmount()
    expect(panelStyle({ id: 'panel', size: null, defaultSize: '10rem' }).flex).toBe('0 1 10rem')
  })

  it('shares the room in equal parts without a default size nor a kept size', () => {
    expect(panelStyle({ id: 'panel', size: null }).flex).toBe('1 1 0px')
    wrapper?.unmount()
    expect(panelStyle({ id: 'panel', size: 120 }).flex).toBe('0 1 120px')
  })

  it('takes its folded size exactly when folded', () => {
    const style = panelStyle({ id: 'panel', size: null, folded: true, foldedSize: '3rem' })
    expect(style.flex).toBe('0 0 3rem')
  })

  it('marks its group as resizing during the drag of a line, which it follows without a transition', async () => {
    const Group = defineComponent({
      setup: () => () =>
        h(SplitGroup, { direction: 'vertical' }, () => [
          h(SplitPanel, { id: 'panel', size: 120 }),
          h(SplitHandle, { controls: 'panel', tooltip: 'Ligne' }),
          h(SplitPanel, { id: 'fill', fill: true }),
        ]),
    })
    wrapper = mount(Group, { attachTo: document.body })
    const group = wrapper.find('.split-group')
    const handle = wrapper.find('[role="separator"]')
    // jsdom has neither the capture of the pointer nor its events.
    const element = handle.element as HTMLElement
    element.setPointerCapture = () => {}
    /**
     * Event of the main pointer.
     *
     * @param type Type of the event.
     */
    const pointer = (type: string) =>
      Object.assign(new MouseEvent(type, { button: 0, clientY: 100 }), {
        isPrimary: true,
        pointerId: 1,
      })
    expect(group.attributes('data-resizing')).toBeUndefined()
    element.dispatchEvent(pointer('pointerdown'))
    await nextTick()
    expect(group.attributes('data-resizing')).toBe('true')
    element.dispatchEvent(pointer('pointerup'))
    await nextTick()
    expect(group.attributes('data-resizing')).toBeUndefined()
  })

  it('ends the drag of a line removed while it is dragged', async () => {
    const shown = ref(true)
    const Group = defineComponent({
      setup: () => () =>
        h(SplitGroup, { direction: 'vertical' }, () => [
          h(SplitPanel, { id: 'fill', fill: true }),
          h(SplitPanel, { id: 'panel', size: 120 }),
          shown.value ? h(SplitHandle, { controls: 'panel', tooltip: 'Ligne' }) : null,
        ]),
    })
    wrapper = mount(Group, { attachTo: document.body })
    const element = wrapper.find('[role="separator"]').element as HTMLElement
    // jsdom has neither the capture of the pointer nor its events.
    element.setPointerCapture = () => {}
    element.dispatchEvent(
      Object.assign(new MouseEvent('pointerdown', { button: 0, clientY: 100 }), {
        isPrimary: true,
        pointerId: 1,
      }),
    )
    await nextTick()
    expect(wrapper.find('.split-group').attributes('data-resizing')).toBe('true')

    shown.value = false
    await nextTick()
    expect(wrapper.find('[role="separator"]').exists()).toBe(false)
    expect(wrapper.find('.split-group').attributes('data-resizing')).toBeUndefined()
  })
})
