import { EditorView } from '@codemirror/view'
import { createPinia, setActivePinia } from 'pinia'
import { beforeEach, describe, expect, it } from 'vitest'

import { mount } from '@vue/test-utils'

import { useDataExplorerStore } from '@/stores/dataExplorer'

import DataExplorer from '../DataExplorer.vue'

beforeEach(() => {
  localStorage.clear()
  setActivePinia(createPinia())
})

/**
 * Mounts the explorer, open.
 */
function mountExplorer() {
  const explorer = useDataExplorerStore()
  explorer.open = true
  const wrapper = mount(DataExplorer, { attachTo: document.body })
  return { explorer, wrapper }
}

/**
 * Text of the explorer's editor.
 *
 * @param wrapper Mounted explorer.
 */
function editorText(wrapper: ReturnType<typeof mountExplorer>['wrapper']): string {
  return EditorView.findFromDOM(
    wrapper.find('.cm-editor').element as HTMLElement,
  )!.state.doc.toString()
}

/**
 * Position of the cursor in the explorer's editor.
 *
 * @param wrapper Mounted explorer.
 */
function cursor(wrapper: ReturnType<typeof mountExplorer>['wrapper']): number {
  return EditorView.findFromDOM(wrapper.find('.cm-editor').element as HTMLElement)!.state.selection
    .main.head
}

describe('DataExplorer', () => {
  it('asks for a selection, then for a single column', async () => {
    const { explorer, wrapper } = mountExplorer()
    expect(wrapper.find('.data-explorer-message').text()).toContain('Sélectionnez des valeurs')
    explorer.show(Symbol('grid'), { state: 'several' })
    await wrapper.vm.$nextTick()
    expect(wrapper.find('.data-explorer-message').text()).toContain('plusieurs colonnes')
    explorer.show(Symbol('grid'), { state: 'scalar', column: 'name' })
    await wrapper.vm.$nextTick()
    expect(wrapper.find('.data-explorer-message').text()).toBe(
      'La colonne « name » ne contient ni JSON ni XML.',
    )
    wrapper.unmount()
  })

  it('shows the selected values, indented, then the next ones', async () => {
    const { explorer, wrapper } = mountExplorer()
    explorer.show(Symbol('grid'), {
      state: 'values',
      column: 'doc',
      kind: 'json',
      cells: [
        { row: 1, value: '{"a":1}' },
        { row: 2, value: null },
      ],
    })
    await wrapper.vm.$nextTick()
    expect(wrapper.find('.data-explorer-message').exists()).toBe(false)
    expect(editorText(wrapper)).toBe('[\n    {\n        "a": 1\n    },\n    null\n]')
    expect(wrapper.text()).toContain('doc · JSON · 2 valeurs')
    explorer.show(Symbol('grid'), {
      state: 'values',
      column: 'doc',
      kind: 'xml',
      cells: [{ row: 3, value: '<a><b/></a>' }],
    })
    await wrapper.vm.$nextTick()
    expect(editorText(wrapper)).toBe('<a>\n    <b/>\n</a>')
    wrapper.unmount()
  })

  it('wraps long lines on demand, off by default, and remembers the choice', async () => {
    const { explorer, wrapper } = mountExplorer()
    explorer.show(Symbol('grid'), {
      state: 'values',
      column: 'doc',
      kind: 'json',
      cells: [{ row: 1, value: '{"a":1}' }],
    })
    await wrapper.vm.$nextTick()
    const editor = wrapper.find('.cm-content')
    const button = wrapper.find('[aria-label="Retour à la ligne"]')
    expect(editor.classes()).not.toContain('cm-lineWrapping')
    expect(button.attributes('aria-pressed')).toBe('false')
    await button.trigger('click')
    expect(editor.classes()).toContain('cm-lineWrapping')
    expect(button.attributes('aria-pressed')).toBe('true')
    expect(localStorage.getItem('prisme.dataExplorerWrap')).toBe('true')
    wrapper.unmount()
    setActivePinia(createPinia())
    expect(useDataExplorerStore().wrap).toBe(true)
  })

  it('searches as the expression is typed, highlights then filters the results', async () => {
    const { explorer, wrapper } = mountExplorer()
    explorer.show(Symbol('grid'), {
      state: 'values',
      column: 'doc',
      kind: 'json',
      cells: [{ row: 1, value: '{"a":[1,2,3]}' }],
    })
    await wrapper.vm.$nextTick()
    await wrapper.find('[aria-label="Rechercher"]').trigger('click')
    const field = wrapper.find('[aria-label="Expression de recherche"]')
    await field.setValue('$.a[?@ > 1]')
    const highlighted = () => wrapper.findAll('.cm-dataMatch, .cm-dataMatch-current')
    expect(highlighted().map((mark) => mark.text())).toEqual(['2', '3'])
    expect(wrapper.find('.cm-dataMatch-current').text()).toBe('2')
    expect(wrapper.find('.data-explorer-search-status').text()).toBe('1 / 2')
    await field.trigger('keydown', { key: 'Enter' })
    expect(wrapper.find('.cm-dataMatch-current').text()).toBe('3')
    expect(wrapper.find('.data-explorer-search-status').text()).toBe('2 / 2')

    await field.setValue('$.a[?')
    expect(wrapper.find('.data-explorer-search-error').text()).toMatch(/^JSONPath invalide/)
    expect(highlighted()).toHaveLength(2)

    await field.setValue('$.a[?@ > 1]')
    await wrapper.find('[aria-label="Afficher seulement les résultats"]').trigger('click')
    expect(editorText(wrapper)).toBe('[\n    2,\n    3\n]')
    expect(wrapper.find('.data-explorer-search-status').text()).toBe('1 / 2')
    expect(highlighted().map((mark) => mark.text())).toEqual(['2'])
    await field.trigger('keydown', { key: 'Enter' })
    expect(wrapper.find('.data-explorer-search-status').text()).toBe('2 / 2')
    expect(highlighted().map((mark) => mark.text())).toEqual(['3'])
    expect(cursor(wrapper)).toBe(editorText(wrapper).indexOf('3'))

    await wrapper.find('[aria-label="Rechercher"]').trigger('click')
    expect(editorText(wrapper)).toBe('{\n    "a": [\n        1,\n        2,\n        3\n    ]\n}')
    wrapper.unmount()
  })

  it('closes with its button', async () => {
    const { explorer, wrapper } = mountExplorer()
    await wrapper.find('[aria-label="Fermer l’explorateur de données"]').trigger('click')
    expect(explorer.open).toBe(false)
    wrapper.unmount()
  })
})
