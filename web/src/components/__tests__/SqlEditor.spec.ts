import { describe, expect, it, vi } from 'vitest'

import { mount } from '@vue/test-utils'
import SqlEditor from '../SqlEditor.vue'

vi.mock('@/api/analyze', () => ({
  analyzeSql: async (sql: string) => ({
    statements: [],
    errors: sql.startsWith('SELEC ')
      ? [{ start: 0, end: 5, message: 'Query not understood by Prisme' }]
      : [],
  }),
}))

describe('SqlEditor', () => {
  it('underlines the syntax errors reported by the server', async () => {
    const wrapper = mount(SqlEditor, {
      props: {
        flavor: 'mysql',
        schema: {},
        version: null,
        ansiQuotes: false,
        defaultSchema: null,
        modelValue: 'SELEC 1',
      },
      attachTo: document.body,
    })
    await vi.waitFor(() => expect(wrapper.find('.cm-lintRange-error').text()).toBe('SELEC'), {
      timeout: 2000,
    })
    wrapper.unmount()
  })

  it('shows the bound text', () => {
    const wrapper = mount(SqlEditor, {
      props: {
        flavor: 'mysql',
        schema: {},
        version: null,
        ansiQuotes: false,
        defaultSchema: null,
        modelValue: 'SELECT 1',
      },
      attachTo: document.body,
    })
    expect(wrapper.find('.cm-content').text()).toContain('SELECT 1')
    wrapper.unmount()
  })

  it('replaces the text with a generated query, cursor on the next line', () => {
    const wrapper = mount(SqlEditor, {
      props: {
        flavor: 'mysql',
        schema: {},
        version: null,
        ansiQuotes: false,
        defaultSchema: null,
        modelValue: 'ancien texte',
      },
      attachTo: document.body,
    })
    const query = 'SELECT ALL * FROM `b`.`t`;\n'
    wrapper.vm.replaceText(query)
    const updates = wrapper.emitted('update:modelValue') ?? []
    expect(updates[updates.length - 1]).toEqual([query])
    expect(wrapper.vm.cursor()).toBe(query.length)
    wrapper.unmount()
  })

  it('keeps the indentation of the line on Enter, without that of the SQL language', () => {
    const wrapper = mount(SqlEditor, {
      props: {
        flavor: 'mysql',
        schema: {},
        version: null,
        ansiQuotes: false,
        defaultSchema: null,
        modelValue: '',
      },
      attachTo: document.body,
    })
    // Open parenthesis: the SQL language would indent what follows by two spaces.
    wrapper.vm.replaceText('    SELECT (a')
    wrapper
      .find('.cm-content')
      .element.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', keyCode: 13 }))
    const updates = wrapper.emitted('update:modelValue') ?? []
    expect(updates[updates.length - 1]).toEqual(['    SELECT (a\n    '])
    wrapper.unmount()
  })

  it('scrolls on Ctrl+Down without moving the cursor', () => {
    const wrapper = mount(SqlEditor, {
      props: {
        flavor: 'mysql',
        schema: {},
        version: null,
        ansiQuotes: false,
        defaultSchema: null,
        modelValue: '',
      },
      attachTo: document.body,
    })
    wrapper.vm.replaceText('SELECT 1;\nSELECT 2;\n')
    const cursor = wrapper.vm.cursor()
    const scroller = wrapper.find('.cm-scroller').element
    let scrollTop = 0
    Object.defineProperty(scroller, 'scrollTop', {
      get: () => scrollTop,
      set: (value: number) => (scrollTop = value),
    })
    wrapper.find('.cm-content').element.dispatchEvent(
      new KeyboardEvent('keydown', {
        key: 'ArrowDown',
        keyCode: 40,
        ctrlKey: true,
        bubbles: true,
        cancelable: true,
      }),
    )
    expect(scrollTop).toBeGreaterThan(0)
    expect(wrapper.vm.cursor()).toBe(cursor)
    wrapper.unmount()
  })

  it('replaces the content when the text changes from outside', async () => {
    const wrapper = mount(SqlEditor, {
      props: {
        flavor: 'mariadb',
        schema: {},
        version: null,
        ansiQuotes: false,
        defaultSchema: null,
        modelValue: 'SELECT 1',
      },
      attachTo: document.body,
    })
    await wrapper.setProps({ modelValue: 'SHOW TABLES' })
    expect(wrapper.find('.cm-content').text()).toContain('SHOW TABLES')
    wrapper.unmount()
  })
})
