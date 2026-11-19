import { mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'

import { useConsoleStore } from '@/stores/console'
import { useEditorLayoutsStore } from '@/stores/editorLayouts'
import { useExplorerStore } from '@/stores/explorer'
import { useNavigatorStore } from '@/stores/navigator'

import PanelActivityBar from '../PanelActivityBar.vue'

beforeEach(() => {
  window.localStorage.clear()
  setActivePinia(createPinia())
})

afterEach(() => {
  window.localStorage.clear()
})

/**
 * Activity bar of the editor 1.
 */
function mountBar() {
  return mount(PanelActivityBar, { props: { editorId: 1 } })
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

  it('opens and closes a panel, its tooltip saying what the click does', async () => {
    const wrapper = mountBar()
    const results = () => wrapper.find('button[aria-label="Résultat"]')
    expect(results().attributes('data-tooltip')).toBe('Ouvrir le résultat')
    await results().trigger('click')
    expect(useEditorLayoutsStore().layoutOf(1).resultsFolded).toBe(false)
    expect(results().attributes('aria-pressed')).toBe('true')
    expect(results().attributes('data-tooltip')).toBe('Fermer le résultat')

    await wrapper.find('button[aria-label="Navigateur d’objets"]').trigger('click')
    expect(useNavigatorStore().layoutOf(1).open).toBe(false)
    await wrapper.find('button[aria-label="Explorateur de données"]').trigger('click')
    expect(useExplorerStore().layoutOf(1).open).toBe(true)
    await wrapper.find('button[aria-label="Console"]').trigger('click')
    expect(useConsoleStore().open).toBe(false)
    wrapper.unmount()
  })

  it('shares the room again: equal heights, default sizes on the sides', async () => {
    const wrapper = mountBar()
    const view = useEditorLayoutsStore().layoutOf(1)
    view.results = 300
    view.pivot = 200
    useNavigatorStore().layoutOf(1).width = 500
    useExplorerStore().layoutOf(1).width = 600
    useConsoleStore().height = 400

    await wrapper.find('button[aria-label="Répartir les volets"]').trigger('click')
    expect(view.results).toBeNull()
    expect(view.pivot).toBeNull()
    expect(useNavigatorStore().layoutOf(1).width).toBeNull()
    expect(useExplorerStore().layoutOf(1).width).toBeNull()
    expect(useConsoleStore().height).toBeNull()
    wrapper.unmount()
  })
})
