import { Download, FileCode } from '@lucide/vue'
import { describe, expect, it, vi } from 'vitest'
import { h } from 'vue'

import { mount } from '@vue/test-utils'

import type { MenuGroup, MenuItem } from '../ui/menuItem'
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

  it('gathers choices in groups, each named by its heading', async () => {
    const groups: MenuGroup[] = [
      { label: 'Fichier', items: [{ label: 'En CSV', icon: Download }] },
      { label: 'Éditeur', items: [{ label: 'En CTE', icon: FileCode }] },
    ]
    const wrapper = mount(UiDropdownMenu, {
      props: { groups },
      slots: { default: () => h('button', { type: 'button' }, 'Menu') },
      attachTo: document.body,
    })
    press(wrapper.find('button').element, 'Enter')
    await vi.waitFor(() => expect(shownLabels()).toEqual(['En CSV', 'En CTE']))

    const named = [...document.querySelectorAll('[role="group"]')].map((group) => [
      document.getElementById(group.getAttribute('aria-labelledby') ?? '')?.textContent,
      group.querySelector('[role="menuitem"]')?.textContent?.trim(),
    ])
    expect(named).toEqual([
      ['Fichier', 'En CSV'],
      ['Éditeur', 'En CTE'],
    ])
    wrapper.unmount()
  })

  it('draws exclusive choices as radio choices, the one made checked with its own icon', async () => {
    const choose = vi.fn<() => void>()
    const groups: MenuGroup[] = [
      {
        label: 'Agrégat',
        items: [
          { label: 'SUM(montant)', icon: Download, checked: true },
          { label: 'MAX(montant)', icon: FileCode, checked: false, onSelect: choose },
        ],
      },
      { label: 'Élément', items: [{ label: 'Retirer', icon: Download }] },
    ]
    const wrapper = mount(UiDropdownMenu, {
      props: { groups },
      slots: { default: () => h('button', { type: 'button' }, 'Menu') },
      attachTo: document.body,
    })
    press(wrapper.find('button').element, 'Enter')
    await vi.waitFor(() => expect(shownLabels()).toEqual(['Retirer']))

    const radios = [...document.querySelectorAll('[role="menuitemradio"]')] as HTMLElement[]
    expect(radios.map((radio) => [radio.textContent?.trim(), radio.dataset.state])).toEqual([
      ['SUM(montant)', 'checked'],
      ['MAX(montant)', 'unchecked'],
    ])
    expect(radios[0]?.getAttribute('aria-checked')).toBe('true')
    // The choice made keeps its icon: no other one is drawn.
    expect(radios[0]?.querySelectorAll('svg')).toHaveLength(1)
    expect(radios[0]?.querySelector('svg')?.classList).toContain('lucide-download')
    radios[1]?.click()
    expect(choose).toHaveBeenCalledOnce()
    wrapper.unmount()
  })
})
