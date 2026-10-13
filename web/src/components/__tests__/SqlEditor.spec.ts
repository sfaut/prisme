import { describe, expect, it, vi } from 'vitest'

import { mount } from '@vue/test-utils'
import SqlEditor from '../SqlEditor.vue'

vi.mock('@/api/analyze', () => ({
  analyzeSql: async () => ({ statements: [] }),
}))

describe('SqlEditor', () => {
  it('affiche le texte lié', () => {
    const wrapper = mount(SqlEditor, {
      props: { flavor: 'mysql', schema: {}, defaultSchema: null, modelValue: 'SELECT 1' },
      attachTo: document.body,
    })
    expect(wrapper.find('.cm-content').text()).toContain('SELECT 1')
    wrapper.unmount()
  })

  it('remplace le contenu quand le texte change de l’extérieur', async () => {
    const wrapper = mount(SqlEditor, {
      props: { flavor: 'mariadb', schema: {}, defaultSchema: null, modelValue: 'SELECT 1' },
      attachTo: document.body,
    })
    await wrapper.setProps({ modelValue: 'SHOW TABLES' })
    expect(wrapper.find('.cm-content').text()).toContain('SHOW TABLES')
    wrapper.unmount()
  })
})
