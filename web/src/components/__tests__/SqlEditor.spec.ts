import { describe, expect, it, vi } from 'vitest'

import { mount } from '@vue/test-utils'
import SqlEditor from '../SqlEditor.vue'

vi.mock('@/api/analyze', () => ({
  analyzeSql: async (sql: string) => ({
    statements: [],
    errors: sql.startsWith('SELEC ')
      ? [{ start: 0, end: 5, message: 'Requête non comprise par Prisme' }]
      : [],
  }),
}))

describe('SqlEditor', () => {
  it('souligne les erreurs de syntaxe signalées par le serveur', async () => {
    const wrapper = mount(SqlEditor, {
      props: { flavor: 'mysql', schema: {}, defaultSchema: null, modelValue: 'SELEC 1' },
      attachTo: document.body,
    })
    await vi.waitFor(() => expect(wrapper.find('.cm-lintRange-error').text()).toBe('SELEC'), {
      timeout: 2000,
    })
    wrapper.unmount()
  })

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
