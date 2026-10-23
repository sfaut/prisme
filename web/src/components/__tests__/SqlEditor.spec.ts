import { describe, expect, it, vi } from 'vitest'

import { EditorView } from '@codemirror/view'
import { mount } from '@vue/test-utils'
import SqlEditor from '../SqlEditor.vue'

vi.mock('@/api/analyze', () => ({
  analyzeSql: async (sql: string) => ({
    statements: [],
    errors: sql.startsWith('SELEC ')
      ? [
          {
            start: 0,
            end: 5,
            message: 'Query not understood by Prisme',
            detail: { code: 'query_not_understood', cause: 'Expected: an SQL statement' },
          },
        ]
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
        indentSize: 4,
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
        indentSize: 4,
        modelValue: 'SELECT 1',
      },
      attachTo: document.body,
    })
    expect(wrapper.find('.cm-content').text()).toContain('SELECT 1')
    wrapper.unmount()
  })

  it('gives the selected passages, without the cursors', () => {
    const wrapper = mount(SqlEditor, {
      props: {
        flavor: 'mysql',
        schema: {},
        version: null,
        ansiQuotes: false,
        defaultSchema: null,
        indentSize: 4,
        modelValue: 'SELECT 1;\nSELECT 2;',
      },
      attachTo: document.body,
    })
    expect(wrapper.vm.selection()).toEqual([])
    const view = EditorView.findFromDOM(wrapper.find('.cm-editor').element as HTMLElement)!
    view.dispatch({ selection: { anchor: 7, head: 12 } })
    expect(wrapper.vm.selection()).toEqual([{ start: 7, end: 12 }])
    wrapper.unmount()
  })

  it('tells when text becomes selected, then no longer is', () => {
    const wrapper = mount(SqlEditor, {
      props: {
        flavor: 'mysql',
        schema: {},
        version: null,
        ansiQuotes: false,
        defaultSchema: null,
        indentSize: 4,
        modelValue: 'SELECT 1;\nSELECT 2;',
      },
      attachTo: document.body,
    })
    const view = EditorView.findFromDOM(wrapper.find('.cm-editor').element as HTMLElement)!
    view.dispatch({ selection: { anchor: 3 } })
    view.dispatch({ selection: { anchor: 7, head: 12 } })
    // Another selection: no new message.
    view.dispatch({ selection: { anchor: 2, head: 4 } })
    view.dispatch({ selection: { anchor: 4 } })
    expect(wrapper.emitted('selection')).toEqual([[true], [false]])
    wrapper.unmount()
  })

  it('appends a generated query to the text, cursor on the next line', () => {
    const wrapper = mount(SqlEditor, {
      props: {
        flavor: 'mysql',
        schema: {},
        version: null,
        ansiQuotes: false,
        defaultSchema: null,
        indentSize: 4,
        modelValue: 'SELECT 1;',
      },
      attachTo: document.body,
    })
    const text = 'SELECT 1;\n\nSELECT ALL * FROM `b`.`t`;\n'
    wrapper.vm.changeText([{ from: 9, insert: text.slice(9) }], text.length)
    const updates = wrapper.emitted('update:modelValue') ?? []
    expect(updates[updates.length - 1]).toEqual([text])
    expect(wrapper.vm.cursor()).toBe(text.length)
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
        indentSize: 4,
        modelValue: '',
      },
      attachTo: document.body,
    })
    // Open parenthesis: the SQL language would indent what follows by two spaces.
    wrapper.vm.changeText({ from: 0, insert: '    SELECT (a' }, 13)
    wrapper
      .find('.cm-content')
      .element.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', keyCode: 13 }))
    const updates = wrapper.emitted('update:modelValue') ?? []
    expect(updates[updates.length - 1]).toEqual(['    SELECT (a\n    '])
    wrapper.unmount()
  })

  it('removes a whole tab stop with Backspace in the indentation and Ctrl+[', () => {
    const wrapper = mount(SqlEditor, {
      props: {
        flavor: 'mysql',
        schema: {},
        version: null,
        ansiQuotes: false,
        defaultSchema: null,
        indentSize: 4,
        modelValue: '',
      },
      attachTo: document.body,
    })
    const content = wrapper.find('.cm-content').element
    const press = (init: KeyboardEventInit) =>
      content.dispatchEvent(
        new KeyboardEvent('keydown', { bubbles: true, cancelable: true, ...init }),
      )
    const last = () => {
      const updates = wrapper.emitted('update:modelValue') ?? []
      return updates[updates.length - 1]
    }
    wrapper.vm.changeText({ from: 0, insert: '        SELECT' }, 8)
    press({ key: 'Backspace', keyCode: 8 })
    expect(last()).toEqual(['    SELECT'])
    press({ key: '[', keyCode: 219, ctrlKey: true })
    expect(last()).toEqual(['SELECT'])
    wrapper.unmount()
  })

  it('follows the indentation size when it changes', async () => {
    const wrapper = mount(SqlEditor, {
      props: {
        flavor: 'mysql',
        schema: {},
        version: null,
        ansiQuotes: false,
        defaultSchema: null,
        indentSize: 4,
        modelValue: '',
      },
      attachTo: document.body,
    })
    await wrapper.setProps({ indentSize: 2 })
    wrapper.vm.changeText({ from: 0, insert: 'SELECT' }, 0)
    wrapper
      .find('.cm-content')
      .element.dispatchEvent(
        new KeyboardEvent('keydown', { key: 'Tab', keyCode: 9, bubbles: true, cancelable: true }),
      )
    const updates = wrapper.emitted('update:modelValue') ?? []
    expect(updates[updates.length - 1]).toEqual(['  SELECT'])
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
        indentSize: 4,
        modelValue: '',
      },
      attachTo: document.body,
    })
    wrapper.vm.changeText({ from: 0, insert: 'SELECT 1;\nSELECT 2;\n' }, 20)
    const cursor = wrapper.vm.cursor()
    const scroller = wrapper.find('.cm-scroller').element
    let scrollTop = 0
    Object.defineProperty(scroller, 'scrollTop', {
      get: () => scrollTop,
      set: (value: number) => {
        scrollTop = value
      },
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

  it('runs the queries of the selection on Ctrl+Shift+Enter, all of them without one', () => {
    const wrapper = mount(SqlEditor, {
      props: {
        flavor: 'mysql',
        schema: {},
        version: null,
        ansiQuotes: false,
        defaultSchema: null,
        indentSize: 4,
        modelValue: 'SELECT 1;\nSELECT 2;\nSELECT 3;',
      },
      attachTo: document.body,
    })
    const view = EditorView.findFromDOM(wrapper.find('.cm-editor').element as HTMLElement)!
    const press = () =>
      wrapper.find('.cm-content').element.dispatchEvent(
        new KeyboardEvent('keydown', {
          key: 'Enter',
          keyCode: 13,
          ctrlKey: true,
          shiftKey: true,
        }),
      )
    view.dispatch({ selection: { anchor: 3 } })
    press()
    // Selected backwards: the passage still goes from its start to its end.
    view.dispatch({ selection: { anchor: 15, head: 5 } })
    press()
    expect(wrapper.emitted('run-all')).toEqual([[[]], [[{ start: 5, end: 15 }]]])
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
        indentSize: 4,
        modelValue: 'SELECT 1',
      },
      attachTo: document.body,
    })
    await wrapper.setProps({ modelValue: 'SHOW TABLES' })
    expect(wrapper.find('.cm-content').text()).toContain('SHOW TABLES')
    wrapper.unmount()
  })
})
