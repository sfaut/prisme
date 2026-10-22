import { mount } from '@vue/test-utils'
import { describe, expect, it, vi } from 'vitest'
import { reactive } from 'vue'

import type { PagedList } from '@/stores/console'

import ConsoleList from '../ConsoleList.vue'

/**
 * List of three elements, with or without older ones on the server.
 *
 * @param hasOlder Are there older elements on the server?
 */
function listOf(hasOlder: boolean): PagedList<{ id: number }> {
  return reactive({
    entries: [{ id: 3 }, { id: 4 }, { id: 5 }],
    hasOlder,
    error: null,
    reload: vi.fn<() => Promise<void>>(async () => {}),
    loadOlder: vi.fn<() => Promise<void>>(async () => {}),
    add: vi.fn<(entry: { id: number }) => void>(),
    clear: vi.fn<() => Promise<void>>(async () => {}),
    empty: vi.fn<() => void>(),
    dismissError: vi.fn<() => void>(),
  })
}

/**
 * Mounts the area, its scrolling metrics set: 400 pixels of content, 100
 * shown, scrolled to `scrollTop`.
 *
 * @param list Elements of the tab.
 * @param scrollTop Scrolling position, in pixels.
 */
function mountScrolled(list: PagedList<{ id: number }>, scrollTop: number) {
  const wrapper = mount(ConsoleList, {
    props: { list, empty: 'Rien' },
    slots: { default: '<table><tbody><tr><td>ligne</td></tr></tbody></table>' },
  })
  const element = wrapper.element as HTMLElement
  Object.defineProperty(element, 'clientHeight', { value: 100 })
  Object.defineProperty(element, 'scrollHeight', { value: 400 })
  element.scrollTop = scrollTop
  return wrapper
}

describe('ConsoleList', () => {
  it('reads the older elements when scrolled near the top', async () => {
    const list = listOf(true)
    const wrapper = mountScrolled(list, 10)
    await wrapper.trigger('scroll')
    expect(list.loadOlder).toHaveBeenCalledOnce()
  })

  it('reads nothing away from the top, or without older elements', async () => {
    const away = listOf(true)
    await mountScrolled(away, 200).trigger('scroll')
    expect(away.loadOlder).not.toHaveBeenCalled()

    const all = listOf(false)
    await mountScrolled(all, 0).trigger('scroll')
    expect(all.loadOlder).not.toHaveBeenCalled()
  })

  it('holds the table directly, without a button above it', () => {
    const wrapper = mountScrolled(listOf(true), 300)
    expect(wrapper.find('button').exists()).toBe(false)
    expect(wrapper.element.firstElementChild?.tagName).toBe('TABLE')
  })
})
