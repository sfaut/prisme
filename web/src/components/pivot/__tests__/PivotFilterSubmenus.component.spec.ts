import { afterEach, describe, expect, it } from 'vitest'

import { type VueWrapper, flushPromises, mount } from '@vue/test-utils'
import { nextTick } from 'vue'

import type { ColumnTypeInfo } from '@/api/generated/ColumnTypeInfo'
import type { Cell } from '@/api/querySession'

import type { PivotColumnFilter, PivotFilter, PivotMeasureFilter } from '../pivotConfig'
import PivotFilterListSubmenu from '../PivotFilterListSubmenu.vue'
import PivotFilterOperandSubmenu from '../PivotFilterOperandSubmenu.vue'
import PivotFilterValues from '../PivotFilterValues.vue'

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
 * Filter `COUNT(vendue_le) > 2`.
 */
const ON_MEASURE: PivotMeasureFilter = {
  kind: 'measure',
  aggregate: 'count',
  column: 1,
  name: '',
  condition: 'greater',
  operand: '2',
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
 * Mounts the fields of a filter, the filter following their changes, as its menu does.
 *
 * @param filter Filter.
 */
function mountOperand(filter: PivotFilter) {
  const changes: PivotFilter[] = []
  let current = filter
  const mounted: VueWrapper = mount(PivotFilterOperandSubmenu, {
    attachTo: document.body,
    props: {
      filter,
      types: TYPES,
      onChange: (changed: Partial<PivotFilter>) => {
        current = { ...current, ...changed } as PivotFilter
        changes.push(current)
        void mounted.setProps({ filter: current })
      },
    },
  })
  wrapper = mounted
  return { mounted, changes }
}

/**
 * Field of the submenu.
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
 * Presses a key in a field.
 *
 * @param label Name of the field.
 * @param key Key.
 */
async function press(label: string, key: string): Promise<void> {
  field(label).dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true }))
  await nextTick()
}

/**
 * Text of the refusal the submenu shows, if any.
 */
function refusal(): string | undefined {
  return document.body.querySelector('[role="alert"]')?.textContent?.trim()
}

describe('PivotFilterOperandSubmenu', () => {
  it('asks a condition for its value or its bounds, with an example', async () => {
    const { mounted } = mountOperand(ON_COLUMN)
    const fields = () => Array.from(document.body.querySelectorAll('input')).map((f) => f.ariaLabel)
    expect(fields()).toEqual(['De', 'À'])
    await mounted.setProps({ filter: { ...ON_COLUMN, condition: 'less' } })
    expect(fields()).toEqual(['Valeur'])
    await mounted.setProps({ filter: { ...ON_COLUMN, column: 1, precision: 'month' } })
    expect(field('De').placeholder).toBe('2026-01')
  })

  it('refuses a wrong value at Enter, showing why, and applies a right one', async () => {
    const { changes } = mountOperand(ON_COLUMN)
    await type('De', 'abc')
    expect(refusal()).toBeUndefined()
    await press('De', 'Enter')
    expect(refusal()).toBe('Un nombre est attendu, comme 100 ou 12.5')
    expect(field('De').getAttribute('aria-invalid')).toBe('true')
    expect(changes).toEqual([])
    await type('De', ' 100 ')
    await press('De', 'Enter')
    expect(refusal()).toBeUndefined()
    expect(changes).toEqual([{ ...ON_COLUMN, operand: '100' }])
    expect(field('De').value).toBe('100')
  })

  it('applies a value on leaving its field', async () => {
    const { changes } = mountOperand(ON_COLUMN)
    await type('À', '400')
    field('À').dispatchEvent(new Event('blur'))
    await nextTick()
    expect(changes).toEqual([{ ...ON_COLUMN, upper: '400' }])
  })

  it('applies when it closes all that is being typed, except what is refused', async () => {
    const { mounted, changes } = mountOperand(ON_COLUMN)
    await type('De', '100')
    await type('À', 'abc')
    mounted.unmount()
    wrapper = null
    expect(changes).toEqual([{ ...ON_COLUMN, operand: '100' }])
  })

  it('forgets what is being typed at Escape', async () => {
    const { mounted, changes } = mountOperand(ON_COLUMN)
    await type('De', '100')
    await press('De', 'Escape')
    field('De').dispatchEvent(new Event('blur'))
    mounted.unmount()
    wrapper = null
    expect(changes).toEqual([])
  })

  it('refuses a value that no longer compares, once its aggregate has changed', async () => {
    const { mounted } = mountOperand(ON_MEASURE)
    expect(refusal()).toBeUndefined()
    await mounted.setProps({ filter: { ...ON_MEASURE, aggregate: 'max' } })
    expect(refusal()).toBe('Un instant est attendu, comme 2026-01-15 10:30:00')
  })
})

describe('PivotFilterListSubmenu', () => {
  it('lists the values, and sends the values checked', async () => {
    const changes: Cell[][] = []
    const mounted = mount(PivotFilterListSubmenu, {
      attachTo: document.body,
      props: {
        label: 'region',
        read: () => Promise.resolve([null, 'Nord', 'Sud']),
        offersNull: false,
        kept: ['Nord'],
        onChange: (values: Cell[]) => changes.push(values),
      },
    })
    wrapper = mounted
    await flushPromises()
    const list = mounted.findComponent(PivotFilterValues)
    expect(list.props()).toMatchObject({ label: 'region', offersNull: false, kept: ['Nord'] })
    list.vm.$emit('update:kept', ['Nord', 'Sud'])
    expect(changes).toEqual([['Nord', 'Sud']])
  })
})
