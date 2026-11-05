import { afterEach, describe, expect, it, vi } from 'vitest'

import { type VueWrapper, flushPromises, mount } from '@vue/test-utils'
import { PopoverRoot } from 'reka-ui'
import { nextTick } from 'vue'

import PivotConcatPanel from '../PivotConcatPanel.vue'
import type { PivotConcat, PivotConfig } from '../pivotConfig'
import UiSelect from '../ui/UiSelect.vue'

/**
 * Columns of the result.
 */
const NAMES = ['region', 'ville', 'montant']

let wrapper: VueWrapper | null = null

afterEach(() => {
  wrapper?.unmount()
  wrapper = null
  document.body.replaceChildren()
})

/**
 * Mounts the settings of `GROUP_CONCAT(ville)`, the measure following their changes.
 *
 * @param concat Settings of the measure.
 */
async function mountPanel(concat: PivotConcat = { separator: ', ' }) {
  const anchor = document.createElement('li')
  anchor.append(document.createElement('button'))
  document.body.append(anchor)
  const changes: PivotConcat[] = []
  const config = (settings: PivotConcat): PivotConfig => ({
    rows: [],
    columns: [],
    measures: [{ aggregate: 'group_concat', column: 1, name: '', concat: settings }],
    filters: [],
  })
  const closed = vi.fn<() => void>()
  const named = vi.fn<() => void>()
  const mounted = mount(PivotConcatPanel, {
    attachTo: document.body,
    props: {
      anchor,
      index: 0,
      config: config(concat),
      names: NAMES,
      types: [],
      onChange: (changed: PivotConcat) => {
        changes.push(changed)
        mounted.setProps({ config: config(changed) })
      },
      onClose: closed,
      onName: named,
    },
  })
  wrapper = mounted
  // The content of the popover draws after the mounting of the panel.
  await flushPromises()
  return { mounted, changes, closed, named, anchor }
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
 * Types in a field, then presses Enter.
 *
 * @param label Name of the field.
 * @param text Text.
 */
async function typeAndEnter(label: string, text: string): Promise<void> {
  await type(label, text)
  field(label).dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter' }))
  await nextTick()
}

/**
 * Button of the panel.
 *
 * @param label Name of the button, or its text.
 */
function button(label: string): HTMLButtonElement {
  const buttons = [...document.body.querySelectorAll<HTMLButtonElement>('button')]
  return buttons.find(
    (item) => item.getAttribute('aria-label') === label || item.textContent?.trim() === label,
  )!
}

describe('PivotConcatPanel', () => {
  it('names itself by the expression of its measure', async () => {
    await mountPanel()
    const content = document.body.querySelector('[aria-label^="Réglages de"]')
    expect(content?.getAttribute('aria-label')).toBe(
      "Réglages de GROUP_CONCAT(ville SEPARATOR ', ')",
    )
  })

  it('takes DISTINCT at once', async () => {
    const { changes } = await mountPanel()
    const box = document.body.querySelector<HTMLInputElement>('input[type="checkbox"]')!
    box.click()
    await nextTick()
    expect(changes).toEqual([{ separator: ', ', distinct: true }])
  })

  it('adds, turns, changes and removes the keys of the order', async () => {
    const { mounted, changes } = await mountPanel()
    expect(document.body.textContent).toContain('Ordre des lignes du résultat')
    // The first key reads the column of the measure, the next one the first column left.
    button('Ajouter une clef').click()
    await nextTick()
    button('Ajouter une clef').click()
    await nextTick()
    expect(changes[changes.length - 1]?.order).toEqual([{ column: 1 }, { column: 0 }])
    button('Clef 2 croissante (ASC)').click()
    await nextTick()
    expect(changes[changes.length - 1]?.order).toEqual([
      { column: 1 },
      { column: 0, descending: true },
    ])
    const second = mounted
      .findAllComponents(UiSelect)
      .find((item) => (item.props() as { label: string }).label === 'Colonne de la clef 2')!
    second.vm.$emit('update:modelValue', 2)
    await nextTick()
    expect(changes[changes.length - 1]?.order).toEqual([
      { column: 1 },
      { column: 2, descending: true },
    ])
    button('Retirer la clef 1').click()
    await nextTick()
    expect(changes[changes.length - 1]?.order).toEqual([{ column: 2, descending: true }])
  })

  it('keeps the separator as it is typed, its spaces included', async () => {
    const { changes } = await mountPanel()
    await typeAndEnter('Séparateur', ' | ')
    expect(changes).toEqual([{ separator: ' | ' }])
    await typeAndEnter('Séparateur', '')
    expect(changes[changes.length - 1]).toEqual({ separator: '' })
  })

  it('takes OFFSET only after a LIMIT, which takes it away when it leaves', async () => {
    const { changes } = await mountPanel()
    expect(field('OFFSET').disabled).toBe(true)
    await typeAndEnter('LIMIT', '5')
    expect(changes[changes.length - 1]).toEqual({ separator: ', ', limit: 5 })
    expect(field('OFFSET').disabled).toBe(false)
    await typeAndEnter('OFFSET', '2')
    expect(changes[changes.length - 1]).toEqual({ separator: ', ', limit: 5, offset: 2 })
    await typeAndEnter('LIMIT', '')
    expect(changes[changes.length - 1]).toEqual({
      separator: ', ',
      limit: undefined,
      offset: undefined,
    })
    expect(field('OFFSET').value).toBe('')
  })

  it('refuses a number that is not a whole number, showing why, and changes nothing', async () => {
    const { changes } = await mountPanel({ separator: ', ', limit: 5 })
    await typeAndEnter('LIMIT', '-1')
    await typeAndEnter('LIMIT', '2.5')
    expect(changes).toEqual([])
    // The largest number is written as French writes it, its groups apart.
    expect(document.body.querySelector('[role="alert"]')?.textContent?.trim()).toBe(
      `LIMIT : Nombre entier de 0 à ${(0xffff_fffe).toLocaleString('fr-FR')} attendu`,
    )
    await typeAndEnter('LIMIT', '7')
    expect(changes).toEqual([{ separator: ', ', limit: 7 }])
    expect(document.body.querySelector('[role="alert"]')).toBeNull()
  })

  it('applies at its closing all that is being typed', async () => {
    const { mounted, changes, closed } = await mountPanel()
    await type('Séparateur', ';')
    await type('LIMIT', '3')
    mounted.findComponent(PopoverRoot).vm.$emit('update:open', false)
    await nextTick()
    expect(changes[changes.length - 1]).toEqual({ separator: ';', limit: 3, offset: undefined })
    expect(closed).toHaveBeenCalledOnce()
  })

  it('forgets what is being typed at Escape, and gives the focus to the button', async () => {
    const { changes, closed, anchor } = await mountPanel()
    await type('Séparateur', ';')
    field('Séparateur').dispatchEvent(
      new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }),
    )
    await nextTick()
    expect(closed).toHaveBeenCalledOnce()
    expect(document.activeElement).toBe(anchor.querySelector('button'))
    expect(changes).toEqual([])
  })

  it('asks for the name of the measure', async () => {
    const { named } = await mountPanel()
    button('Nommer…').click()
    expect(named).toHaveBeenCalled()
  })
})
