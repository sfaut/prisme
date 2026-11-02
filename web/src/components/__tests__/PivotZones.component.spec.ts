import { Check } from '@lucide/vue'
import { afterEach, describe, expect, it } from 'vitest'

import { type VueWrapper, mount } from '@vue/test-utils'
import { nextTick } from 'vue'

import PivotValuesList from '../PivotValuesList.vue'
import PivotZones from '../PivotZones.vue'
import type { PivotConfig } from '../pivotConfig'
import type { MenuGroup } from '../ui/menuItem'
import UiDropdownMenu from '../ui/UiDropdownMenu.vue'

/**
 * Columns of the result.
 */
const NAMES = ['region', 'ville', 'montant']

/**
 * Configuration with one element in two zones.
 */
const CONFIG: PivotConfig = {
  rows: [{ column: 0, name: '', precision: 'none' }],
  columns: [],
  measures: [{ aggregate: 'sum', column: 2, name: '' }],
  filters: [],
}

let wrapper: VueWrapper | null = null

// jsdom does not lay out the page: a list of values scrolls to its filter.
Element.prototype.scrollIntoView = () => undefined

afterEach(() => {
  wrapper?.unmount()
  wrapper = null
})

/**
 * Mounts the zones in the page, the configuration following their changes.
 */
function mountZones() {
  const mounted = mount(PivotZones, {
    attachTo: document.body,
    props: {
      names: NAMES,
      types: [],
      marks: [],
      foreignKeys: [],
      readValues: () => Promise.resolve([null, 'Nord', 'Sud']),
      config: CONFIG,
      'onUpdate:config': (config: PivotConfig) => mounted.setProps({ config }),
    },
  })
  wrapper = mounted
  return mounted
}

/**
 * Configuration shown by the zones.
 *
 * @param mounted Mounted zones.
 */
function shownConfig(mounted: VueWrapper): PivotConfig {
  return mounted.props('config' as never) as PivotConfig
}

/**
 * Drops what is dragged on the page, outside the zones.
 */
function dropOnPage(): void {
  document.body.dispatchEvent(new Event('drop', { bubbles: true, cancelable: true }))
}

/**
 * Sends an event of a drag and tells whether it is accepted: the browser
 * shows the forbidden cursor otherwise.
 *
 * @param target Element under the pointer.
 * @param type Type of the event.
 */
function accepted(target: EventTarget, type: 'dragenter' | 'dragover'): boolean {
  const event = new Event(type, { bubbles: true, cancelable: true })
  target.dispatchEvent(event)
  return event.defaultPrevented
}

/**
 * Groups of the menu of the first element of the zones.
 *
 * @param mounted Mounted zones.
 */
function firstElementGroups(mounted: VueWrapper): MenuGroup[] {
  const menu = mounted.findAllComponents(UiDropdownMenu).find((item) => item.props('groups'))
  return menu?.props('groups') ?? []
}

describe('PivotZones', () => {
  it('frames the zone under a dragged element, even with elements in it', async () => {
    const mounted = mountZones()
    await mounted.find('[data-element]').trigger('dragstart')
    await mounted.findAll('section')[0]!.trigger('dragover', { clientY: 0 })
    const frames = mounted.findAll('.pivot-zone')
    expect(frames[0]!.classes()).toContain('drop-target')
    expect(frames[1]!.classes()).not.toContain('drop-target')
  })

  it('accepts a column of the list at each entry in an element of a zone', async () => {
    const mounted = mountZones()
    await mounted.find('li[draggable="true"]').trigger('dragstart')
    const inZone = mounted.find('section h3').element
    expect(accepted(inZone, 'dragenter')).toBe(true)
    expect(accepted(inZone, 'dragover')).toBe(true)
    expect(accepted(document.body, 'dragenter')).toBe(false)
  })

  it('accepts an element of a zone everywhere in the page, until the end of its drag', async () => {
    const mounted = mountZones()
    const element = mounted.find('[data-element]')
    await element.trigger('dragstart')
    expect(accepted(element.element, 'dragenter')).toBe(true)
    expect(accepted(document.body, 'dragenter')).toBe(true)
    expect(accepted(document.body, 'dragover')).toBe(true)
    await element.trigger('dragend')
    expect(accepted(document.body, 'dragenter')).toBe(false)
    expect(accepted(document.body, 'dragover')).toBe(false)
  })

  it('refuses what comes from outside the panel', () => {
    const mounted = mountZones()
    const inZone = mounted.find('section h3').element
    expect(accepted(inZone, 'dragenter')).toBe(false)
    expect(accepted(inZone, 'dragover')).toBe(false)
  })

  it('removes an element released outside the zones', async () => {
    const mounted = mountZones()
    await mounted.find('[data-element]').trigger('dragstart')
    dropOnPage()
    await nextTick()
    expect(shownConfig(mounted).rows).toEqual([])
    expect(shownConfig(mounted).measures).toEqual(CONFIG.measures)
  })

  it('keeps an element when the drag ends without a drop', async () => {
    const mounted = mountZones()
    const element = mounted.find('[data-element]')
    await element.trigger('dragstart')
    await element.trigger('dragend')
    dropOnPage()
    await nextTick()
    expect(shownConfig(mounted)).toEqual(CONFIG)
  })

  it('keeps an element dropped into a zone, where it moves', async () => {
    const mounted = mountZones()
    await mounted.find('[data-element]').trigger('dragstart')
    await mounted.findAll('section')[1]!.trigger('drop', { clientY: 0 })
    expect(shownConfig(mounted).rows).toEqual([])
    expect(shownConfig(mounted).columns).toEqual(CONFIG.rows)
  })

  it('gives a column dropped into the measures its default aggregate', async () => {
    const mounted = mountZones()
    await mounted.setProps({
      types: [
        {
          family: 'integer',
          name: 'INT',
          origin: { schema: 's', table: 't', column: 'id', primary_key: true },
        },
        { family: 'integer', name: 'INT' },
        { family: 'decimal', name: 'DECIMAL(10,2)' },
      ],
      foreignKeys: [false, true, false],
    })
    const zone = mounted.findAll('section')[2]!
    for (const column of [0, 1]) {
      await mounted.findAll('li[draggable="true"]')[column]!.trigger('dragstart')
      await zone.trigger('drop', { clientY: 0 })
    }
    expect(shownConfig(mounted).measures.map(({ aggregate }) => aggregate)).toEqual([
      'sum',
      'count_star',
      'count',
    ])
  })

  it('gives a level of a date and time the precisions of its menu, the current one checked', async () => {
    const mounted = mountZones()
    await mounted.setProps({
      types: [
        { family: 'datetime', name: 'DATETIME' },
        { family: 'text', name: 'TEXT' },
        { family: 'decimal', name: 'DECIMAL(10,2)' },
      ],
    })
    const precision = () => firstElementGroups(mounted).find((group) => group.label === 'Précision')
    expect(precision()?.items.map((item) => item.label)).toEqual([
      'Année',
      'Trimestre',
      'Mois',
      'Semaine',
      'Date',
      'Date et heure',
    ])
    expect(precision()?.items[5]?.icon).toBe(Check)
    precision()?.items[2]?.onSelect?.()
    await nextTick()
    expect(shownConfig(mounted).rows).toEqual([{ column: 0, name: '', precision: 'month' }])
    expect(precision()?.items[2]?.icon).toBe(Check)
    expect(precision()?.items[5]?.icon).not.toBe(Check)
    expect(mounted.find('[data-element]').text()).toContain('region (Mois)')
  })

  it('gives no precision to a level without dates, nor to a measure', async () => {
    const mounted = mountZones()
    await mounted.setProps({
      types: [
        { family: 'text', name: 'TEXT' },
        { family: 'text', name: 'TEXT' },
        { family: 'date', name: 'DATE' },
      ],
    })
    const labels = mounted
      .findAllComponents(UiDropdownMenu)
      .flatMap((menu) => (menu.props('groups') ?? []).map((group: MenuGroup) => group.label))
    expect(labels).toEqual(['Élément', 'Agrégat', 'Élément'])
  })

  /**
   * Drops a column of the list, or an element, on the zone Filtres.
   *
   * @param mounted Mounted zones.
   * @param selector Element dragged.
   */
  async function dropOnFilters(mounted: VueWrapper, selector: string): Promise<void> {
    await mounted.find(selector).trigger('dragstart')
    await mounted.find('section[aria-label="Filtres"]').trigger('drop', { clientY: 0 })
    await nextTick()
    await nextTick()
  }

  it('opens the list of the values of a column dropped into the filters', async () => {
    const mounted = mountZones()
    await dropOnFilters(mounted, 'ul[aria-label="Colonnes du résultat"] li')
    expect(shownConfig(mounted).filters).toEqual([
      { kind: 'values', column: 0, name: '', values: [] },
    ])
    const list = mounted.findComponent(PivotValuesList)
    expect(list.props('label')).toBe('region')
    list.vm.$emit('update:kept', ['Nord'])
    await nextTick()
    expect(shownConfig(mounted).filters[0]).toMatchObject({ values: ['Nord'] })
    expect(mounted.find('section[aria-label="Filtres"] [data-element]').text()).toContain(
      "region IN ('Nord')",
    )
    list.vm.$emit('close')
    await nextTick()
    expect(mounted.findComponent(PivotValuesList).exists()).toBe(false)
  })

  it('types the value of a measure dropped into the filters, and refuses a wrong one', async () => {
    const mounted = mountZones()
    await mounted.setProps({
      types: [
        { family: 'text', name: 'TEXT' },
        { family: 'text', name: 'TEXT' },
        { family: 'decimal', name: 'DECIMAL' },
      ],
    })
    await dropOnFilters(mounted, 'section[aria-label="Mesures"] [data-element]')
    expect(shownConfig(mounted).measures).toEqual(CONFIG.measures)
    const zone = () => mounted.find('section[aria-label="Filtres"]')
    const field = () => zone().find('input')
    expect(field().attributes('aria-label')).toBe('Valeur de SUM(montant) > ?')
    expect(document.activeElement).toBe(field().element)

    await field().setValue('abc')
    await field().trigger('keydown', { key: 'Enter' })
    expect(zone().find('[role="alert"]').text()).toBe('Un nombre est attendu, comme 100 ou 12.5')
    expect(field().exists()).toBe(true)

    await field().setValue(' 100 ')
    await field().trigger('keydown', { key: 'Enter' })
    expect(zone().find('[role="alert"]').exists()).toBe(false)
    expect(field().exists()).toBe(false)
    expect(shownConfig(mounted).filters[0]).toMatchObject({ operand: '100' })
    expect(zone().find('[data-element]').text()).toContain('SUM(montant) > 100')
  })

  it('keeps the former value when leaving the field on a wrong one, or on Escape', async () => {
    const mounted = mountZones()
    await mounted.setProps({
      types: [{ family: 'decimal', name: 'DECIMAL' }],
      config: {
        ...CONFIG,
        filters: [
          {
            kind: 'measure',
            aggregate: 'count_star',
            column: 0,
            name: '',
            comparison: 'greater',
            operand: '1',
          },
        ],
      },
    })
    const zone = () => mounted.find('section[aria-label="Filtres"]')
    const groups = () =>
      mounted
        .findAllComponents(UiDropdownMenu)
        .flatMap((menu) => (menu.props('groups') ?? []) as MenuGroup[])
    const openValue = async () => {
      const value = groups()
        .find((group) => group.label === 'Comparaison')
        ?.items.find((item) => item.label === 'Valeur…')
      value?.onSelect?.()
      await nextTick()
    }

    await openValue()
    await zone().find('input').setValue('abc')
    await zone().find('input').trigger('blur')
    expect(zone().find('input').exists()).toBe(false)
    expect(zone().find('[role="alert"]').exists()).toBe(false)
    expect(shownConfig(mounted).filters[0]).toMatchObject({ operand: '1' })

    await openValue()
    await zone().find('input').setValue('2')
    await zone().find('input').trigger('keydown', { key: 'Escape' })
    expect(shownConfig(mounted).filters[0]).toMatchObject({ operand: '1' })
  })

  it('gives a measure the choice of a filter on it, and a filter its groups', async () => {
    const mounted = mountZones()
    const groups = () =>
      mounted
        .findAllComponents(UiDropdownMenu)
        .flatMap((menu) => (menu.props('groups') ?? []) as MenuGroup[])
    const item = (group: string, label: string) =>
      groups()
        .filter((candidate) => candidate.label === group)
        .flatMap((candidate) => candidate.items)
        .find((candidate) => candidate.label === label)
    item('Élément', 'Filtrer sur cette mesure')?.onSelect?.()
    await nextTick()
    await mounted.find('section[aria-label="Filtres"] input').trigger('keydown', { key: 'Escape' })
    expect(shownConfig(mounted).filters).toMatchObject([
      { kind: 'measure', aggregate: 'sum', column: 2, comparison: 'greater', operand: '' },
    ])
    expect(groups().map((group) => group.label)).toEqual([
      'Élément',
      'Agrégat',
      'Élément',
      'Agrégat',
      'Comparaison',
      'Niveau',
      'Élément',
    ])

    item('Comparaison', '<=')?.onSelect?.()
    await nextTick()
    expect(shownConfig(mounted).filters[0]).toMatchObject({ comparison: 'less_or_equal' })
    expect(item('Comparaison', '<=')?.icon).toBe(Check)

    const levels = () => groups().find((group) => group.label === 'Niveau')?.items ?? []
    expect(levels().map(({ label }) => label)).toEqual(['Le niveau le plus fin', 'region'])
    expect(levels()[0]?.icon).toBe(Check)
    levels()[1]?.onSelect?.()
    await nextTick()
    expect(shownConfig(mounted).filters[0]).toMatchObject({ level: 0 })
    expect(levels()[1]?.icon).toBe(Check)
  })

  it('marks a filter whose value no longer compares with its measure', async () => {
    const mounted = mountZones()
    await mounted.setProps({
      config: {
        ...CONFIG,
        filters: [
          {
            kind: 'measure',
            aggregate: 'count',
            column: 0,
            name: '',
            comparison: 'equal',
            operand: 'abc',
          },
        ],
      },
    })
    const element = mounted.find('section[aria-label="Filtres"] [data-element]')
    expect(element.classes()).toContain('invalid')
  })

  it('ignores a column of the list released outside the zones', async () => {
    const mounted = mountZones()
    await mounted.find('[draggable="true"]').trigger('dragstart')
    dropOnPage()
    await nextTick()
    expect(shownConfig(mounted)).toEqual(CONFIG)
  })
})
