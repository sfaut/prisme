import { describe, expect, it } from 'vitest'

import { mount } from '@vue/test-utils'

import ResultGrid from '../ResultGrid.vue'

/**
 * Monte la grille.
 *
 * @param columns Noms des colonnes.
 * @param rows Lignes du résultat.
 * @param sql Texte de la requête.
 */
function mountGrid(columns: string[], rows: (string | null)[][], sql = 'SELECT 1') {
  const rightAligned = columns.map(() => false)
  return mount(ResultGrid, {
    props: { columns, rightAligned, rows, rowCount: rows.length, sql },
  })
}

describe('ResultGrid', () => {
  it('sélectionne tout par un clic sur le coin supérieur gauche', async () => {
    const wrapper = mountGrid(['id', 'name'], [['1', 'a']])
    await wrapper.find('.grid-corner').trigger('mousedown')
    const headers = wrapper.findAll('.grid-column-header')
    expect(headers.map((header) => header.classes('selected'))).toEqual([true, true])
  })

  it('ajuste une colonne au contenu, puis lui rend sa largeur estimée', async () => {
    const wrapper = mountGrid(['texte'], [['x'.repeat(100)]], 'SELECT texte FROM double_clic')
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
    expect(wrapper.find('.grid-count').text()).toBe('2 lignes')
  })

  it('filtre les lignes', async () => {
    const wrapper = mountGrid(
      ['id', 'name'],
      [
        ['1', 'a'],
        ['2', null],
      ],
    )
    // Replié au départ : le bouton de la barre du bas déplie le formulaire.
    expect(wrapper.find('.grid-filter').exists()).toBe(false)
    await wrapper.find('.grid-filter-toggle').trigger('click')
    await wrapper.find('.grid-filter input').setValue('null')
    expect(wrapper.find('.grid-count').text()).toBe('1 / 2 lignes')
  })
})
