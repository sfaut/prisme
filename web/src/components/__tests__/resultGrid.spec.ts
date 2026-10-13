import { describe, expect, it } from 'vitest'

import type { Cell } from '@/api/querySession'

import {
  dropPosition,
  moveColumn,
  MAX_ESTIMATED_WIDTH,
  MAX_FITTED_WIDTH,
  estimatedWidth,
  fittedWidth,
  MIN_COLUMN_WIDTH,
  resizedWidth,
  compareCells,
  emptySelection,
  isSelected,
  nextSort,
  selectAll,
  selectCell,
  selectColumn,
  selectRow,
  selectionToText,
  visibleRows,
} from '../resultGrid'

const columns = ['id', 'name', 'city']
const rows: Cell[][] = [
  ['10', 'Zoé', 'Lyon'],
  ['2', 'adam', null],
  ['1', 'Éric', 'Paris'],
]
const noFilter = { text: '', column: null }

describe('tri', () => {
  it('compare les nombres par valeur et met NULL en premier', () => {
    expect(compareCells('2', '10')).toBeLessThan(0)
    expect(compareCells(null, 'a')).toBeLessThan(0)
    expect(compareCells('a2', 'a10')).toBeLessThan(0)
  })

  it('trie la vue sans toucher aux lignes', () => {
    expect(visibleRows(rows, noFilter, { column: 0, direction: 'asc' })).toEqual([2, 1, 0])
    expect(visibleRows(rows, noFilter, { column: 2, direction: 'asc' })).toEqual([1, 0, 2])
    expect(visibleRows(rows, noFilter, { column: 0, direction: 'desc' })).toEqual([0, 1, 2])
    expect(rows[0]?.[0]).toBe('10')
  })

  it('passe de croissant à décroissant puis à aucun tri', () => {
    const asc = nextSort(null, 1)
    expect(asc).toEqual({ column: 1, direction: 'asc' })
    const desc = nextSort(asc, 1)
    expect(desc).toEqual({ column: 1, direction: 'desc' })
    expect(nextSort(desc, 1)).toBeNull()
    expect(nextSort(desc, 2)).toEqual({ column: 2, direction: 'asc' })
  })
})

describe('filtre', () => {
  it('cherche dans toutes les colonnes, sans distinction de casse', () => {
    expect(visibleRows(rows, { text: 'PAR', column: null }, null)).toEqual([2])
  })

  it('cherche dans une seule colonne', () => {
    expect(visibleRows(rows, { text: '1', column: 0 }, null)).toEqual([0, 2])
    expect(visibleRows(rows, { text: '1', column: 1 }, null)).toEqual([])
  })

  it('trouve NULL par son texte affiché', () => {
    expect(visibleRows(rows, { text: 'null', column: 2 }, null)).toEqual([1])
  })
})

describe('sélection', () => {
  it('étend une plage depuis le point de départ (Maj-clic)', () => {
    let selection = selectCell(emptySelection(), 0, 0, 'replace')
    selection = selectCell(selection, 1, 1, 'extend')
    expect(selection.ranges).toEqual([{ top: 0, bottom: 1, left: 0, right: 1 }])
    expect(isSelected(selection, 1, 0)).toBe(true)
    expect(isSelected(selection, 2, 0)).toBe(false)
  })

  it('ajoute des plages (Ctrl-clic)', () => {
    let selection = selectCell(emptySelection(), 0, 0, 'replace')
    selection = selectCell(selection, 2, 2, 'add')
    expect(isSelected(selection, 0, 0)).toBe(true)
    expect(isSelected(selection, 2, 2)).toBe(true)
    expect(isSelected(selection, 1, 1)).toBe(false)
  })

  it('sélectionne des lignes et des colonnes entières', () => {
    let selection = selectRow(emptySelection(), 0, 3, 'replace')
    selection = selectRow(selection, 1, 3, 'extend')
    expect(selection.ranges).toEqual([{ top: 0, bottom: 1, left: 0, right: 2 }])

    selection = selectColumn(emptySelection(), 1, 3, 'replace')
    expect(selection.ranges).toEqual([{ top: 0, bottom: 2, left: 1, right: 1 }])
  })

  it('sélectionne tout, ou rien pour une grille vide', () => {
    expect(selectAll(3, 3).ranges).toEqual([{ top: 0, bottom: 2, left: 0, right: 2 }])
    expect(selectAll(0, 3)).toEqual(emptySelection())
  })
})

describe('copie', () => {
  const view = [0, 1, 2]

  it('copie une plage en texte tabulé, NULL en cellule vide', () => {
    const selection = selectCell(selectCell(emptySelection(), 0, 1, 'replace'), 1, 2, 'extend')
    expect(selectionToText(selection, view, rows, columns, false)).toBe('Zoé\tLyon\nadam\t')
  })

  it('ajoute les en-têtes sur demande', () => {
    const selection = selectColumn(emptySelection(), 0, 3, 'replace')
    expect(selectionToText(selection, view, rows, columns, true)).toBe('id\n10\n2\n1')
  })

  it('suit la vue triée', () => {
    const sorted = visibleRows(rows, noFilter, { column: 0, direction: 'asc' })
    const selection = selectRow(emptySelection(), 0, 3, 'replace')
    expect(selectionToText(selection, sorted, rows, columns, false)).toBe('1\tÉric\tParis')
  })

  it('laisse vides les cellules non sélectionnées du cadre', () => {
    let selection = selectCell(emptySelection(), 0, 0, 'replace')
    selection = selectCell(selection, 1, 2, 'add')
    expect(selectionToText(selection, view, rows, columns, false)).toBe('10\t\n\t')
  })

  it('protège les tabulations, retours à la ligne et guillemets', () => {
    const special: Cell[][] = [['a\tb', 'dit "oui"', 'x\ny']]
    const selection = selectAll(1, 3)
    expect(selectionToText(selection, [0], special, columns, false)).toBe(
      '"a\tb"\t"dit ""oui"""\t"x\ny"',
    )
  })
})

describe('largeur des colonnes', () => {
  it('suit le pointeur sans descendre sous le minimum', () => {
    expect(resizedWidth(120, 35.6)).toBe(156)
    expect(resizedWidth(120, -30)).toBe(90)
    expect(resizedWidth(120, -500)).toBe(MIN_COLUMN_WIDTH)
  })
})

describe('largeur estimée ou ajustée', () => {
  const rows: Cell[][] = [['a'], ['x'.repeat(80)], ...Array.from({ length: 300 }, () => ['b'])]

  it('estime sur les premières lignes, avec un plafond', () => {
    expect(estimatedWidth('id', [['1']], 0)).toBe(48)
    expect(estimatedWidth('texte', rows, 0)).toBe(MAX_ESTIMATED_WIDTH)
    // Valeur longue au-delà des 200 premières lignes : ignorée par l'estimation.
    expect(estimatedWidth('texte', [...rows.slice(2), ['x'.repeat(80)]], 0)).toBe(65)
  })

  it('ajuste sur toutes les lignes, avec un plafond plus haut', () => {
    expect(fittedWidth('texte', [...rows.slice(2), ['x'.repeat(80)]], 0)).toBe(80 * 7 + 16)
    expect(fittedWidth('texte', [['x'.repeat(500)]], 0)).toBe(MAX_FITTED_WIDTH)
  })
})

describe('déplacement des colonnes', () => {
  it('insère la colonne à la position lâchée', () => {
    // a b c d : b lâchée tout à droite, puis tout à gauche.
    expect(moveColumn([0, 1, 2, 3], 1, 4)).toEqual({ order: [0, 2, 3, 1], position: 3 })
    expect(moveColumn([0, 1, 2, 3], 1, 0)).toEqual({ order: [1, 0, 2, 3], position: 0 })
    // Lâchée juste avant ou juste après elle-même : rien ne bouge.
    expect(moveColumn([0, 1, 2, 3], 1, 1).order).toEqual([0, 1, 2, 3])
    expect(moveColumn([0, 1, 2, 3], 1, 2).order).toEqual([0, 1, 2, 3])
  })

  it('choisit la position d’après le milieu des en-têtes', () => {
    const centers = [50, 150, 250]
    expect(dropPosition(centers, 10)).toBe(0)
    expect(dropPosition(centers, 160)).toBe(2)
    expect(dropPosition(centers, 400)).toBe(3)
  })

  it('copie les colonnes dans l’ordre affiché', () => {
    const selection = selectRow(emptySelection(), 0, 3, 'replace')
    expect(selectionToText(selection, [0], rows, columns, true, [2, 0, 1])).toBe(
      'city\tid\tname\nLyon\t10\tZoé',
    )
  })
})
