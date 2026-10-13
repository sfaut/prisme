import { describe, expect, it } from 'vitest'

import { mount } from '@vue/test-utils'
import { NMessageProvider } from 'naive-ui'
import { h } from 'vue'

import ResultGrid from '../ResultGrid.vue'

/**
 * Monte la grille dans le fournisseur de messages de Naive UI, dont elle a besoin.
 */
function mountGrid(columns: string[], rows: (string | null)[][]) {
  const numeric = columns.map(() => false)
  return mount({
    render: () =>
      h(NMessageProvider, () =>
        h(ResultGrid, { columns, numeric, rows, rowCount: rows.length, sql: 'SELECT 1' }),
      ),
  })
}

describe('ResultGrid', () => {
  it('ajuste une colonne au contenu, puis lui rend sa largeur estimée', async () => {
    const wrapper = mount({
      render: () =>
        h(NMessageProvider, () =>
          h(ResultGrid, {
            columns: ['texte'],
            numeric: [false],
            rows: [['x'.repeat(100)]],
            rowCount: 1,
            sql: 'SELECT texte FROM double_clic',
          }),
        ),
    })
    const width = () =>
      (wrapper.find('.grid-header').element as HTMLElement).style.gridTemplateColumns.split(' ')[1]
    expect(width()).toBe('360px')
    await wrapper.find('.grid-resizer').trigger('dblclick')
    expect(width()).toBe('716px')
    await wrapper.find('.grid-resizer').trigger('dblclick')
    expect(width()).toBe('360px')
  })

  it('affiche les en-têtes et le nombre de lignes', () => {
    const wrapper = mountGrid(
      ['id', 'name'],
      [
        ['1', 'a'],
        ['2', null],
      ],
    )
    const headers = wrapper.findAll('.grid-column-name').map((header) => header.text())
    expect(headers).toEqual(['id', 'name'])
    expect(wrapper.find('.grid-count').text()).toBe('2 / 2 lignes')
  })

  it('filtre les lignes', async () => {
    const wrapper = mountGrid(
      ['id', 'name'],
      [
        ['1', 'a'],
        ['2', null],
      ],
    )
    await wrapper.find('.grid-filter input').setValue('null')
    expect(wrapper.find('.grid-count').text()).toBe('1 / 2 lignes')
  })
})
