import { EditorView } from '@codemirror/view'
import { createPinia, setActivePinia } from 'pinia'
import { beforeEach, describe, expect, it } from 'vitest'

import { mount } from '@vue/test-utils'

import { useDataExplorerStore } from '@/stores/dataExplorer'

import DataExplorer from '../DataExplorer.vue'

beforeEach(() => setActivePinia(createPinia()))

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

  it('closes with its button', async () => {
    const { explorer, wrapper } = mountExplorer()
    await wrapper.find('[aria-label="Fermer l’explorateur de données"]').trigger('click')
    expect(explorer.open).toBe(false)
    wrapper.unmount()
  })
})
