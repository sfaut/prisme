import { Check } from '@lucide/vue'
import { afterEach, describe, expect, it, vi } from 'vitest'

import { type VueWrapper, mount } from '@vue/test-utils'
import { nextTick } from 'vue'

import type { Cell } from '@/api/querySession'

import PivotFilterPanel from '../PivotFilterPanel.vue'
import PivotZones from '../PivotZones.vue'
import type { PivotConfig, PivotFilter } from '../pivotConfig'
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

// jsdom does not lay out the page: the settings of a filter scroll to it.
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

  /**
   * Filter of a measure.
   *
   * @param changes What differs from it.
   */
  function onMeasure(changes: Partial<PivotFilter> = {}): PivotFilter {
    return {
      kind: 'measure',
      aggregate: 'count_star',
      column: 0,
      name: '',
      condition: 'greater',
      operand: '1',
      upper: '',
      level_precision: 'none',
      ...changes,
    } as PivotFilter
  }

  it('opens the settings of a column dropped into the filters', async () => {
    const mounted = mountZones()
    await dropOnFilters(mounted, 'ul[aria-label="Colonnes du résultat"] li')
    expect(shownConfig(mounted).filters).toEqual([
      {
        kind: 'column',
        column: 0,
        name: '',
        precision: 'none',
        condition: 'in_list',
        values: [],
        operand: '',
        upper: '',
      },
    ])
    const panel = mounted.findComponent(PivotFilterPanel)
    expect(panel.props('index')).toBe(0)
    const filter = shownConfig(mounted).filters[0]!
    panel.vm.$emit('change', { ...filter, values: ['Nord'] })
    await nextTick()
    expect(shownConfig(mounted).filters[0]).toMatchObject({ values: ['Nord'] })
    expect(mounted.find('section[aria-label="Filtres"] [data-element]').text()).toContain(
      "region IN ('Nord')",
    )
    panel.vm.$emit('close')
    await nextTick()
    expect(mounted.findComponent(PivotFilterPanel).exists()).toBe(false)
  })

  it('starts a column of numbers or of dates on its bounds, with its panel', async () => {
    const mounted = mountZones()
    await mounted.setProps({ types: [{ family: 'date', name: 'DATE' }] })
    await dropOnFilters(mounted, 'ul[aria-label="Colonnes du résultat"] li')
    expect(shownConfig(mounted).filters[0]).toMatchObject({ condition: 'between' })
    expect(mounted.findComponent(PivotFilterPanel).exists()).toBe(true)
    expect(mounted.find('section[aria-label="Filtres"] [data-element]').text()).toContain(
      'region BETWEEN ? AND ?',
    )
  })

  it('opens the settings of a measure dropped into the filters', async () => {
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
    expect(shownConfig(mounted).filters).toMatchObject([
      { kind: 'measure', aggregate: 'sum', column: 2, condition: 'between', operand: '' },
    ])
    expect(mounted.findComponent(PivotFilterPanel).exists()).toBe(true)
  })

  it('gives a measure the choice of a filter on it, whose settings open', async () => {
    const mounted = mountZones()
    const groups = () =>
      mounted
        .findAllComponents(UiDropdownMenu)
        .flatMap((menu) => (menu.props('groups') ?? []) as MenuGroup[])
    groups()
      .filter((group) => group.label === 'Élément')
      .flatMap((group) => group.items)
      .find((item) => item.label === 'Filtrer sur cette mesure')
      ?.onSelect?.()
    await nextTick()
    expect(shownConfig(mounted).filters).toMatchObject([
      { kind: 'measure', aggregate: 'sum', column: 2 },
    ])
    // A filter has no menu: its settings hold everything it sets.
    expect(groups().map((group) => group.label)).toEqual(['Élément', 'Agrégat', 'Élément'])
  })

  it('opens and closes the settings of a filter with its button', async () => {
    const mounted = mountZones()
    await mounted.setProps({ config: { ...CONFIG, filters: [onMeasure(), onMeasure()] } })
    const buttons = mounted.findAll('button[aria-label^="Réglages"]')
    expect(buttons).toHaveLength(2)
    await buttons[1]!.trigger('click')
    await nextTick()
    await nextTick()
    expect(mounted.findComponent(PivotFilterPanel).props('index')).toBe(1)
    expect(buttons[1]!.attributes('aria-expanded')).toBe('true')
    await buttons[1]!.trigger('click')
    await nextTick()
    expect(mounted.findComponent(PivotFilterPanel).exists()).toBe(false)
  })

  it('names a filter from its settings, or with a double click', async () => {
    const mounted = mountZones()
    await mounted.setProps({ config: { ...CONFIG, filters: [onMeasure()] } })
    const zone = () => mounted.find('section[aria-label="Filtres"]')
    await zone().find('button[aria-label^="Réglages"]').trigger('click')
    await nextTick()
    await nextTick()
    mounted.findComponent(PivotFilterPanel).vm.$emit('name')
    await nextTick()
    await nextTick()
    expect(mounted.findComponent(PivotFilterPanel).exists()).toBe(false)
    const field = zone().find('input')
    expect(document.activeElement).toBe(field.element)
    await field.setValue('Gros')
    await field.trigger('keydown', { key: 'Enter' })
    expect(shownConfig(mounted).filters[0]).toMatchObject({ name: 'Gros' })
    await zone().find('[data-element] span.truncate').trigger('dblclick')
    await nextTick()
    expect(zone().find('input').exists()).toBe(true)
  })

  it('opens the settings of the filter just dropped, among filters on the same column', async () => {
    const mounted = mountZones()
    await dropOnFilters(mounted, 'ul[aria-label="Colonnes du résultat"] li')
    const panel = () => mounted.findComponent(PivotFilterPanel)
    const change = (values: string[]) =>
      panel().vm.$emit('change', { ...panel().props('filter'), values })
    change(['Nord'])
    panel().vm.$emit('close')
    await nextTick()
    await dropOnFilters(mounted, 'ul[aria-label="Colonnes du résultat"] li')
    change(['Sud'])
    await nextTick()
    expect(shownConfig(mounted).filters).toMatchObject([{ values: ['Nord'] }, { values: ['Sud'] }])
  })

  it('reads the values of a list at the precision of its filter', async () => {
    const readValues = vi.fn<() => Promise<Cell[]>>(() => Promise.resolve(['2026-01']))
    const mounted = mountZones()
    const filter: PivotFilter = {
      kind: 'column',
      column: 2,
      name: '',
      precision: 'month',
      condition: 'in_list',
      values: [],
      operand: '',
      upper: '',
    }
    await mounted.setProps({ readValues, config: { ...CONFIG, filters: [filter] } })
    await mounted.find('button[aria-label^="Réglages"]').trigger('click')
    await nextTick()
    await nextTick()
    expect(readValues).toHaveBeenCalledWith(2, 'month')
  })

  it('marks a filter whose value no longer compares with what it reads', async () => {
    const mounted = mountZones()
    const element = () => mounted.find('section[aria-label="Filtres"] [data-element]')
    await mounted.setProps({
      config: {
        ...CONFIG,
        filters: [onMeasure({ aggregate: 'count', condition: 'equal', operand: 'abc' })],
      },
    })
    expect(element().classes()).toContain('invalid')
    await mounted.setProps({
      config: {
        ...CONFIG,
        filters: [
          onMeasure({ aggregate: 'count', condition: 'between', operand: '1', upper: 'abc' }),
        ],
      },
    })
    expect(element().classes()).toContain('invalid')
    await mounted.setProps({
      config: {
        ...CONFIG,
        filters: [onMeasure({ aggregate: 'count', condition: 'is_null', operand: 'abc' })],
      },
    })
    expect(element().classes()).not.toContain('invalid')
  })

  it('ignores a column of the list released outside the zones', async () => {
    const mounted = mountZones()
    await mounted.find('[draggable="true"]').trigger('dragstart')
    dropOnPage()
    await nextTick()
    expect(shownConfig(mounted)).toEqual(CONFIG)
  })
})
