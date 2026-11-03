import { afterEach, describe, expect, it, vi } from 'vitest'

import { type VueWrapper, flushPromises, mount } from '@vue/test-utils'
import { PopoverRoot } from 'reka-ui'
import { nextTick } from 'vue'

import type { ColumnTypeInfo } from '@/api/generated/ColumnTypeInfo'
import type { Cell } from '@/api/querySession'

import PivotFilterPanel from '../PivotFilterPanel.vue'
import PivotValuesList from '../PivotValuesList.vue'
import type {
  PivotColumnFilter,
  PivotConfig,
  PivotFilter,
  PivotLevel,
  PivotMeasureFilter,
} from '../pivotConfig'
import UiSelect from '../ui/UiSelect.vue'

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

let wrapper: VueWrapper | null = null

afterEach(() => {
  wrapper?.unmount()
  wrapper = null
  document.body.replaceChildren()
})

/**
 * Mounts the settings of a filter, the filter following their changes.
 *
 * @param filter Filter.
 * @param rows Levels of the rows.
 * @param columns Levels of the columns.
 */
async function mountPanel(
  filter: PivotFilter,
  rows: PivotLevel[] = [],
  columns: PivotLevel[] = [],
) {
  const anchor = document.createElement('li')
  anchor.append(document.createElement('button'))
  document.body.append(anchor)
  const changes: PivotFilter[] = []
  const config = (filters: PivotFilter[]): PivotConfig => ({
    rows,
    columns,
    measures: [],
    filters,
  })
  const readValues = vi.fn<() => Promise<Cell[]>>(() => Promise.resolve([null, 'Nord', 'Sud']))
  const closed = vi.fn<() => void>()
  const named = vi.fn<() => void>()
  const mounted = mount(PivotFilterPanel, {
    attachTo: document.body,
    props: {
      anchor,
      filter,
      index: 0,
      config: config([filter]),
      names: NAMES,
      types: TYPES,
      readValues,
      onChange: (changed: PivotFilter) => {
        changes.push(changed)
        mounted.setProps({ filter: changed, config: config([changed]) })
      },
      onClose: closed,
      onName: named,
    },
  })
  wrapper = mounted
  // The content of the popover draws after the mounting of the panel.
  await flushPromises()
  return { mounted, changes, readValues, closed, named, anchor }
}

/**
 * Field of the panel.
 *
 * @param label Name of the field.
 */
function field(label: string): HTMLInputElement {
  return document.body.querySelector<HTMLInputElement>(`input[aria-label="${label}"]`)!
}

/**
 * Types in a field.
 *
 * @param label Name of the field.
 * @param text Text.
 */
async function type(label: string, text: string): Promise<void> {
  const input = field(label)
  input.value = text
  input.dispatchEvent(new Event('input'))
  await nextTick()
}

/**
 * Presses Enter in a field.
 *
 * @param label Name of the field.
 */
async function pressEnter(label: string): Promise<void> {
  field(label).dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter' }))
  await nextTick()
}

/**
 * Text of the refusal the panel shows, if any.
 */
function refusal(): string | undefined {
  return document.body.querySelector('[role="alert"]')?.textContent?.trim()
}

/**
 * Properties of a choice of the panel.
 */
interface SelectProps {
  /**
   * Name of the choice.
   */
  label: string
  /**
   * Choices.
   */
  items: { label: string; value: string; disabled?: boolean; hint?: string }[]
  /**
   * Value taken.
   */
  modelValue: string
}

/**
 * Choice of the panel.
 *
 * @param mounted Mounted panel.
 * @param label Name of the choice.
 */
function select(mounted: VueWrapper, label: string) {
  return mounted
    .findAllComponents(UiSelect)
    .find((item) => (item.props() as unknown as SelectProps).label === label)
}

/**
 * Properties of a choice of the panel, which has to be there.
 *
 * @param mounted Mounted panel.
 * @param label Name of the choice.
 */
function selectProps(mounted: VueWrapper, label: string): SelectProps {
  return select(mounted, label)?.props() as unknown as SelectProps
}

/**
 * Takes a value of a choice of the panel.
 *
 * @param mounted Mounted panel.
 * @param label Name of the choice.
 * @param value Value.
 */
async function choose(mounted: VueWrapper, label: string, value: string): Promise<void> {
  select(mounted, label)?.vm.$emit('update:modelValue', value)
  await nextTick()
}

/**
 * Labels of a choice of the panel.
 *
 * @param mounted Mounted panel.
 * @param label Name of the choice.
 */
function labels(mounted: VueWrapper, label: string): string[] {
  return selectProps(mounted, label).items.map((item) => item.label)
}

describe('PivotFilterPanel', () => {
  it('adds no element to its parent, which would shift the layout of its children', async () => {
    const { mounted } = await mountPanel(ON_COLUMN)
    expect(document.body.querySelector('[role="dialog"]')).not.toBeNull()
    expect(mounted.vm.$el.parentElement.children).toHaveLength(0)
  })

  it('asks a condition for what it needs', async () => {
    const { mounted } = await mountPanel(ON_COLUMN)
    const fields = () => Array.from(document.body.querySelectorAll('input')).map((f) => f.ariaLabel)
    expect(fields()).toEqual(['De', 'À'])
    await choose(mounted, 'Condition', 'less')
    expect(fields()).toEqual(['Valeur'])
    await choose(mounted, 'Condition', 'is_null')
    expect(fields()).toEqual([])
    expect(mounted.findComponent(PivotValuesList).exists()).toBe(false)
    await choose(mounted, 'Condition', 'in_list')
    expect(mounted.findComponent(PivotValuesList).exists()).toBe(true)
  })

  it('offers each condition to a filter on a column', async () => {
    const { mounted } = await mountPanel(ON_COLUMN)
    expect(labels(mounted, 'Condition')).toEqual([
      'Dans la liste',
      'Hors de la liste',
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
  })

  it('refuses a wrong value at Enter, showing why, and applies a right one', async () => {
    const { changes } = await mountPanel(ON_COLUMN)
    await type('De', 'abc')
    expect(refusal()).toBeUndefined()
    await pressEnter('De')
    expect(refusal()).toBe('Un nombre est attendu, comme 100 ou 12.5')
    expect(field('De').getAttribute('aria-invalid')).toBe('true')
    expect(changes).toEqual([])
    await type('De', ' 100 ')
    await pressEnter('De')
    expect(refusal()).toBeUndefined()
    expect(changes).toEqual([{ ...ON_COLUMN, operand: '100' }])
    expect(field('De').value).toBe('100')
  })

  it('applies a value on leaving its field', async () => {
    const { changes } = await mountPanel(ON_COLUMN)
    await type('À', '400')
    field('À').dispatchEvent(new Event('blur'))
    await nextTick()
    expect(changes).toEqual([{ ...ON_COLUMN, upper: '400' }])
  })

  it('applies at its closing all that is being typed, except what is refused', async () => {
    const { mounted, changes, closed } = await mountPanel(ON_COLUMN)
    await type('De', '100')
    await type('À', '400')
    mounted.findComponent(PopoverRoot).vm.$emit('update:open', false)
    await nextTick()
    expect(changes).toEqual([{ ...ON_COLUMN, operand: '100', upper: '400' }])
    expect(closed).toHaveBeenCalledOnce()
  })

  it('forgets what is being typed at Escape, and gives the focus to the button', async () => {
    const { changes, closed, anchor } = await mountPanel(ON_COLUMN)
    await flushPromises()
    await type('De', '100')
    field('De').dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
    await nextTick()
    expect(closed).toHaveBeenCalledOnce()
    expect(document.activeElement).toBe(anchor.querySelector('button'))
    expect(changes).toEqual([])
  })

  it('keeps the values under another condition', async () => {
    const filter = { ...ON_COLUMN, operand: '1', upper: '2', values: ['Nord'] }
    const { mounted, changes } = await mountPanel(filter)
    await choose(mounted, 'Condition', 'less')
    await choose(mounted, 'Condition', 'between')
    expect(changes[changes.length - 1]).toEqual(filter)
    expect(field('De').value).toBe('1')
    expect(field('À').value).toBe('2')
  })

  it('offers a precision to the dates, which empties what the filter compares to', async () => {
    const filter = { ...ON_COLUMN, column: 1, operand: '2026-01-15 10:00:00', values: ['x'] }
    const { mounted, changes } = await mountPanel(filter)
    expect(labels(mounted, 'Précision')).toEqual([
      'Année',
      'Trimestre',
      'Mois',
      'Semaine',
      'Date',
      'Date et heure',
    ])
    await choose(mounted, 'Précision', 'none')
    expect(changes).toEqual([])
    await type('À', 'abc')
    await pressEnter('À')
    await choose(mounted, 'Précision', 'month')
    expect(changes).toEqual([{ ...filter, precision: 'month', operand: '', values: [] }])
    expect(field('De').value).toBe('')
    expect(field('De').placeholder).toBe('2026-01')
    expect(field('À').value).toBe('')
    expect(refusal()).toBeUndefined()
  })

  it('offers no precision to a column without dates, nor to a measure that reads none', async () => {
    expect(select((await mountPanel(ON_COLUMN)).mounted, 'Précision')).toBeUndefined()
    wrapper?.unmount()
    expect(select((await mountPanel(ON_MEASURE)).mounted, 'Précision')).toBeUndefined()
    wrapper?.unmount()
    const summed = { ...ON_MEASURE, column: 1, aggregate: 'count' } as const
    expect(select((await mountPanel(summed)).mounted, 'Précision')).toBeUndefined()
  })

  it('offers a precision to MIN and MAX of dates, which then compare periods', async () => {
    const filter: PivotMeasureFilter = {
      ...ON_MEASURE,
      column: 1,
      aggregate: 'max',
      condition: 'greater_or_equal',
      operand: '2026-01-15 10:00:00',
    }
    const { mounted, changes } = await mountPanel(filter)
    expect(labels(mounted, 'Précision')).toEqual([
      'Année',
      'Trimestre',
      'Mois',
      'Semaine',
      'Date',
      'Date et heure',
    ])
    expect(selectProps(mounted, 'Précision').modelValue).toBe('none')
    await choose(mounted, 'Précision', 'month')
    expect(changes).toEqual([{ ...filter, precision: 'month', operand: '', upper: '' }])
    expect(field('Valeur').value).toBe('')
    expect(field('Valeur').placeholder).toBe('2026-01')
    await type('Valeur', '2026-01-15')
    await pressEnter('Valeur')
    expect(refusal()).toBe('Un mois est attendu, comme 2026-01')
    await type('Valeur', '2026-02')
    await pressEnter('Valeur')
    expect(changes[changes.length - 1]).toEqual({
      ...filter,
      precision: 'month',
      operand: '2026-02',
      upper: '',
    })
    expect(labels(mounted, 'Agrégat')).toContain('MIN(vendue_le (Mois))')
    // COUNT(DISTINCT) keeps the precision, COUNT drops it.
    await choose(mounted, 'Agrégat', 'count_distinct')
    expect(changes[changes.length - 1]).toMatchObject({
      aggregate: 'count_distinct',
      precision: 'month',
    })
    await choose(mounted, 'Agrégat', 'count')
    expect(changes[changes.length - 1]).toMatchObject({ aggregate: 'count', precision: undefined })
    expect(select(mounted, 'Précision')).toBeUndefined()
  })

  it('lists the values at its precision, NULL only in the list kept', async () => {
    const filter: PivotColumnFilter = {
      ...ON_COLUMN,
      column: 1,
      precision: 'month',
      condition: 'in_list',
    }
    const { mounted, changes, readValues } = await mountPanel(filter)
    await flushPromises()
    expect(readValues).toHaveBeenCalledExactlyOnceWith(1, 'month')
    const list = () => mounted.findComponent(PivotValuesList)
    expect(list().props('offersNull')).toBe(true)
    list().vm.$emit('update:kept', ['Nord'])
    await nextTick()
    expect(changes).toEqual([{ ...filter, values: ['Nord'] }])
    await choose(mounted, 'Condition', 'not_in_list')
    expect(list().props('offersNull')).toBe(false)
    expect(list().props('kept')).toEqual(['Nord'])
    await choose(mounted, 'Précision', 'year')
    expect(readValues).toHaveBeenLastCalledWith(1, 'year')
  })

  it('offers a filter on a measure the tests, without a list', async () => {
    const { mounted } = await mountPanel(ON_MEASURE)
    expect(labels(mounted, 'Condition')).toEqual([
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
  })

  it('offers each aggregate, refusing those that add up a column of no numbers', async () => {
    const { mounted, changes } = await mountPanel({ ...ON_MEASURE, column: 0, aggregate: 'count' })
    const { items } = selectProps(mounted, 'Agrégat')
    expect(items.map(({ label }) => label)).toEqual([
      'COUNT(*)',
      'COUNT(region)',
      'COUNT(DISTINCT region)',
      'SUM(region)',
      'SUM(DISTINCT region)',
      'AVG(region)',
      'AVG(DISTINCT region)',
      'MIN(region)',
      'MAX(region)',
    ])
    const refused = items.filter(({ disabled }) => disabled)
    expect(refused.map(({ value }) => value)).toEqual([
      'sum',
      'sum_distinct',
      'avg',
      'avg_distinct',
    ])
    expect(refused.every(({ hint }) => hint === 'Demande une colonne de nombres')).toBe(true)
    await choose(mounted, 'Agrégat', 'max')
    expect(changes[0]).toMatchObject({ aggregate: 'max' })
  })

  it('refuses a value that no longer compares, once its aggregate has changed', async () => {
    const { mounted } = await mountPanel({
      ...ON_MEASURE,
      column: 1,
      aggregate: 'count',
      operand: '2',
    })
    expect(refusal()).toBeUndefined()
    await choose(mounted, 'Agrégat', 'max')
    expect(refusal()).toBe('Un instant est attendu, comme 2026-01-15 10:30:00')
  })

  it('offers the levels of the two axes, the same level placed twice being one', async () => {
    const level = (column: number, precision: PivotLevel['precision'] = 'none'): PivotLevel => ({
      column,
      name: '',
      precision,
    })
    const { mounted, changes } = await mountPanel(
      ON_MEASURE,
      [level(0), level(0), level(1, 'month')],
      [level(1, 'year')],
    )
    expect(labels(mounted, 'Niveau')).toEqual([
      'Le niveau le plus fin',
      'region',
      'vendue_le (Mois)',
      'vendue_le (Année)',
    ])
    expect(selectProps(mounted, 'Niveau').modelValue).toBe('default')
    await choose(mounted, 'Niveau', '1:month')
    expect(changes[0]).toEqual({ ...ON_MEASURE, level: 1, level_precision: 'month' })
    expect(selectProps(mounted, 'Niveau').modelValue).toBe('1:month')
    await choose(mounted, 'Niveau', 'default')
    expect(changes[1]).toEqual({ ...ON_MEASURE, level: undefined, level_precision: 'none' })
  })

  it('asks for the name of the filter', async () => {
    const { named } = await mountPanel(ON_COLUMN)
    const buttons = Array.from(document.body.querySelectorAll('[role="dialog"] button'))
    buttons
      .find((button) => button.textContent?.includes('Nommer'))
      ?.dispatchEvent(new Event('click'))
    expect(named).toHaveBeenCalledOnce()
  })
})
