import { describe, expect, it, vi } from 'vitest'

import type { PivotShare } from '@/api/generated/PivotShare'

import type { PivotConcat } from '../pivotConfig'
import { concatGroup, orderText, shareGroup, windowText } from '../pivotMenus'
import PivotConcatSeparatorSubmenu from '../PivotConcatSeparatorSubmenu.vue'

/**
 * Columns of the result.
 */
const NAMES = ['region', 'ville', 'montant']

describe('concatGroup', () => {
  it('shows each setting of a GROUP_CONCAT with its value, DISTINCT as a checkbox', () => {
    const concat: PivotConcat = {
      distinct: true,
      order: [{ column: 2, descending: true }, { column: 0 }],
      separator: '\r\n',
      limit: 5,
      offset: 1,
    }
    const group = concatGroup(concat, 1, NAMES, () => undefined)
    expect(group.label).toBe('Concaténation')
    expect(group.items.map(({ label, value, ticked }) => [label, value ?? ticked])).toEqual([
      ['Valeurs distinctes (DISTINCT)', true],
      ['Ordre (ORDER BY)', 'montant ↓, region'],
      ['Séparateur (SEPARATOR)', "'\\r\\n'"],
      ['Limite (LIMIT / OFFSET)', '5 / 1'],
    ])
    expect(group.items.slice(1).every((item) => item.panel !== undefined)).toBe(true)
  })

  it('shows the default settings: the order of the rows, every value', () => {
    expect(orderText({ separator: ', ' }, NAMES)).toBe('Ordre du résultat')
    expect(windowText({ separator: ', ' })).toBe('Toutes')
    expect(windowText({ separator: ', ', limit: 3 })).toBe('3')
    expect(windowText({ separator: ', ', limit: 3, offset: 0 })).toBe('3')
  })

  it('changes DISTINCT, and the settings its submenus send', () => {
    const change = vi.fn<(changes: Partial<PivotConcat>) => void>()
    const group = concatGroup({ separator: ', ' }, 1, NAMES, change)
    group.items[0]!.onSelect!()
    expect(change).toHaveBeenLastCalledWith({ distinct: true })
    const separator = group.items[2]!.panel!
    expect(separator.component).toBe(PivotConcatSeparatorSubmenu)
    expect(separator.props.separator).toBe(', ')
    ;(separator.props.onChange as (separator: string) => void)(';')
    expect(change).toHaveBeenLastCalledWith({ separator: ';' })
    const window = group.items[3]!.panel!.props.onChange as (window: object) => void
    window({ limit: 2, offset: undefined })
    expect(change).toHaveBeenLastCalledWith({ limit: 2, offset: undefined })
  })
})

describe('shareGroup', () => {
  it('keeps the menu open when a share is chosen, then shows its digits', () => {
    const change = vi.fn<(share: PivotShare | undefined) => void>()
    const values = shareGroup(undefined, change)
    expect(values.items.map(({ label, checked, keepOpen }) => [label, checked, keepOpen])).toEqual([
      ['Valeur', true, false],
      ['Part du total de la ligne', false, true],
      ['Part du total de la colonne', false, true],
    ])
    values.items[1]!.onSelect!()
    expect(change).toHaveBeenLastCalledWith({ of: 'row', decimals: 0 })

    const share = shareGroup({ of: 'row', decimals: 2 }, change)
    expect(share.items.map(({ label, value }) => [label, value])).toEqual([
      ['Valeur', undefined],
      ['Part du total de la ligne', undefined],
      ['Part du total de la colonne', undefined],
      ['Décimales', '2'],
    ])
    expect(share.items.every((item) => item.keepOpen !== true)).toBe(true)
    // The other share keeps the digits.
    share.items[2]!.onSelect!()
    expect(change).toHaveBeenLastCalledWith({ of: 'column', decimals: 2 })
    const digits = share.items[3]!.groups![0]!.items
    expect(digits.map(({ label, checked }) => [label, checked])).toEqual([
      ['0', false],
      ['1', false],
      ['2', true],
      ['3', false],
      ['4', false],
      ['5', false],
      ['6', false],
    ])
    digits[4]!.onSelect!()
    expect(change).toHaveBeenLastCalledWith({ of: 'row', decimals: 4 })
    share.items[0]!.onSelect!()
    expect(change).toHaveBeenLastCalledWith(undefined)
  })
})
