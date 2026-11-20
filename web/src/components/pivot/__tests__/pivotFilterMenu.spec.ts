import { describe, expect, it, vi } from 'vitest'

import type { ColumnTypeInfo } from '@/api/generated/ColumnTypeInfo'
import type { Cell } from '@/api/querySession'
import type { MenuGroup, MenuItem } from '@/components/ui/menuItem'

import type { PivotColumnFilter, PivotFilter, PivotLevel, PivotMeasureFilter } from '../pivotConfig'
import PivotFilterListSubmenu from '../PivotFilterListSubmenu.vue'
import { filterGroups, operandLine, operandText } from '../pivotFilterMenu'
import PivotFilterOperandSubmenu from '../PivotFilterOperandSubmenu.vue'

/**
 * Columns of the result.
 */
const NAMES = ['region', 'vendue_le', 'montant']

/**
 * Types of the columns of the result.
 */
const TYPES: ColumnTypeInfo[] = [
  { family: 'text', name: 'TEXT' },
  { family: 'datetime', name: 'DATETIME' },
  { family: 'decimal', name: 'DECIMAL' },
]

/**
 * Precisions of a column of dates.
 */
const PRECISIONS = [
  'Année',
  'Trimestre',
  'Mois',
  'Semaine',
  'Date',
  'Horodatage',
  'Horodatage (hh)',
  'Horodatage (hh:mm)',
  'Horodatage (hh:mm:ss)',
  'Horodatage (hh:mm:ss.ms)',
  'Horodatage (hh:mm:ss.µs)',
]

/**
 * Filter `montant BETWEEN ? AND ?`.
 */
const ON_COLUMN: PivotColumnFilter = {
  kind: 'column',
  column: 2,
  name: '',
  precision: 'none',
  condition: 'between',
  values: [],
  operand: '',
  upper: '',
}

/**
 * Filter `SUM(montant) > 100`.
 */
const ON_MEASURE: PivotMeasureFilter = {
  kind: 'measure',
  aggregate: 'sum',
  column: 2,
  name: '',
  condition: 'greater',
  operand: '100',
  upper: '',
  level_precision: 'none',
}

/**
 * Builds the menu of a filter, the filter following its changes, as the zones do.
 *
 * @param filter Filter.
 * @param rows Levels of the rows.
 * @param columns Levels of the columns.
 */
function follow(filter: PivotFilter, rows: PivotLevel[] = [], columns: PivotLevel[] = []) {
  const readValues = vi.fn<(column: number, precision: string) => Promise<Cell[]>>(() =>
    Promise.resolve([]),
  )
  const state = { filter, changes: [] as PivotFilter[] }
  const groups = (): MenuGroup[] =>
    filterGroups(
      state.filter,
      {
        config: { rows, columns, measures: [], filters: [state.filter] },
        names: NAMES,
        types: TYPES,
        readValues,
        receiving: false,
      },
      (changes) => {
        state.filter = { ...state.filter, ...changes } as PivotFilter
        state.changes.push(state.filter)
      },
    )
  /**
   * Group of the menu, by its label.
   */
  const group = (label: string): MenuGroup | undefined =>
    groups().find((item) => item.label === label)
  /**
   * Choice of the menu, by its label, possibly in the submenu of a line.
   */
  const item = (label: string, line?: string): MenuItem => {
    const items = groups().flatMap((each) => each.items)
    const list =
      line === undefined
        ? items
        : items.find((each) => each.label === line)!.groups!.flatMap((each) => each.items)
    return list.find((each) => each.label === label)!
  }
  return { state, groups, group, item, readValues }
}

describe('filterGroups', () => {
  it('offers each condition to a filter on a column, with the line of what it compares to', () => {
    const { groups, group, item, state } = follow(ON_COLUMN)
    expect(groups().map(({ label }) => label)).toEqual(['Condition', 'Bornes'])
    expect(group('Condition')!.items.map(({ label, checked }) => [label, checked])).toEqual([
      ['Dans la liste', false],
      ['Hors de la liste', false],
      ['Égal à', false],
      ['Différent de', false],
      ['Inférieur à', false],
      ['Inférieur ou égal à', false],
      ['Supérieur à', false],
      ['Supérieur ou égal à', false],
      ['Entre', true],
      ['N’est pas entre', false],
      ['Est NULL', false],
      ['N’est pas NULL', false],
    ])
    expect(item('Bornes').value).toBe('À saisir')
    expect(item('Bornes').panel!.component).toBe(PivotFilterOperandSubmenu)
    // A condition that compares to something else keeps the menu open, which shows its line.
    expect(item('Inférieur à').keepOpen).toBe(true)
    expect(item('N’est pas entre').keepOpen).toBe(false)
    expect(item('Est NULL').keepOpen).toBe(false)
    item('Inférieur à').onSelect!()
    expect(state.filter.condition).toBe('less')
    expect(group('Valeur')!.items.map(({ label }) => label)).toEqual(['Valeur'])
    item('Est NULL').onSelect!()
    expect(groups().map(({ label }) => label)).toEqual(['Condition'])
    item('Dans la liste').onSelect!()
    expect(item('Valeurs cochées').panel!.component).toBe(PivotFilterListSubmenu)
  })

  it('shows what the filter compares to on the line of its submenu', () => {
    expect(operandText({ ...ON_COLUMN, operand: '10', upper: '100' })).toBe('10 – 100')
    expect(operandText({ ...ON_COLUMN, operand: '10' })).toBe('10 – …')
    expect(operandText(ON_MEASURE)).toBe('100')
    expect(operandText({ ...ON_MEASURE, operand: '' })).toBe('À saisir')
    expect(operandText({ ...ON_COLUMN, condition: 'in_list' })).toBe('Aucune')
    expect(operandText({ ...ON_COLUMN, condition: 'in_list', values: ['Nord', null] })).toBe('2')
    expect(operandLine({ ...ON_COLUMN, condition: 'not_in_list' })).toBe('Valeurs cochées')
    expect(operandLine({ ...ON_COLUMN, condition: 'is_not_null' })).toBeUndefined()
  })

  it('keeps the values under another condition', () => {
    const filter = { ...ON_COLUMN, operand: '1', upper: '2', values: ['Nord'] }
    const { item, state } = follow(filter)
    item('Inférieur à').onSelect!()
    item('Entre').onSelect!()
    expect(state.filter).toEqual(filter)
  })

  it('lists the values at its precision, NULL only in the list kept', async () => {
    const filter: PivotColumnFilter = {
      ...ON_COLUMN,
      column: 1,
      precision: 'month',
      condition: 'in_list',
    }
    const { item, readValues, state } = follow(filter)
    const props = () => item('Valeurs cochées').panel!.props
    expect(props()).toMatchObject({
      label: 'vendue_le',
      offersNull: true,
      partial: false,
      kept: [],
    })
    await (props().read as () => Promise<Cell[]>)()
    expect(readValues).toHaveBeenCalledExactlyOnceWith(1, 'month')
    ;(props().onChange as (values: Cell[]) => void)(['Nord'])
    expect(state.filter).toEqual({ ...filter, values: ['Nord'] })
    item('Hors de la liste').onSelect!()
    expect(props()).toMatchObject({ offersNull: false, kept: ['Nord'] })
  })

  it('offers a precision to the dates, which empties what the filter compares to', () => {
    const filter = { ...ON_COLUMN, column: 1, operand: '2026-01-15 10:00:00', values: ['x'] }
    const { group, item, state } = follow(filter)
    expect(group('Précision')!.items.map(({ label }) => label)).toEqual(PRECISIONS)
    // The whole instant, without precision.
    expect(
      group('Précision')!
        .items.filter(({ checked }) => checked)
        .map(({ label }) => label),
    ).toEqual(['Horodatage'])
    item('Horodatage').onSelect!()
    expect(state.changes).toEqual([])
    item('Mois').onSelect!()
    expect(state.changes).toEqual([{ ...filter, precision: 'month', operand: '', values: [] }])
    expect(item('Mois').checked).toBe(true)
    // The precision taken changes nothing.
    item('Mois').onSelect!()
    expect(state.changes).toHaveLength(1)
  })

  it('offers no precision to a column without dates, nor to a measure that reads none', () => {
    expect(follow(ON_COLUMN).group('Précision')).toBeUndefined()
    expect(follow(ON_MEASURE).group('Précision')).toBeUndefined()
    expect(
      follow({ ...ON_MEASURE, column: 1, aggregate: 'count' }).group('Précision'),
    ).toBeUndefined()
  })

  it('offers a filter on a measure its aggregate and its level, then the tests, without a list', () => {
    const { groups, group, item } = follow(ON_MEASURE)
    expect(groups().map(({ label }) => label)).toEqual(['Filtre', 'Condition', 'Valeur'])
    expect(group('Filtre')!.items.map(({ label, value }) => [label, value])).toEqual([
      ['Agrégat', 'SUM(montant)'],
      ['Niveau', 'Le niveau le plus fin'],
    ])
    expect(group('Condition')!.items.map(({ label }) => label)).toEqual([
      'Égal à',
      'Différent de',
      'Inférieur à',
      'Inférieur ou égal à',
      'Supérieur à',
      'Supérieur ou égal à',
      'Entre',
      'N’est pas entre',
      'Est NULL',
      'N’est pas NULL',
    ])
    expect(item('Valeur').value).toBe('100')
  })

  it('offers each aggregate, refusing those that add up a column of no numbers', () => {
    const { group, item, state } = follow({ ...ON_MEASURE, column: 0, aggregate: 'count' })
    const aggregates = group('Filtre')!.items[0]!.groups![0]!.items
    // Without GROUP_CONCAT, whose texts no filter compares yet.
    expect(aggregates.map(({ label, checked }) => [label, checked])).toEqual([
      ['COUNT(*)', false],
      ['COUNT(region)', true],
      ['COUNT(DISTINCT region)', false],
      ['SUM(region)', false],
      ['SUM(DISTINCT region)', false],
      ['AVG(region)', false],
      ['AVG(DISTINCT region)', false],
      ['MIN(region)', false],
      ['MAX(region)', false],
    ])
    const refused = aggregates.filter(({ disabled }) => disabled)
    expect(refused.map(({ label }) => label)).toEqual([
      'SUM(region)',
      'SUM(DISTINCT region)',
      'AVG(region)',
      'AVG(DISTINCT region)',
    ])
    expect(refused.every(({ hint }) => hint === 'Demande une colonne de nombres')).toBe(true)
    item('MAX(region)', 'Agrégat').onSelect!()
    expect(state.filter).toMatchObject({ aggregate: 'max' })
  })

  it('offers a precision to MIN and MAX of dates, which keeps the menu open', () => {
    const filter: PivotMeasureFilter = {
      ...ON_MEASURE,
      column: 1,
      aggregate: 'count',
      condition: 'greater_or_equal',
      operand: '2',
    }
    const { group, item, state } = follow(filter)
    expect(item('MAX(vendue_le)', 'Agrégat').keepOpen).toBe(true)
    expect(item('COUNT(*)', 'Agrégat').keepOpen).toBe(false)
    item('MAX(vendue_le)', 'Agrégat').onSelect!()
    expect(group('Précision')!.items.map(({ label }) => label)).toEqual(PRECISIONS)
    expect(item('MIN(vendue_le)', 'Agrégat').keepOpen).toBe(false)
    item('Mois').onSelect!()
    expect(state.filter).toMatchObject({ precision: 'month', operand: '', upper: '' })
    expect(item('Agrégat').value).toBe('MAX(vendue_le (Mois))')
    // COUNT(DISTINCT) keeps the precision, COUNT drops it.
    item('COUNT(DISTINCT vendue_le (Mois))', 'Agrégat').onSelect!()
    expect(state.filter).toMatchObject({ aggregate: 'count_distinct', precision: 'month' })
    item('COUNT(vendue_le)', 'Agrégat').onSelect!()
    expect(state.filter).toMatchObject({ aggregate: 'count', precision: undefined })
    expect(group('Précision')).toBeUndefined()
  })

  it('offers the levels of the two axes, the same level placed twice being one', () => {
    const level = (column: number, precision: PivotLevel['precision'] = 'none'): PivotLevel => ({
      column,
      name: '',
      precision,
    })
    const { item, state } = follow(
      ON_MEASURE,
      [level(0), level(0), level(1, 'month')],
      [level(1, 'year')],
    )
    const levels = () => item('Niveau').groups![0]!.items
    expect(levels().map(({ label, checked }) => [label, checked])).toEqual([
      ['Le niveau le plus fin', true],
      ['region', false],
      ['vendue_le (Mois)', false],
      ['vendue_le (Année)', false],
    ])
    levels()[2]!.onSelect!()
    expect(state.filter).toEqual({ ...ON_MEASURE, level: 1, level_precision: 'month' })
    expect(item('Niveau').value).toBe('vendue_le (Mois)')
    levels()[0]!.onSelect!()
    expect(state.filter).toEqual({ ...ON_MEASURE, level: undefined, level_precision: 'none' })
  })
})
