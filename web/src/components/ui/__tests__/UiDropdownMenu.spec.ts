import { Download, FileCode } from '@lucide/vue'
import { describe, expect, it, vi } from 'vitest'
import { defineComponent, h, nextTick } from 'vue'

import { mount } from '@vue/test-utils'

import type { MenuGroup, MenuItem } from '../menuItem'
import UiDropdownMenu from '../UiDropdownMenu.vue'

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

  it('keeps the menu open after a choice that asks it, or after a checkbox', async () => {
    const tick = vi.fn<() => void>()
    const choose = vi.fn<() => void>()
    const groups: MenuGroup[] = [
      {
        label: 'Agrégat',
        items: [
          { label: 'SUM(montant)', icon: Download, checked: true },
          {
            label: 'GROUP_CONCAT(montant)',
            icon: FileCode,
            checked: false,
            keepOpen: true,
            onSelect: choose,
          },
        ],
      },
      {
        label: 'Concaténation',
        items: [{ label: 'Valeurs distinctes', icon: Download, ticked: false, onSelect: tick }],
      },
    ]
    const wrapper = mount(UiDropdownMenu, {
      props: { groups },
      slots: { default: () => h('button', { type: 'button' }, 'Menu') },
      attachTo: document.body,
    })
    press(wrapper.find('button').element, 'Enter')
    await vi.waitFor(() =>
      expect(document.querySelector('[role="menuitemcheckbox"]')).not.toBeNull(),
    )

    const box = document.querySelector<HTMLElement>('[role="menuitemcheckbox"]')!
    expect(box.getAttribute('aria-checked')).toBe('false')
    expect(box.querySelector('svg')?.classList).toContain('lucide-square')
    box.click()
    expect(tick).toHaveBeenCalledOnce()
    ;[...document.querySelectorAll<HTMLElement>('[role="menuitemradio"]')][1]!.click()
    expect(choose).toHaveBeenCalledOnce()
    await nextTick()
    expect(document.querySelector('[role="menu"]')).not.toBeNull()
    wrapper.unmount()
  })

  it('shows the value of a setting beside its choice, and its radio choices in a submenu', async () => {
    const choose = vi.fn<() => void>()
    const items: MenuItem[] = [
      {
        label: 'Décimales',
        icon: Download,
        value: '2',
        groups: [
          {
            label: 'Décimales',
            items: [
              { label: '1', icon: Download, checked: false, onSelect: choose },
              { label: '2', icon: Download, checked: true },
            ],
          },
        ],
      },
    ]
    const wrapper = mount(UiDropdownMenu, {
      props: { items },
      slots: { default: () => h('button', { type: 'button' }, 'Menu') },
      attachTo: document.body,
    })
    press(wrapper.find('button').element, 'Enter')
    await vi.waitFor(() => expect(shownLabels()).toEqual(['Décimales2']))

    press(document.querySelector('[role="menuitem"]')!, 'ArrowRight')
    await vi.waitFor(() =>
      expect(document.querySelectorAll('[role="menuitemradio"]')).toHaveLength(2),
    )
    const radios = [...document.querySelectorAll<HTMLElement>('[role="menuitemradio"]')]
    expect(radios.map((radio) => radio.dataset.state)).toEqual(['unchecked', 'checked'])
    radios[0]!.click()
    expect(choose).toHaveBeenCalledOnce()
    wrapper.unmount()
  })

  it('keeps the keys of the fields of a submenu, which Escape closes alone', async () => {
    const Fields = defineComponent({
      setup: () => () =>
        h('div', [
          h('input', { 'aria-label': 'De' }),
          h('input', { 'aria-label': 'À' }),
          h('button', { type: 'button' }, 'Effacer'),
        ]),
    })
    const items: MenuItem[] = [
      { label: 'Bornes', icon: Download, value: '1 – 5', panel: { component: Fields, props: {} } },
      { label: 'Retirer', icon: FileCode },
    ]
    const wrapper = mount(UiDropdownMenu, {
      props: { items },
      slots: { default: () => h('button', { type: 'button' }, 'Menu') },
      attachTo: document.body,
    })
    press(wrapper.find('button').element, 'Enter')
    await vi.waitFor(() => expect(shownLabels()).toEqual(['Bornes1 – 5', 'Retirer']))
    const trigger = document.querySelector<HTMLElement>('[role="menuitem"]')!
    trigger.focus()
    press(trigger, 'ArrowRight')
    const from = () => document.querySelector<HTMLInputElement>('input[aria-label="De"]')
    await vi.waitFor(() => expect(document.activeElement).toBe(from()))

    // The arrows stay in the field: the menu does not move among its choices.
    press(from()!, 'ArrowDown')
    press(from()!, 'Home')
    expect(document.activeElement).toBe(from())
    // Tab and Shift+Tab go round the fields.
    const tab = (shiftKey: boolean) =>
      document.activeElement!.dispatchEvent(
        new KeyboardEvent('keydown', { key: 'Tab', shiftKey, bubbles: true, cancelable: true }),
      )
    tab(false)
    expect(document.activeElement?.getAttribute('aria-label')).toBe('À')
    tab(false)
    expect(document.activeElement?.textContent).toBe('Effacer')
    tab(false)
    expect(document.activeElement).toBe(from())
    tab(true)
    expect(document.activeElement?.textContent).toBe('Effacer')

    // Escape closes the submenu alone, and gives the focus back to its choice.
    press(document.activeElement!, 'Escape')
    await vi.waitFor(() => expect(from()).toBeNull())
    expect(document.querySelector('[role="menu"]')).not.toBeNull()
    expect(document.activeElement).toBe(trigger)
    wrapper.unmount()
  })

  it('opens by the code, the submenu of a choice open and its field ready to type', async () => {
    const Field = defineComponent({
      setup: () => () => h('input', { 'aria-label': 'Valeur' }),
    })
    const groups: MenuGroup[] = [
      {
        label: 'Filtre',
        items: [{ label: 'Valeur', icon: Download, panel: { component: Field, props: {} } }],
      },
    ]
    const wrapper = mount(UiDropdownMenu, {
      props: { groups, open: false, openSub: 'Valeur' },
      slots: { default: () => h('button', { type: 'button' }, 'Menu') },
      attachTo: document.body,
    })
    await wrapper.setProps({ open: true })
    await vi.waitFor(() =>
      expect(document.activeElement?.getAttribute('aria-label')).toBe('Valeur'),
    )
    wrapper.unmount()
  })
})
