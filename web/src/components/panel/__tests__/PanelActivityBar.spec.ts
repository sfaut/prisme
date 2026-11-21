import { mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { useEditorDisplaysStore } from '@/stores/editorDisplays'

import PanelActivityBar from '../PanelActivityBar.vue'

vi.mock('@/api/editors', () => ({ saveEditorDisplay: async () => {} }))

beforeEach(() => {
  setActivePinia(createPinia())
})

/**
 * Activity bar of the editor 1.
 */
function mountBar() {
  return mount(PanelActivityBar, { props: { editorId: 1, active: true } })
}

/**
 * Panels of the editor 1.
 */
function panels() {
  return useEditorDisplaysStore().displayOf(1).panels
}

describe('PanelActivityBar', () => {
  it('shows a button per panel, pressed while its panel is open', () => {
    const wrapper = mountBar()
    const pressed = wrapper
      .findAll('button[aria-pressed]')
      .map((button) => [button.attributes('aria-label'), button.attributes('aria-pressed')])
    expect(pressed).toEqual([
      ['Navigateur d’objets', 'true'],
      ['Éditeur', 'true'],
      ['Résultat', 'false'],
      ['Pivoteur', 'false'],
      ['Explorateur de données', 'false'],
      ['Console', 'true'],
    ])
    wrapper.unmount()
  })

  it('opens and closes a panel, its tooltip naming it with its shortcut', async () => {
    const wrapper = mountBar()
    const results = () => wrapper.find('button[aria-label="Résultat"]')
    expect(results().attributes('data-tooltip')).toBe('Résultat')
    expect(results().attributes('data-tooltip-shortcut')).toBe('Ctrl-K puis R')
    await results().trigger('click')
    expect(panels().results.open).toBe(true)
    expect(results().attributes('aria-pressed')).toBe('true')
    expect(results().attributes('data-tooltip')).toBe('Résultat')

    await wrapper.find('button[aria-label="Navigateur d’objets"]').trigger('click')
    expect(panels().navigator.open).toBe(false)
    await wrapper.find('button[aria-label="Explorateur de données"]').trigger('click')
    expect(panels().explorer.open).toBe(true)
    await wrapper.find('button[aria-label="Console"]').trigger('click')
    expect(panels().console.open).toBe(false)
    wrapper.unmount()
  })

  it('follows the shortcuts while its editor is active', async () => {
    const wrapper = mountBar()
    const chord = (key: string) => {
      window.dispatchEvent(new KeyboardEvent('keydown', { key: 'k', ctrlKey: true }))
      window.dispatchEvent(new KeyboardEvent('keydown', { key }))
    }
    chord('p')
    expect(panels().pivot.open).toBe(true)
    chord('c')
    expect(panels().console.open).toBe(false)

    await wrapper.setProps({ active: false })
    chord('c')
    expect(panels().console.open).toBe(false)
    wrapper.unmount()
  })

  it('shares the room again: equal heights, default sizes on the sides', async () => {
    const wrapper = mountBar()
    panels().results.size = 300
    panels().pivot.size = 200
    panels().console.size = 400
    panels().navigator.size = 500
    panels().explorer.size = 600

    await wrapper.find('button[aria-label="Répartir les volets"]').trigger('click')
    for (const name of ['results', 'pivot', 'console', 'navigator', 'explorer'] as const) {
      expect(panels()[name].size).toBeNull()
    }
    wrapper.unmount()
  })
})
