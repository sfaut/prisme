import { flushPromises, mount } from '@vue/test-utils'
import { Code, Server } from '@lucide/vue'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import UiSplitButton from '../UiSplitButton.vue'

beforeEach(() => {
  document.body.innerHTML = ''
})

describe('UiSplitButton', () => {
  it('does the main action on its main part, the others from its menu', async () => {
    const editor = vi.fn<() => void>()
    const connection = vi.fn<() => void>()
    const wrapper = mount(UiSplitButton, {
      props: {
        label: 'Nouveau',
        menuLabel: 'Choisir quoi créer',
        items: [
          { label: 'Éditeur', icon: Code, onSelect: editor },
          { label: 'Connexion', icon: Server, onSelect: connection },
        ],
      },
      attachTo: document.body,
    })

    await wrapper.find('button:not([aria-label])').trigger('click')
    expect(wrapper.emitted('click')).toHaveLength(1)
    expect(document.querySelector('[role="menu"]')).toBeNull()

    // The chevron opens the menu, as with the keyboard.
    const chevron = wrapper.find('[aria-label="Choisir quoi créer"]').element
    chevron.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
    const labels = () =>
      [...document.querySelectorAll('[role="menuitem"]')].map((item) => item.textContent?.trim())
    await vi.waitFor(() => expect(labels()).toEqual(['Éditeur', 'Connexion']))
    const items = [...document.querySelectorAll<HTMLElement>('[role="menuitem"]')]

    items[1]?.click()
    await flushPromises()
    expect(connection).toHaveBeenCalledOnce()
    expect(editor).not.toHaveBeenCalled()
    expect(wrapper.emitted('click')).toHaveLength(1)
    wrapper.unmount()
  })
})
