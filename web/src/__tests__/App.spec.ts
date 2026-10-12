import { createPinia, setActivePinia } from 'pinia'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { mount } from '@vue/test-utils'
import App from '../App.vue'

vi.mock('@/api/connections', () => ({
  listConnections: async () => [],
}))

beforeEach(() => {
  setActivePinia(createPinia())
})

describe('App', () => {
  it("affiche le nom de l'application et le choix de connexion", () => {
    const wrapper = mount(App)
    expect(wrapper.text()).toContain('Prisme')
    expect(wrapper.text()).toContain('Nouvelle')
  })
})
