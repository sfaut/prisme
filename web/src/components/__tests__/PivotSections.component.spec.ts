import { ArrowDownToLine, CalendarClock, CalendarDays } from '@lucide/vue'
import { afterEach, describe, expect, it, vi } from 'vitest'

import { type VueWrapper, mount } from '@vue/test-utils'
import { nextTick } from 'vue'

import type { Cell } from '@/api/querySession'

import PivotConcatPopover from '../PivotConcatPopover.vue'
import PivotFilterPopover from '../PivotFilterPopover.vue'
import PivotSections from '../PivotSections.vue'
import SplitHandle from '../split/SplitHandle.vue'
import SplitPanel from '../split/SplitPanel.vue'
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
 *
 * @param config Configuration shown at first.
 */
function mountZones(config: PivotConfig = CONFIG) {
  const mounted = mount(PivotSections, {
    attachTo: document.body,
    props: {
      names: NAMES,
      types: [],
      marks: [],
      foreignKeys: [],
      readValues: () => Promise.resolve([null, 'Nord', 'Sud']),
      config,
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

/**
 * Menu of an element of the zones.
 *
 * @param mounted Mounted zones.
 * @param element Position of the element among those of the zones, in their order on screen.
 */
function elementMenu(mounted: VueWrapper, element: number) {
  return mounted.findAllComponents(UiDropdownMenu).filter((item) => item.props('groups'))[element]!
}

/**
 * Labels of the group "Élément" of the menu of an element.
 *
 * @param mounted Mounted zones.
 * @param element Position of the element among those of the zones, in their order on screen.
 */
function elementEntries(mounted: VueWrapper, element: number): string[] {
  const groups = elementMenu(mounted, element).props('groups') ?? []
  return groups.find((group) => group.label === 'Élément')!.items.map((item) => item.label)
}

/**
 * Chooses an entry of the menu of an element, then closes the menu, as a click on the entry does.
 *
 * @param mounted Mounted zones.
 * @param element Position of the element among those of the zones, in their order on screen.
 * @param label Label of the entry.
 */
async function choose(mounted: VueWrapper, element: number, label: string): Promise<void> {
  const menu = elementMenu(mounted, element)
  const groups = menu.props('groups') ?? []
  groups.flatMap((group) => group.items).find((item) => item.label === label)!.onSelect!()
  menu.vm.$emit('closeAutoFocus', new Event('focus', { cancelable: true }))
  await nextTick()
  await nextTick()
}

describe('PivotSections', () => {
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

  it('removes an element by its menu, in each zone', async () => {
    const mounted = mountZones()
    const filter: PivotFilter = {
      kind: 'column',
      column: 0,
      name: '',
      precision: 'none',
      condition: 'is_null',
      values: [],
      operand: '',
      upper: '',
    }
    await mounted.setProps({ config: { ...CONFIG, columns: CONFIG.rows, filters: [filter] } })
    expect(elementEntries(mounted, 0)).toEqual(['Nommer…', 'Passer en colonnes', 'Retirer'])
    expect(elementEntries(mounted, 2)).toEqual(['Nommer…', 'Filtrer sur cette mesure', 'Retirer'])
    expect(elementEntries(mounted, 3)).toEqual(['Réglages…', 'Nommer…', 'Retirer'])
    await choose(mounted, 1, 'Retirer')
    expect(shownConfig(mounted).columns).toEqual([])
    expect(shownConfig(mounted).rows).toEqual(CONFIG.rows)
    for (const element of [2, 1, 0]) {
      await choose(mounted, element, 'Retirer')
    }
    expect(shownConfig(mounted)).toEqual({ rows: [], columns: [], measures: [], filters: [] })
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
      'Horodatage',
      'Horodatage (hh)',
      'Horodatage (hh:mm)',
      'Horodatage (hh:mm:ss)',
      'Horodatage (hh:mm:ss.ms)',
      'Horodatage (hh:mm:ss.µs)',
    ])
    expect(precision()?.items.map((item) => item.checked)).toEqual([
      false,
      false,
      false,
      false,
      false,
      true,
      false,
      false,
      false,
      false,
      false,
    ])
    // The choice made keeps its icon.
    expect(precision()?.items[5]?.icon).toBe(CalendarClock)
    precision()?.items[2]?.onSelect?.()
    await nextTick()
    expect(shownConfig(mounted).rows).toEqual([{ column: 0, name: '', precision: 'month' }])
    expect(precision()?.items[2]).toMatchObject({ checked: true, icon: CalendarDays })
    expect(precision()?.items[5]?.checked).toBe(false)
    expect(mounted.find('[data-element]').text()).toContain('region (Mois)')
    precision()?.items[7]?.onSelect?.()
    await nextTick()
    expect(shownConfig(mounted).rows).toEqual([{ column: 0, name: '', precision: 'minute' }])
    expect(mounted.find('[data-element]').text()).toContain('region (Horodatage (min))')
  })

  it('gives COUNT(DISTINCT), MIN and MAX of dates the precisions of their menu', async () => {
    const mounted = mountZones()
    await mounted.setProps({
      types: [
        { family: 'text', name: 'TEXT' },
        { family: 'text', name: 'TEXT' },
        { family: 'date', name: 'DATE' },
      ],
      config: { ...CONFIG, measures: [{ aggregate: 'min', column: 2, name: '' }] },
    })
    const groups = () => elementMenu(mounted, 1).props('groups') as MenuGroup[]
    const precision = () => groups().find((group) => group.label === 'Précision')
    expect(precision()?.items.map((item) => item.label)).toEqual([
      'Année',
      'Trimestre',
      'Mois',
      'Semaine',
      'Date',
    ])
    expect(precision()?.items.map((item) => item.checked)).toEqual([
      false,
      false,
      false,
      false,
      true,
    ])
    precision()?.items[2]?.onSelect?.()
    await nextTick()
    expect(shownConfig(mounted).measures).toEqual([
      { aggregate: 'min', column: 2, name: '', precision: 'month' },
    ])
    expect(mounted.findAll('[data-element]')[1]!.text()).toContain('MIN(montant (Mois))')
    // The aggregates show the precision they would keep.
    const aggregates = groups().find((group) => group.label === 'Agrégat')!.items
    expect(aggregates.map((item) => item.label)).toContain('MAX(montant (Mois))')
    expect(aggregates.map((item) => item.label)).toContain('COUNT(montant)')
    // The aggregate chosen is checked and keeps its icon.
    expect(aggregates.filter((item) => item.checked)).toEqual([
      expect.objectContaining({ label: 'MIN(montant (Mois))', icon: ArrowDownToLine }),
    ])
    expect(aggregates.every((item) => item.checked !== undefined)).toBe(true)
    await choose(mounted, 1, 'COUNT(montant)')
    expect(shownConfig(mounted).measures[0]?.precision).toBeUndefined()
    expect(groups().map((group) => group.label)).toEqual(['Agrégat', 'Affichage', 'Élément'])
  })

  it('gives no precision to a level without dates, nor to a measure that reads none', async () => {
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
    expect(labels).toEqual(['Élément', 'Agrégat', 'Affichage', 'Élément'])
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
    const panel = mounted.findComponent(PivotFilterPopover)
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
    expect(mounted.findComponent(PivotFilterPopover).exists()).toBe(false)
  })

  it('starts a column of numbers or of dates on its bounds, with its panel', async () => {
    const mounted = mountZones()
    await mounted.setProps({ types: [{ family: 'date', name: 'DATE' }] })
    await dropOnFilters(mounted, 'ul[aria-label="Colonnes du résultat"] li')
    expect(shownConfig(mounted).filters[0]).toMatchObject({ condition: 'between' })
    expect(mounted.findComponent(PivotFilterPopover).exists()).toBe(true)
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
    expect(mounted.findComponent(PivotFilterPopover).exists()).toBe(true)
  })

  it('gives a measure the choice of a filter on it, whose settings open', async () => {
    const mounted = mountZones()
    await choose(mounted, 1, 'Filtrer sur cette mesure')
    expect(shownConfig(mounted).filters).toMatchObject([
      { kind: 'measure', aggregate: 'sum', column: 2 },
    ])
    expect(mounted.findComponent(PivotFilterPopover).props('index')).toBe(0)
  })

  it('opens the settings of a filter by its menu', async () => {
    const mounted = mountZones()
    await mounted.setProps({ config: { ...CONFIG, filters: [onMeasure(), onMeasure()] } })
    await choose(mounted, 3, 'Réglages…')
    expect(mounted.findComponent(PivotFilterPopover).props('index')).toBe(1)
  })

  it('names a filter from its settings, or with a double click', async () => {
    const mounted = mountZones()
    await mounted.setProps({ config: { ...CONFIG, filters: [onMeasure()] } })
    const zone = () => mounted.find('section[aria-label="Filtres"]')
    await choose(mounted, 2, 'Réglages…')
    mounted.findComponent(PivotFilterPopover).vm.$emit('name')
    await nextTick()
    await nextTick()
    expect(mounted.findComponent(PivotFilterPopover).exists()).toBe(false)
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
    const panel = () => mounted.findComponent(PivotFilterPopover)
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
    await choose(mounted, 2, 'Réglages…')
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

  it('opens the settings of a measure that becomes a GROUP_CONCAT, which gives no filter', async () => {
    const mounted = mountZones()
    await choose(mounted, 1, 'GROUP_CONCAT(montant)')
    expect(shownConfig(mounted).measures).toEqual([
      { aggregate: 'group_concat', column: 2, name: '', concat: { separator: ', ' } },
    ])
    const panel = () => mounted.findComponent(PivotConcatPopover)
    expect(panel().props('index')).toBe(0)
    panel().vm.$emit('change', { separator: ' | ', distinct: true })
    await nextTick()
    expect(shownConfig(mounted).measures[0]?.concat).toEqual({ separator: ' | ', distinct: true })
    panel().vm.$emit('close')
    await nextTick()
    expect(panel().exists()).toBe(false)
    // Its menu opens them again, and offers no filter on it.
    expect(elementEntries(mounted, 1)).toEqual(['Réglages…', 'Nommer…', 'Retirer'])
    await choose(mounted, 1, 'Réglages…')
    expect(panel().props('index')).toBe(0)
    // A measure that already is one opens nothing when it takes it again.
    panel().vm.$emit('close')
    await choose(mounted, 1, 'GROUP_CONCAT(montant)')
    expect(panel().exists()).toBe(false)
  })

  it('shows a sum as a share of a total, with its digits', async () => {
    const mounted = mountZones()
    const group = (label: string) =>
      (elementMenu(mounted, 1).props('groups') as MenuGroup[]).find((item) => item.label === label)
    const show = () => group('Affichage')?.items.map((item) => [item.label, item.checked])
    expect(show()).toEqual([
      ['Valeur', true],
      ['Part du total de la ligne', false],
      ['Part du total de la colonne', false],
    ])
    expect(group('Décimales')).toBeUndefined()
    await choose(mounted, 1, 'Part du total de la ligne')
    expect(shownConfig(mounted).measures[0]?.share).toEqual({ of: 'row', decimals: 0 })
    expect(mounted.findAll('[data-element]')[1]!.text()).toContain('SUM(montant) (% ligne)')
    expect(group('Décimales')?.items.map((item) => item.label)).toEqual([
      '0',
      '1',
      '2',
      '3',
      '4',
      '5',
      '6',
    ])
    await choose(mounted, 1, '2')
    expect(shownConfig(mounted).measures[0]?.share).toEqual({ of: 'row', decimals: 2 })
    await choose(mounted, 1, 'Valeur')
    expect(shownConfig(mounted).measures[0]).toEqual(CONFIG.measures[0])
  })

  it('offers no share to MIN, MAX or GROUP_CONCAT', async () => {
    const mounted = mountZones({ ...CONFIG, measures: [{ aggregate: 'max', column: 2, name: '' }] })
    const labels = (elementMenu(mounted, 1).props('groups') as MenuGroup[]).map(
      (item) => item.label,
    )
    expect(labels).not.toContain('Affichage')
  })

  it('refuses a GROUP_CONCAT dropped into the filters', async () => {
    const concat = { separator: ', ' }
    const mounted = mountZones({
      ...CONFIG,
      measures: [{ aggregate: 'group_concat', column: 1, name: '', concat }],
    })
    await mounted.find('section[aria-label="Mesures"] [data-element]').trigger('dragstart')
    const filters = mounted.find('section[aria-label="Filtres"]')
    expect(accepted(filters.element, 'dragover')).toBe(false)
    expect(accepted(mounted.find('section[aria-label="Lignes"]').element, 'dragover')).toBe(true)
    await filters.trigger('drop', { clientY: 0 })
    await nextTick()
    expect(shownConfig(mounted).filters).toEqual([])
    expect(shownConfig(mounted).measures).toHaveLength(1)
    expect(mounted.findComponent(PivotFilterPopover).exists()).toBe(false)
  })
})

describe('PivotSections, the widths of its sections', () => {
  /**
   * Width of the page laid out, in pixels.
   */
  const PAGE_WIDTH = 800

  /**
   * Lengths of the theme in pixels, as the group of the sections measures them.
   */
  const LENGTHS: Record<string, number> = {
    'var(--spacing)': 3,
    'var(--pivot-fields-min-width)': 126,
    'var(--pivot-zones-min-width)': 156,
  }

  /**
   * Gives the elements a width, as in a page laid out on screen: the sections theirs, the measures
   * of the group those of the theme, the others that of the page.
   *
   * @param fields Width of the section of the columns.
   * @param zones Width of the section of the zones.
   */
  function layOutPage(fields: number, zones: number): void {
    vi.spyOn(Element.prototype, 'getBoundingClientRect').mockImplementation(function (
      this: Element,
    ) {
      const element = this as HTMLElement
      const width = element.classList.contains('split-probe')
        ? (LENGTHS[element.style.inlineSize] ?? 0)
        : element.id.endsWith('-fields')
          ? fields
          : element.id.endsWith('-zones')
            ? zones
            : PAGE_WIDTH
      return new DOMRect(0, 0, width, 300)
    })
  }

  afterEach(() => {
    vi.restoreAllMocks()
  })

  it('shows the widths of the configuration, those saved in steps in pixels', () => {
    const mounted = mountZones({ ...CONFIG, fields_px: 210.4, zones_width: 80 })
    const panels = mounted.findAllComponents(SplitPanel)
    expect(panels.map((panel) => panel.props('size'))).toEqual([210.4, 240, null])
  })

  it('gives a section its default width back on a double click or Enter on its line', async () => {
    const mounted = mountZones({ ...CONFIG, fields_px: 210, zones_width: 80 })
    const handles = mounted.findAllComponents(SplitHandle)
    await handles[0]!.trigger('dblclick')
    expect(shownConfig(mounted).fields_px).toBeUndefined()
    expect(shownConfig(mounted).zones_width).toBe(80)
    await handles[1]!.trigger('keydown', { key: 'Enter' })
    expect(shownConfig(mounted).zones_width).toBeUndefined()
    expect(shownConfig(mounted).zones_px).toBeUndefined()
    expect(mounted.findAllComponents(SplitPanel)[0]!.props('size')).toBeNull()
  })

  it('keeps the width given by a key on the line, in pixels, in place of one in steps', async () => {
    layOutPage(210.5, 240)
    const mounted = mountZones({ ...CONFIG, fields_px: 210.5, zones_width: 80 })
    const handles = mounted.findAllComponents(SplitHandle)
    // A key moves the line by four steps.
    await handles[0]!.trigger('keydown', { key: 'ArrowRight' })
    expect(shownConfig(mounted).fields_px).toBe(222.5)
    await handles[1]!.trigger('keydown', { key: 'ArrowLeft' })
    expect(shownConfig(mounted).zones_px).toBe(228)
    expect(shownConfig(mounted).zones_width).toBeUndefined()
    // Home: the smallest width.
    await handles[1]!.trigger('keydown', { key: 'Home' })
    expect(shownConfig(mounted).zones_px).toBe(156)
  })
})
