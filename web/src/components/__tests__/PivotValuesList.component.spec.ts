import { afterEach, describe, expect, it } from 'vitest'

import { type VueWrapper, flushPromises, mount } from '@vue/test-utils'

import type { Cell } from '@/api/querySession'

import PivotValuesList from '../PivotValuesList.vue'

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
 */
async function mountList(read: () => Promise<Cell[]>, kept: Cell[] = []) {
  const changes: Cell[][] = []
  const mounted = mount(PivotValuesList, {
    attachTo: document.body,
    props: {
      anchor: document.body,
      label: 'region',
      read,
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
  const buttons = Array.from(document.body.querySelectorAll('[role="dialog"] button'))
  buttons.find((button) => button.textContent?.trim() === label)?.dispatchEvent(new Event('click'))
  await flushPromises()
}

describe('PivotValuesList', () => {
  it('adds no element to its parent, which would shift the layout of its children', async () => {
    const { mounted } = await mountList(() => Promise.resolve(['Nord']))
    expect(document.body.querySelector('[role="dialog"]')).not.toBeNull()
    expect(mounted.vm.$el.parentElement.children).toHaveLength(0)
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
    const search = document.body.querySelector<HTMLInputElement>('[role="dialog"] input')
    search!.value = 'NOR'
    search!.dispatchEvent(new Event('input'))
    await flushPromises()
    await clickButton('Tout')
    expect(changes[changes.length - 1]).toEqual(['Nord', 'Sud'])
    await clickButton('Aucun')
    expect(changes[changes.length - 1]).toEqual(['Sud'])
  })

  it('tells when the values cannot be read', async () => {
    await mountList(() => Promise.reject(new Error('no engine')))
    const alert = document.body.querySelector('[role="dialog"] [role="alert"]')
    expect(alert?.textContent).toContain('n’ont pas pu être lues')
  })

  it('tells that it reads the values', async () => {
    await mountList(() => new Promise(() => undefined))
    expect(document.body.querySelector('[role="dialog"] [role="status"]')?.textContent).toContain(
      'Lecture des valeurs',
    )
  })
})
