import { describe, expect, it } from 'vitest'

import { mount } from '@vue/test-utils'
import SqlEditor from '../SqlEditor.vue'

describe('SqlEditor', () => {
  it('affiche le texte lié', () => {
    const wrapper = mount(SqlEditor, {
      props: { flavor: 'mysql', modelValue: 'SELECT 1' },
      attachTo: document.body,
    })
    expect(wrapper.find('.cm-content').text()).toContain('SELECT 1')
    wrapper.unmount()
  })

  it('remplace le contenu quand le texte change de l’extérieur', async () => {
    const wrapper = mount(SqlEditor, {
      props: { flavor: 'mariadb', modelValue: 'SELECT 1' },
      attachTo: document.body,
    })
    await wrapper.setProps({ modelValue: 'SHOW TABLES' })
    expect(wrapper.find('.cm-content').text()).toContain('SHOW TABLES')
    wrapper.unmount()
  })
})
