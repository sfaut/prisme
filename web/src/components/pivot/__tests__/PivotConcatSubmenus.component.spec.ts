import { afterEach, describe, expect, it } from 'vitest'

import { type VueWrapper, mount } from '@vue/test-utils'
import { type Component, nextTick } from 'vue'

import type { PivotConcatKey } from '@/api/generated/PivotConcatKey'

import PivotConcatOrderSubmenu from '../PivotConcatOrderSubmenu.vue'
import PivotConcatSeparatorSubmenu from '../PivotConcatSeparatorSubmenu.vue'
import PivotConcatWindowSubmenu from '../PivotConcatWindowSubmenu.vue'
import UiSelect from '@/components/ui/UiSelect.vue'

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
 * Mounts a submenu, its props following its changes, as the menu of the measure does.
 *
 * @param component Submenu.
 * @param props Props of the submenu.
 * @param follow Props of the submenu after a change.
 */
function mountSubmenu<T>(
  component: Component,
  props: Record<string, unknown>,
  follow: (change: T) => Record<string, unknown>,
) {
  const changes: T[] = []
  const mounted: VueWrapper = mount(component, {
    attachTo: document.body,
    props: {
      ...props,
      onChange: (change: T) => {
        changes.push(change)
        void mounted.setProps(follow(change))
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
 * Types in a field, then presses Enter.
 *
 * @param label Name of the field.
 * @param text Text.
 */
async function typeAndEnter(label: string, text: string): Promise<void> {
  await type(label, text)
  field(label).dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
  await nextTick()
}

/**
 * Button of the submenu.
 *
 * @param label Name of the button, or its text.
 */
function button(label: string): HTMLButtonElement {
  const buttons = [...document.body.querySelectorAll<HTMLButtonElement>('button')]
  return buttons.find(
    (item) =>
      item.getAttribute('aria-label') === label || item.textContent?.trim().startsWith(label),
  )!
}

describe('PivotConcatOrderSubmenu', () => {
  it('adds, turns, changes and removes the keys of the order', async () => {
    const { mounted, changes } = mountSubmenu<PivotConcatKey[]>(
      PivotConcatOrderSubmenu,
      { order: [], names: NAMES, column: 1 },
      (order) => ({ order }),
    )
    expect(document.body.textContent).toContain('Ordre des lignes du résultat')
    // The first key reads the column of the measure, the next one the first column left.
    button('Ajouter une clef').click()
    await nextTick()
    button('Ajouter une clef').click()
    await nextTick()
    expect(changes[changes.length - 1]).toEqual([{ column: 1 }, { column: 0 }])
    button('Clef 2 croissante (ASC)').click()
    await nextTick()
    expect(changes[changes.length - 1]).toEqual([{ column: 1 }, { column: 0, descending: true }])
    const second = mounted
      .findAllComponents(UiSelect)
      .find((item) => (item.props() as { label: string }).label === 'Colonne de la clef 2')!
    second.vm.$emit('update:modelValue', 2)
    await nextTick()
    expect(changes[changes.length - 1]).toEqual([{ column: 1 }, { column: 2, descending: true }])
    button('Retirer la clef 1').click()
    await nextTick()
    expect(changes[changes.length - 1]).toEqual([{ column: 2, descending: true }])
  })
})

describe('PivotConcatSeparatorSubmenu', () => {
  it('keeps the separator as it is typed, its spaces included', async () => {
    const { changes } = mountSubmenu<string>(
      PivotConcatSeparatorSubmenu,
      { separator: ', ' },
      (separator) => ({ separator }),
    )
    expect(field('Séparateur').value).toBe(', ')
    await typeAndEnter('Séparateur', ' | ')
    expect(changes).toEqual([' | '])
    await typeAndEnter('Séparateur', '')
    expect(changes[changes.length - 1]).toBe('')
  })

  it('reads and writes a line break with a backslash', async () => {
    const { changes } = mountSubmenu<string>(
      PivotConcatSeparatorSubmenu,
      { separator: '\r\n' },
      (separator) => ({ separator }),
    )
    expect(field('Séparateur').value).toBe('\\r\\n')
    await typeAndEnter('Séparateur', '\\t')
    expect(changes).toEqual(['\t'])
  })

  it('offers the usual separators, the current one pressed, which apply at once', async () => {
    const { changes } = mountSubmenu<string>(
      PivotConcatSeparatorSubmenu,
      { separator: ', ' },
      (separator) => ({ separator }),
    )
    const usual = [...document.body.querySelectorAll('[aria-label="Séparateurs courants"] button')]
    expect(
      usual.map((item) => [item.textContent?.trim(), item.getAttribute('aria-pressed')]),
    ).toEqual([
      ["Virgule', '", 'true'],
      ["Point-virgule'; '", 'false'],
      ["Barre' | '", 'false'],
      ["Retour à la ligne (LF)'\\n'", 'false'],
      ["Retour à la ligne (CR LF)'\\r\\n'", 'false'],
    ])
    button('Retour à la ligne (CR LF)').click()
    await nextTick()
    expect(changes).toEqual(['\r\n'])
    expect(field('Séparateur').value).toBe('\\r\\n')
  })

  it('applies what is being typed when the submenu closes, and forgets it after Escape', async () => {
    const closed = mountSubmenu<string>(
      PivotConcatSeparatorSubmenu,
      { separator: ', ' },
      (separator) => ({ separator }),
    )
    await type('Séparateur', ';')
    closed.mounted.unmount()
    wrapper = null
    expect(closed.changes).toEqual([';'])

    const escaped = mountSubmenu<string>(
      PivotConcatSeparatorSubmenu,
      { separator: ', ' },
      (separator) => ({ separator }),
    )
    await type('Séparateur', ';')
    field('Séparateur').dispatchEvent(
      new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }),
    )
    field('Séparateur').dispatchEvent(new FocusEvent('blur'))
    escaped.mounted.unmount()
    wrapper = null
    expect(escaped.changes).toEqual([])
  })
})

describe('PivotConcatWindowSubmenu', () => {
  /**
   * Mounts the window of the values, its props following its changes.
   *
   * @param limit `LIMIT` of the measure.
   * @param offset `OFFSET` of the measure.
   */
  function mountWindow(limit?: number, offset?: number) {
    type Window = { limit?: number; offset?: number }
    return mountSubmenu<Window>(PivotConcatWindowSubmenu, { limit, offset }, (window) => window)
  }

  it('takes OFFSET only after a LIMIT, which takes it away when it leaves', async () => {
    const { changes } = mountWindow()
    expect(field('OFFSET').disabled).toBe(true)
    await typeAndEnter('LIMIT', '5')
    expect(changes[changes.length - 1]).toEqual({ limit: 5, offset: undefined })
    expect(field('OFFSET').disabled).toBe(false)
    await typeAndEnter('OFFSET', '2')
    expect(changes[changes.length - 1]).toEqual({ limit: 5, offset: 2 })
    await typeAndEnter('LIMIT', '')
    expect(changes[changes.length - 1]).toEqual({ limit: undefined, offset: undefined })
    expect(field('OFFSET').value).toBe('')
  })

  it('refuses a number that is not a whole number, showing why, and changes nothing', async () => {
    const { changes } = mountWindow(5)
    await typeAndEnter('LIMIT', '-1')
    await typeAndEnter('LIMIT', '2.5')
    expect(changes).toEqual([])
    // The largest number is written as French writes it, its groups apart.
    expect(document.body.querySelector('[role="alert"]')?.textContent?.trim()).toBe(
      `LIMIT : Nombre entier de 0 à ${(0xffff_fffe).toLocaleString('fr-FR')} attendu`,
    )
    await typeAndEnter('LIMIT', '7')
    expect(changes).toEqual([{ limit: 7, offset: undefined }])
    expect(document.body.querySelector('[role="alert"]')).toBeNull()
  })

  it('applies what is being typed when the submenu closes', async () => {
    const { mounted, changes } = mountWindow()
    await type('LIMIT', '3')
    mounted.unmount()
    wrapper = null
    expect(changes).toEqual([{ limit: 3, offset: undefined }])
  })
})
