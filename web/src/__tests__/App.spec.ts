import { createPinia, setActivePinia } from 'pinia'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { mount } from '@vue/test-utils'
import App from '../App.vue'

vi.mock('@/api/connections', () => ({
  listConnections: async () => [],
}))

beforeEach(() => {
  setActivePinia(createPinia())
  // L'arbre de l'explorateur (défilement virtuel) interroge les media queries, absentes de jsdom.
  vi.stubGlobal('matchMedia', () => ({
    matches: false,
    addEventListener() {},
    removeEventListener() {},
  }))
})

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('App', () => {
  it("affiche le nom de l'application et le choix de connexion", () => {
    const wrapper = mount(App)
    expect(wrapper.text()).toContain('Prisme')
    expect(wrapper.text()).toContain('Nouvelle')
  })
})
