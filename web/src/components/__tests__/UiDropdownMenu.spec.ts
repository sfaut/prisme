import { Download, FileCode } from '@lucide/vue'
import { describe, expect, it, vi } from 'vitest'
import { h } from 'vue'

import { mount } from '@vue/test-utils'

import type { MenuItem } from '../ui/menuItem'
import UiDropdownMenu from '../ui/UiDropdownMenu.vue'

/**
 * Labels of the choices shown, menu and submenus together.
 */
function shownLabels(): (string | undefined)[] {
  return [...document.querySelectorAll('[role="menuitem"]')].map((item) => item.textContent?.trim())
}

/**
 * Sends a key to an element, as the keyboard would.
 *
 * @param element Element that has the focus.
 * @param key Key pressed.
 */
function press(element: Element, key: string): void {
  element.dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true }))
}

describe('UiDropdownMenu', () => {
  it('opens a submenu from its choice, whose choices act like the others', async () => {
    const save = vi.fn<() => void>()
    const items: MenuItem[] = [
      { label: 'En CSV', icon: Download },
      {
        label: 'En CTE',
        icon: FileCode,
        items: [
          { label: 'MySQL', icon: FileCode, onSelect: save },
          { label: 'MariaDB', icon: FileCode },
        ],
      },
    ]
    const wrapper = mount(UiDropdownMenu, {
      props: { items },
      slots: { default: () => h('button', { type: 'button' }, 'Menu') },
      attachTo: document.body,
    })
    press(wrapper.find('button').element, 'Enter')
    await vi.waitFor(() => expect(shownLabels()).toEqual(['En CSV', 'En CTE']))

    const trigger = [...document.querySelectorAll('[role="menuitem"]')][1]!
    expect(trigger.getAttribute('aria-haspopup')).toBe('menu')
    press(trigger, 'ArrowRight')
    await vi.waitFor(() => expect(shownLabels()).toEqual(['En CSV', 'En CTE', 'MySQL', 'MariaDB']))

    const mysql = [...document.querySelectorAll('[role="menuitem"]')][2] as HTMLElement
    mysql.click()
    expect(save).toHaveBeenCalledOnce()
    wrapper.unmount()
  })
})
