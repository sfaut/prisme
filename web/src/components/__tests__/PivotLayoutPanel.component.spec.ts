import { afterEach, describe, expect, it } from 'vitest'

import { type VueWrapper, flushPromises, mount } from '@vue/test-utils'

import PivotLayoutPanel from '../PivotLayoutPanel.vue'
import { type PivotConfig, type PivotLevel, engineLayout } from '../pivotConfig'
import UiSelect from '../ui/UiSelect.vue'

/**
 * Columns of the result.
 */
const NAMES = ['region', 'ville', 'annee', 'trimestre']

/**
 * Level of a column.
 *
 * @param column Index of the column.
 */
function level(column: number): PivotLevel {
  return { column, name: '', precision: 'none' }
}

let wrapper: VueWrapper | null = null

afterEach(() => {
  wrapper?.unmount()
  wrapper = null
  document.body.replaceChildren()
})

/**
 * Mounts the panel and opens it, the configuration following its changes.
 *
 * @param start Configuration.
 */
async function openPanel(start: PivotConfig) {
  const state = { config: start }
  const mounted = mount(PivotLayoutPanel, {
    attachTo: document.body,
    props: {
      config: start,
      names: NAMES,
      'onUpdate:config': (config: PivotConfig) => {
        state.config = config
        void mounted.setProps({ config })
      },
    },
  })
  wrapper = mounted
  await mounted.find('button').trigger('click')
  await flushPromises()
  return { state, mounted }
}

/**
 * Box of a label of the panel, in an axis or out of them.
 *
 * @param label Text of the label.
 * @param section Axis it belongs to.
 */
function box(label: string, section?: string): HTMLInputElement {
  const root =
    section === undefined ? document : document.querySelector(`section[aria-label="${section}"]`)
  const found = [...(root?.querySelectorAll('label') ?? [])].find(
    (item) => item.textContent?.trim() === label,
  )
  const input = found?.querySelector('input')
  if (input === null || input === undefined) {
    throw new Error(`No box: ${label}`)
  }
  return input
}

/**
 * Clicks a box or a button.
 *
 * @param element Box or button.
 */
async function click(element: HTMLElement): Promise<void> {
  element.click()
  await flushPromises()
}

/**
 * Button of a section of the panel.
 *
 * @param label Text of the button.
 * @param section Axis it belongs to.
 */
function button(label: string, section: string): HTMLButtonElement {
  const found = [
    ...(document.querySelector(`section[aria-label="${section}"]`)?.querySelectorAll('button') ??
      []),
  ].find((item) => item.textContent?.trim() === label)
  if (found === undefined) {
    throw new Error(`No button: ${label}`)
  }
  return found
}

/**
 * Configuration of three levels of rows and two of columns.
 */
const LEVELS: PivotConfig = {
  rows: [level(0), level(1), level(2)],
  columns: [level(2), level(3)],
  measures: [],
  filters: [],
}

describe('PivotLayoutPanel', () => {
  it('starts on the compact form, the repetition inactive, every total shown', async () => {
    const { mounted } = await openPanel(LEVELS)
    const select = mounted.findComponent(UiSelect).props() as unknown as { modelValue: string }
    expect(select.modelValue).toBe('compact')
    expect(box('Répéter les libellés').disabled).toBe(true)
    expect(box('Total général', 'Lignes').checked).toBe(true)
    expect(box('Total général', 'Colonnes').checked).toBe(true)
    expect(box('region', 'Lignes').checked).toBe(true)
  })

  it('takes the form chosen, and then the repetition', async () => {
    const { state, mounted } = await openPanel(LEVELS)
    mounted.findComponent(UiSelect).vm.$emit('update:modelValue', 'tabular')
    await flushPromises()
    expect(state.config.form).toBe('tabular')
    const repeat = box('Répéter les libellés')
    expect(repeat.disabled).toBe(false)
    await click(repeat)
    expect(state.config.repeat_labels).toBe(true)
  })

  it('lists the levels that have a subtotal, the last one apart', async () => {
    await openPanel(LEVELS)
    const labels = (section: string) =>
      [
        ...(document.querySelector(`section[aria-label="${section}"]`)?.querySelectorAll('label') ??
          []),
      ].map((item) => item.textContent?.trim())
    expect(labels('Lignes')).toEqual(['Total général', 'region', 'ville'])
    expect(labels('Colonnes')).toEqual(['Total général', 'annee'])
  })

  it('leaves out an axis without levels, and the subtotals of a single level', async () => {
    await openPanel({ ...LEVELS, rows: [level(0)], columns: [] })
    expect(document.querySelector('section[aria-label="Colonnes"]')).toBeNull()
    const section = document.querySelector('section[aria-label="Lignes"]')
    expect(section?.textContent).toContain('Total général')
    expect(section?.textContent).not.toContain('Sous-totaux')
  })

  it('sets the grand total of each axis apart', async () => {
    const { state } = await openPanel(LEVELS)
    await click(box('Total général', 'Colonnes'))
    expect(engineLayout(state.config).columns.grandTotal).toBe(false)
    expect(engineLayout(state.config).rows.grandTotal).toBe(true)
    await click(box('Total général', 'Colonnes'))
    expect(engineLayout(state.config).columns.grandTotal).toBe(true)
  })

  it('sets the subtotal of a level, then all or none of them', async () => {
    const { state } = await openPanel(LEVELS)
    await click(box('ville', 'Lignes'))
    expect(engineLayout(state.config).rows.subtotals).toEqual([true, false, true])
    expect(box('ville', 'Lignes').checked).toBe(false)
    await click(button('Aucun', 'Lignes'))
    expect(engineLayout(state.config).rows.subtotals).toEqual([false, false, false])
    expect(box('region', 'Lignes').checked).toBe(false)
    expect(engineLayout(state.config).columns.subtotals).toEqual([])
    await click(button('Tout', 'Lignes'))
    expect(engineLayout(state.config).rows.subtotals).toEqual([])
    expect(box('region', 'Lignes').checked).toBe(true)
  })
})
