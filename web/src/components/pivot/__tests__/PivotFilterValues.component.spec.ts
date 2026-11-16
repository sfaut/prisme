import { afterEach, describe, expect, it } from 'vitest'

import { type VueWrapper, flushPromises, mount } from '@vue/test-utils'

import type { Cell } from '@/api/querySession'

import PivotFilterValues from '../PivotFilterValues.vue'

let wrapper: VueWrapper | null = null

afterEach(() => {
  wrapper?.unmount()
  wrapper = null
})

/**
 * Mounts a list, and gives what it keeps.
 *
 * @param read Reads the values of the column.
 * @param kept Values the filter keeps.
 * @param offersNull Does the list offer NULL?
 */
async function mountList(read: () => Promise<Cell[]>, kept: Cell[] = [], offersNull = true) {
  const changes: Cell[][] = []
  const mounted = mount(PivotFilterValues, {
    attachTo: document.body,
    props: {
      label: 'region',
      read,
      offersNull,
      kept,
      'onUpdate:kept': (values: Cell[]) => {
        changes.push(values)
        mounted.setProps({ kept: values })
      },
    },
  })
  wrapper = mounted
  await flushPromises()
  return { mounted, changes }
}

/**
 * Clicks a button of the list.
 *
 * @param label Label of the button.
 */
async function clickButton(label: string): Promise<void> {
  const buttons = Array.from(document.body.querySelectorAll('button'))
  buttons.find((button) => button.textContent?.trim() === label)?.dispatchEvent(new Event('click'))
  await flushPromises()
}

describe('PivotFilterValues', () => {
  it('says when it holds the values of the rows received so far', async () => {
    const note = 'Valeurs des lignes reçues à l’ouverture de la liste, la requête étant en cours'
    const { mounted } = await mountList(() => Promise.resolve(['Nord']))
    expect(mounted.text()).not.toContain(note)
    mounted.unmount()
    const partial = mount(PivotFilterValues, {
      attachTo: document.body,
      props: {
        label: 'region',
        read: () => Promise.resolve(['Nord']),
        offersNull: true,
        kept: [],
        partial: true,
      },
    })
    wrapper = partial
    await flushPromises()
    expect(partial.text()).toContain(note)
    // The end of the reception leaves the values read as they are.
    await partial.setProps({ partial: false })
    expect(partial.text()).toContain(note)
  })

  it('checks and unchecks the values, in the order of the list', async () => {
    const { changes } = await mountList(() => Promise.resolve([null, 'Nord', 'Sud']), ['Sud'])
    await clickButton('Tout')
    expect(changes[changes.length - 1]).toEqual([null, 'Nord', 'Sud'])
    await clickButton('Aucun')
    expect(changes[changes.length - 1]).toEqual([])
  })

  it('applies to the values the search shows', async () => {
    const { changes } = await mountList(() => Promise.resolve([null, 'Nord', 'Sud']), ['Sud'])
    const search = document.body.querySelector<HTMLInputElement>('input')
    search!.value = 'NOR'
    search!.dispatchEvent(new Event('input'))
    await flushPromises()
    await clickButton('Tout')
    expect(changes[changes.length - 1]).toEqual(['Nord', 'Sud'])
    await clickButton('Aucun')
    expect(changes[changes.length - 1]).toEqual(['Sud'])
  })

  it('offers no NULL when asked to, and writes none', async () => {
    const { changes } = await mountList(
      () => Promise.resolve([null, 'Nord', 'Sud']),
      ['Sud', null],
      false,
    )
    await clickButton('Tout')
    expect(changes[changes.length - 1]).toEqual(['Nord', 'Sud'])
  })

  it('tells when the values cannot be read', async () => {
    await mountList(() => Promise.reject(new Error('no engine')))
    const alert = document.body.querySelector('[role="alert"]')
    expect(alert?.textContent).toContain('n’ont pas pu être lues')
  })

  it('tells that it reads the values', async () => {
    await mountList(() => new Promise(() => undefined))
    expect(document.body.querySelector('[role="status"]')?.textContent).toContain(
      'Lecture des valeurs',
    )
  })
})
