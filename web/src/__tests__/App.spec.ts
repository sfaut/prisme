import { describe, it, expect } from 'vitest'

import { mount } from '@vue/test-utils'
import App from '../App.vue'

describe('App', () => {
  it('affiche le nom de l’application', () => {
    const wrapper = mount(App)
    expect(wrapper.text()).toContain('Prisme')
  })
})
