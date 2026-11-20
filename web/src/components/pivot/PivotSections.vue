<!--
  Settings of the Pivoteur (`PivotPanel.vue`), in two sections beside the
  table, which the panel gives in the default slot: the columns of the
  result, with the mark of their type and a search by name above them
  (F-10), then the zones Lignes, Colonnes, Mesures and Filtres, stacked,
  which scroll in their section when the panel is low (F-05).

  The sections stand in a splitter of Prisme (`split/`): the line on the
  right of a section sets its width, the table taking the rest; a double
  click on the line, or Enter, gives back its default width (`main.css`).
  The width is kept in the configuration of the tab, in CSS pixels as the
  line left it, at the end of a drag or of a key on the line, and only
  then: a section that the window makes narrower keeps its width for the
  next tab shown. The browser lays the sections out, shown or hidden under
  a folded Pivoteur alike.

  A column enters a zone by dragging it there, or by the "+" menu of its
  line (F-11); the order in a zone sets the nesting, the first element
  being the outermost level (F-27). An element moves by dragging it, from
  one zone to another too, a level to the other axis by its menu too. It
  leaves its zone by its menu ("Retirer"), or released outside the zones
  (Escape cancels the drag).
  A column dropped into the measures gives its default aggregate
  (`defaultAggregate`, F-15); the menu of a measure changes it. The menu of a
  level of dates sets its precision, what it keeps of them (`precisionChoices`),
  and so does that of `COUNT(DISTINCT)`, `MIN`, `MAX` and `GROUP_CONCAT` of dates
  (`takesPrecision`). The menu of a `GROUP_CONCAT` holds its settings (group
  "Concaténation"), that of a count, a sum or an average its display as a share of
  a total (group "Affichage", `pivotMenus.ts`). An aggregate or a share that brings
  new settings keeps the menu open, which shows them at once.
  A column can be placed several times, in a zone and on both axes: dropping it
  from the list always adds an element, dragging an element moves it.
  An element takes a name by its menu ("Nommer…") or a double click on its label
  (F-17): Enter confirms, Escape cancels, an empty name gives back its
  expression, which the tooltip of a named element recalls.
  An element is hidden, and shown again, by its eye or by its menu
  ("Masquer", "Afficher"): greyed, it keeps its place and its settings, and the
  table is computed without it, a hidden filter having no effect.

  A column or a measure dropped into the filters, or a measure by its menu
  ("Filtrer sur cette mesure"), gives a filter, whose settings open beside it
  (`PivotFilterPopover.vue`, F-21, F-44); its menu opens them again ("Réglages…").
  A `GROUP_CONCAT` gives no filter (`filterable`): the filters refuse its drop.
  The logic of the zones is in `pivotConfig.ts`.
-->
<script setup lang="ts">
import {
  ArrowLeftRight,
  Columns3,
  EllipsisVertical,
  Eye,
  EyeOff,
  Funnel,
  PenLine,
  Plus,
  Rows3,
  Search,
  Sigma,
  SlidersHorizontal,
  X,
} from '@lucide/vue'
import {
  type Component,
  computed,
  nextTick,
  onBeforeUnmount,
  ref,
  shallowRef,
  useId,
  useTemplateRef,
  type WritableComputedRef,
} from 'vue'

import type { ColumnTypeInfo } from '@/api/generated/ColumnTypeInfo'
import type { Cell } from '@/api/querySession'
import { cssPixels } from '@/utils/cssLength'

import { AGGREGATES, PRECISION_ICONS } from './pivotChoices'
import {
  type DatePrecision,
  type PivotConfig,
  type PivotFilter,
  type PivotSection,
  type PivotZone,
  type ZonePlace,
  addsNumbers,
  aggregateLabel,
  concatOf,
  defaultAggregate,
  elementExpression,
  elementLabel,
  filterable,
  isHidden,
  isRefused,
  measurePrecision,
  placeColumn,
  precisionChoices,
  removeElement,
  shareOf,
  sectionWidth,
  summable,
  takesPrecision,
  takesShare,
  withAggregate,
  withConcat,
  withFilter,
  withHidden,
  withName,
  withPrecision,
  withShare,
  withSectionWidth,
} from './pivotConfig'
import type { PivotMeasure } from './pivotEngine'
import PivotFilterPopover from './PivotFilterPopover.vue'
import { concatGroup, shareGroup } from './pivotMenus'
import { dropPosition } from '@/components/result/resultGrid'
import SplitGroup from '@/components/split/SplitGroup.vue'
import SplitHandle from '@/components/split/SplitHandle.vue'
import SplitPanel from '@/components/split/SplitPanel.vue'
import type { TypeMark } from '@/components/result/typeIcons'
import type { MenuGroup, MenuItem } from '@/components/ui/menuItem'
import UiButton from '@/components/ui/UiButton.vue'
import UiDropdownMenu from '@/components/ui/UiDropdownMenu.vue'
import UiInput from '@/components/ui/UiInput.vue'
import { vTooltip } from '@/components/ui/tooltip'

const props = defineProps<{
  /**
   * Names of the columns of the result.
   */
  names: string[]
  /**
   * Type of each column; none for a result saved before its columns were
   * typed.
   */
  types: ColumnTypeInfo[]
  /**
   * Mark of each column (`columnIcon`), absent without a type.
   */
  marks: (TypeMark | undefined)[]
  /**
   * Is each column part of a foreign key of its table?
   */
  foreignKeys: boolean[]
  /**
   * Reads the distinct values of a column, or the periods of a precision of its dates, for the
   * list of a filter.
   */
  readValues: (column: number, precision: DatePrecision) => Promise<Cell[]>
  /**
   * Are the rows of the result still being received? The list of a filter then offers the values
   * of the rows received so far.
   */
  receiving?: boolean
}>()

/**
 * Configuration of the tab shown, replaced as a whole on each change.
 */
const config = defineModel<PivotConfig>('config', { required: true })

/**
 * Zones, in their order on screen, with their label and their icon.
 */
const ZONES: { zone: PivotZone; label: string; icon: Component }[] = [
  { zone: 'rows', label: 'Lignes', icon: Rows3 },
  { zone: 'columns', label: 'Colonnes', icon: Columns3 },
  { zone: 'measures', label: 'Mesures', icon: Sigma },
  { zone: 'filters', label: 'Filtres', icon: Funnel },
]

/**
 * Width of a step of the spacing scale, in pixels (`main.css`): that of the widths saved in steps,
 * before the widths in pixels.
 */
const STEP = cssPixels('--spacing', 3)

/**
 * Prefix of the ids of the sections, unique to the page: their lines name them.
 */
const sectionsId = useId()

/**
 * Width of each section, in CSS pixels, read from and written to the configuration; `null` for its
 * default width.
 */
const widths: Record<PivotSection, WritableComputedRef<number | null>> = {
  fields: sectionModel('fields'),
  zones: sectionModel('zones'),
}

/**
 * Width of a section, as a model of its panel.
 *
 * @param section Section.
 */
function sectionModel(section: PivotSection): WritableComputedRef<number | null> {
  return computed({
    get: () => sectionWidth(config.value, section, STEP),
    set: (width) => {
      config.value = withSectionWidth(config.value, section, width)
    },
  })
}

/**
 * Gives a section its default width back.
 *
 * @param section Section.
 */
function resetWidth(section: PivotSection): void {
  widths[section].value = null
}

/**
 * Searched text, in the names of the columns.
 */
const search = ref('')

/**
 * Columns whose name holds the searched text, in their order.
 */
const shownFields = computed(() => {
  const needle = search.value.trim().toLocaleLowerCase('fr-FR')
  return props.names
    .map((_, column) => column)
    .filter((column) => props.names[column]!.toLocaleLowerCase('fr-FR').includes(needle))
})

/**
 * Element of a zone, as shown.
 */
interface ZoneElement {
  /**
   * Column of the element.
   */
  column: number
  /**
   * Label: its name, or its expression without one.
   */
  label: string
  /**
   * Expression: the name of the column of a level, the expression of a
   * measure, of a filter.
   */
  expression: string
  /**
   * Is the element in error (F-22): a filter whose value cannot compare with what it reads, after
   * a change of its aggregate or of the type of its column.
   */
  invalid: boolean
  /**
   * Is the element hidden? Greyed, it stays in its zone, and the table is computed without it.
   */
  hidden: boolean
}

/**
 * Elements of each zone.
 */
const elements = computed<Record<PivotZone, ZoneElement[]>>(() => {
  const shown = (zone: PivotZone) =>
    config.value[zone].map((element, index) => ({
      column: element.column,
      label: elementLabel(config.value, { zone, index }, props.names, props.types),
      expression: elementExpression(config.value, { zone, index }, props.names, props.types),
      invalid: 'condition' in element && isRefused(element, props.types),
      hidden: isHidden(element),
    }))
  return {
    rows: shown('rows'),
    columns: shown('columns'),
    measures: shown('measures'),
    filters: shown('filters'),
  }
})

/**
 * Places a column in a zone, a new element or a moved one.
 *
 * @param column Index of the column.
 * @param to Zone and position.
 * @param from Place of the moved element, `null` for a new one.
 */
function place(column: number, to: ZonePlace, from: ZonePlace | null): void {
  const aggregate = defaultAggregate(props.types[column], props.foreignKeys[column] ?? false)
  const placed = placeColumn(config.value, column, aggregate, to, from, props.types)
  if (placed === config.value) {
    return
  }
  config.value = placed
  if (to.zone === 'filters' && from?.zone !== 'filters') {
    pendingPanel = { zone: 'filters', index: Math.min(to.index, placed.filters.length - 1) }
  }
}

/**
 * Choices of the "+" menu of a column: the zone it enters, last.
 *
 * @param column Index of the column.
 */
function addItems(column: number): MenuItem[] {
  return ZONES.map(({ zone, label, icon }) => ({
    label: `Ajouter aux ${label.toLocaleLowerCase('fr-FR')}`,
    icon,
    onSelect: () => place(column, { zone, index: elements.value[zone].length }, null),
  }))
}

/**
 * Choices that set, name, move, hide or remove an element, the settings of a filter first.
 *
 * @param zone Zone of the element.
 * @param index Position of the element.
 */
function elementItems(zone: PivotZone, index: number): MenuItem[] {
  const column = elements.value[zone][index]!.column
  const from = { zone, index }
  const items: MenuItem[] = []
  const measure = zone === 'measures' ? config.value.measures[index] : undefined
  if (zone === 'filters') {
    items.push({
      label: 'Réglages…',
      icon: SlidersHorizontal,
      // They open beside the element once the menu has closed (`onMenuClosed`).
      onSelect: () => (pendingPanel = from),
    })
  }
  items.push({
    label: 'Nommer…',
    icon: PenLine,
    // The field takes the focus once the menu has closed (`onMenuClosed`).
    onSelect: () => startNaming(from),
  })
  if (measure !== undefined && filterable(measure.aggregate)) {
    items.push({
      label: 'Filtrer sur cette mesure',
      icon: Funnel,
      onSelect: () =>
        place(column, { zone: 'filters', index: elements.value.filters.length }, from),
    })
  }
  if (zone === 'rows' || zone === 'columns') {
    const other = zone === 'rows' ? 'columns' : 'rows'
    items.push({
      label: other === 'rows' ? 'Passer en lignes' : 'Passer en colonnes',
      icon: ArrowLeftRight,
      onSelect: () => place(column, { zone: other, index: elements.value[other].length }, from),
    })
  }
  const hidden = isHidden(config.value[zone][index])
  items.push({
    label: hidden ? 'Afficher' : 'Masquer',
    icon: hidden ? Eye : EyeOff,
    onSelect: () => toggleHidden(from),
  })
  items.push({
    label: 'Retirer',
    icon: X,
    onSelect: () => (config.value = removeElement(config.value, from)),
  })
  return items
}

/**
 * Hides an element, or shows it again.
 *
 * @param place Place of the element.
 */
function toggleHidden(place: ZonePlace): void {
  const hidden = isHidden(config.value[place.zone][place.index])
  config.value = withHidden(config.value, place, !hidden)
}

/**
 * Choices of the precision of a level, or of a measure that takes one (`takesPrecision`), the
 * current one checked; none for a column that holds no dates.
 *
 * @param zone Zone of the element.
 * @param index Position of the element.
 */
function precisionItems(zone: 'rows' | 'columns' | 'measures', index: number): MenuItem[] {
  const element = zone === 'measures' ? config.value.measures[index] : config.value[zone][index]
  if (element === undefined || ('aggregate' in element && !takesPrecision(element.aggregate))) {
    return []
  }
  const current = 'aggregate' in element ? measurePrecision(element) : element.precision
  return precisionChoices(props.types[element.column]?.family).map(({ precision, label }) => ({
    label,
    icon: PRECISION_ICONS[label],
    checked: precision === current,
    onSelect: () => (config.value = withPrecision(config.value, { zone, index }, precision)),
  }))
}

/**
 * Choices of the aggregate of a measure, the current one checked.
 *
 * @param measure Measure.
 * @param choose Takes the aggregate chosen.
 */
function aggregateItems(
  measure: PivotMeasure,
  choose: (aggregate: PivotMeasure['aggregate']) => void,
): MenuItem[] {
  const family = props.types[measure.column]?.family
  const numbers = summable(family)
  const current = settingsOf(measure.aggregate, family)
  return AGGREGATES.map(({ aggregate, icon }) => {
    const refused = addsNumbers(aggregate) && !numbers
    const { column, precision } = measure
    return {
      label: aggregateLabel({ aggregate, column, precision }, props.names),
      icon,
      checked: aggregate === measure.aggregate,
      disabled: refused,
      hint: refused ? 'Demande une colonne de nombres' : undefined,
      keepOpen: settingsOf(aggregate, family).some((group) => !current.includes(group)),
      onSelect: () => choose(aggregate),
    }
  })
}

/**
 * Groups of settings the menu of a measure shows for an aggregate, besides the aggregate and the
 * element: an aggregate that brings new ones keeps the menu open.
 *
 * @param aggregate Aggregate.
 * @param family Family of the type of the column of the measure.
 */
function settingsOf(
  aggregate: PivotMeasure['aggregate'],
  family: ColumnTypeInfo['family'] | undefined,
): string[] {
  return [
    takesPrecision(aggregate) && precisionChoices(family).length > 0 ? 'Précision' : '',
    aggregate === 'group_concat' ? 'Concaténation' : '',
    takesShare(aggregate) ? 'Affichage' : '',
  ].filter((group) => group !== '')
}

/**
 * Choices of the menu of an element: for a measure, its aggregate, for a
 * level or a measure of dates, its precision, for a `GROUP_CONCAT`, its
 * settings, for a count, a sum or an average, its display as a share of a
 * total, then the settings of a filter, its name, its place and its removal.
 *
 * @param zone Zone of the element.
 * @param index Position of the element.
 */
function elementGroups(zone: PivotZone, index: number): MenuGroup[] {
  const groups: MenuGroup[] = []
  const measure = zone === 'measures' ? config.value.measures[index] : undefined
  if (measure !== undefined) {
    groups.push({
      label: 'Agrégat',
      items: aggregateItems(measure, (aggregate) => {
        config.value = withAggregate(config.value, index, aggregate)
      }),
    })
  }
  if (zone !== 'filters') {
    const items = precisionItems(zone, index)
    if (items.length > 0) {
      groups.push({ label: 'Précision', items })
    }
  }
  if (measure?.aggregate === 'group_concat') {
    groups.push(
      concatGroup(concatOf(measure), measure.column, props.names, (changes) => {
        const settings = concatOf(config.value.measures[index] ?? measure)
        config.value = withConcat(config.value, index, { ...settings, ...changes })
      }),
    )
  }
  if (measure !== undefined && takesShare(measure.aggregate)) {
    groups.push(
      shareGroup(shareOf(measure), (share) => {
        config.value = withShare(config.value, index, share)
      }),
    )
  }
  groups.push({ label: 'Élément', items: elementItems(zone, index) })
  return groups
}

/**
 * Element being named, `null` without one.
 */
const naming = ref<ZonePlace | null>(null)

/**
 * Name being typed.
 */
const draft = ref('')

/**
 * Field of the name, the only one shown (`v-for`).
 */
const nameInputs = useTemplateRef<HTMLInputElement[]>('nameInput')

/**
 * Is this element being named?
 *
 * @param zone Zone of the element.
 * @param index Position of the element.
 */
function isNaming(zone: PivotZone, index: number): boolean {
  return naming.value?.zone === zone && naming.value.index === index
}

/**
 * Makes the label of an element editable, with its current label.
 *
 * @param place Place of the element.
 */
function startNaming(place: ZonePlace): void {
  draft.value = elements.value[place.zone][place.index]?.label ?? ''
  naming.value = place
}

/**
 * Filter whose settings are to open, `null` without one.
 */
let pendingPanel: ZonePlace | null = null

/**
 * Filter whose settings are open, with the element they open beside.
 */
const panel = shallowRef<(ZonePlace & { anchor: HTMLElement }) | null>(null)

/**
 * Filter whose settings are open.
 */
const panelFilter = computed(() =>
  panel.value?.zone === 'filters' ? config.value.filters[panel.value.index] : undefined,
)

/**
 * Changes the filter whose settings are open.
 *
 * @param filter New filter.
 */
function changePanelFilter(filter: PivotFilter): void {
  if (panel.value !== null) {
    config.value = withFilter(config.value, panel.value.index, filter)
  }
}

/**
 * Shows what the last action opens, once its element is on the page: the field being named, or the
 * settings of a filter.
 */
async function showPending(): Promise<void> {
  await nextTick()
  if (naming.value !== null) {
    focusName()
    return
  }
  const pending = pendingPanel
  pendingPanel = null
  if (pending === null) {
    return
  }
  const section = zoneSections.value?.find((zone) => zone.dataset.zone === pending.zone)
  const anchor = section?.querySelectorAll('[data-element]')[pending.index]
  if (anchor instanceof HTMLElement) {
    // The zones scroll in their column: the panel opens beside an element in sight.
    anchor.scrollIntoView({ block: 'nearest' })
    panel.value = { ...pending, anchor }
  }
}

/**
 * Focuses the field of the name, its text selected.
 */
function focusName(): void {
  const input = nameInputs.value?.[0]
  input?.focus()
  input?.select()
}

/**
 * Double click on the label of an element, or the choice of the name in the
 * settings of a filter: names it at once.
 *
 * @param place Place of the element.
 */
async function nameAtOnce(place: ZonePlace): Promise<void> {
  panel.value = null
  startNaming(place)
  await nextTick()
  focusName()
}

/**
 * Closing of a menu: when a choice opens a field or settings, it takes the focus,
 * which the menu would give back to its button. The menu sends the event while
 * it is removed, before the field is reachable: the focus waits for the end of
 * the update.
 *
 * @param event Return of the focus to the button.
 */
async function onMenuClosed(event: Event): Promise<void> {
  if (naming.value !== null || pendingPanel !== null) {
    event.preventDefault()
    await showPending()
  }
}

/**
 * Closes the field being typed, without changing anything.
 */
function cancelName(): void {
  naming.value = null
}

/**
 * Confirms the name being typed.
 */
function commitName(): void {
  const place = naming.value
  if (place === null) {
    return
  }
  naming.value = null
  config.value = withName(config.value, place, draft.value, props.names, props.types)
}

/**
 * Element dragged: its column, and its place when it comes from a zone.
 * The browser gives the data of a drag only at the drop: the zones read
 * this one while it passes over them.
 */
const dragged = ref<{ column: number; from: ZonePlace | null } | null>(null)

/**
 * Zone and position where the dragged element would be dropped, `null`
 * outside the zones.
 */
const dropAt = ref<ZonePlace | null>(null)

/**
 * Sections of the zones: a drop outside them removes the dragged element.
 */
const zoneSections = useTemplateRef<HTMLElement[]>('zoneSection')

/**
 * Does an event of the drag happen over a zone?
 *
 * @param event Event of the drag.
 */
function overZone(event: DragEvent): boolean {
  const target = event.target
  return target instanceof Node && (zoneSections.value ?? []).some((zone) => zone.contains(target))
}

/**
 * Entry or passage of an element of a zone over the rest of the page: the
 * browser is told it can be dropped there, and what lies below does not hear
 * of it (the SQL editor would take its text).
 *
 * @param event Entry in an element of the page, or passage over it.
 */
function onDragOverPage(event: DragEvent): void {
  if (!overZone(event)) {
    event.preventDefault()
    event.stopPropagation()
    if (event.dataTransfer) {
      event.dataTransfer.dropEffect = 'move'
    }
  }
}

/**
 * Drop of an element of a zone on the rest of the page: it leaves its zone.
 * Escape, or a release outside the page, sends no drop and changes nothing.
 * The end of the drag is handled here: the browser may not tell it to an
 * element that has just left the page.
 *
 * @param event Drop on the page.
 */
function onDropOnPage(event: DragEvent): void {
  const from = dragged.value?.from ?? null
  if (from !== null && !overZone(event)) {
    event.preventDefault()
    event.stopPropagation()
    config.value = removeElement(config.value, from)
    onDragEnd()
  }
}

/**
 * Stops following the drag over the page.
 */
function stopFollowingPage(): void {
  document.removeEventListener('dragenter', onDragOverPage, { capture: true })
  document.removeEventListener('dragover', onDragOverPage, { capture: true })
  document.removeEventListener('drop', onDropOnPage, { capture: true })
}

onBeforeUnmount(stopFollowingPage)

/**
 * Starts dragging a column or an element.
 *
 * @param event Start of the drag.
 * @param column Index of the column.
 * @param from Place of the element, `null` for a column of the list.
 */
function onDragStart(event: DragEvent, column: number, from: ZonePlace | null): void {
  dragged.value = { column, from }
  if (event.dataTransfer) {
    event.dataTransfer.effectAllowed = 'move'
    event.dataTransfer.setData('text/plain', props.names[column] ?? '')
  }
  if (from !== null) {
    document.addEventListener('dragenter', onDragOverPage, { capture: true })
    document.addEventListener('dragover', onDragOverPage, { capture: true })
    document.addEventListener('drop', onDropOnPage, { capture: true })
  }
}

/**
 * Ends a drag, dropped or not.
 */
function onDragEnd(): void {
  dragged.value = null
  dropAt.value = null
  stopFollowingPage()
}

/**
 * Place in a zone under the pointer: before the first element whose
 * middle is below it.
 *
 * @param event Passage over the zone, or drop.
 * @param zone Zone.
 */
function placeUnder(event: DragEvent, zone: PivotZone): ZonePlace {
  const list = (event.currentTarget as HTMLElement).querySelectorAll('[data-element]')
  const centers = Array.from(list, (element) => {
    const box = element.getBoundingClientRect()
    return box.top + box.height / 2
  })
  return { zone, index: dropPosition(centers, event.clientY) }
}

/**
 * Does a zone accept the dragged element? The filters refuse a measure no filter reads
 * (`filterable`).
 *
 * @param zone Zone.
 */
function accepts(zone: PivotZone): boolean {
  const from = dragged.value?.from
  const measure = from?.zone === 'measures' ? config.value.measures[from.index] : undefined
  return dragged.value !== null && (zone !== 'filters' || filterable(measure?.aggregate ?? 'count'))
}

/**
 * Accepts the dragged element over a zone, and follows it with the insertion
 * marker. Both events ask for it: when the pointer changes of element, the
 * browser sends only `dragenter`, and refused, it shows the forbidden cursor
 * until the next `dragover`.
 *
 * @param event Entry in the zone or in one of its elements, or passage over it.
 * @param zone Zone.
 */
function onDragOver(event: DragEvent, zone: PivotZone): void {
  if (accepts(zone)) {
    event.preventDefault()
    dropAt.value = placeUnder(event, zone)
  }
}

/**
 * Leaves a zone: the marker goes, unless the pointer only passes over an
 * element of the zone.
 *
 * @param event Exit of the zone or of one of its elements.
 */
function onDragLeave(event: DragEvent): void {
  const target = event.currentTarget as HTMLElement
  if (!(event.relatedTarget instanceof Node && target.contains(event.relatedTarget))) {
    dropAt.value = null
  }
}

/**
 * Drops the dragged element under the pointer, where the marker stands.
 * The place is read again from the drop: the marker may have left with
 * the pointer passing from one element to another.
 *
 * @param event Drop.
 * @param zone Zone.
 */
function onDrop(event: DragEvent, zone: PivotZone): void {
  if (dragged.value !== null && accepts(zone)) {
    event.preventDefault()
    place(dragged.value.column, placeUnder(event, zone), dragged.value.from)
    void showPending()
  }
  onDragEnd()
}

/**
 * Does the marker stand before an element, or after the last one?
 *
 * @param zone Zone of the element.
 * @param index Position of the element.
 */
function markerClass(zone: PivotZone, index: number): Record<string, boolean> {
  const at = dropAt.value
  const last = elements.value[zone].length - 1
  return {
    'drop-before': at?.zone === zone && at.index === index,
    'drop-after': at?.zone === zone && at.index === last + 1 && index === last,
  }
}
</script>

<template>
  <!-- Either line moves only the width of the section on its left, the table taking the rest. -->
  <split-group direction="horizontal" class="min-h-0 flex-1">
    <split-panel
      :id="`${sectionsId}-fields`"
      v-model:size="widths.fields.value"
      default-size="var(--pivot-fields-width)"
      min-size="var(--pivot-fields-min-width)"
      class="flex"
    >
      <!--
        The room beside the line is the padding of a child: that of a panel would add to the width
        Reka UI gives it.
      -->
      <div class="flex min-w-0 flex-1 flex-col gap-2 pe-2">
        <ui-input
          v-model="search"
          :icon="Search"
          placeholder="Rechercher une colonne"
          aria-label="Rechercher une colonne"
        />
        <ul class="flex min-h-0 flex-1 flex-col overflow-y-auto" aria-label="Colonnes du résultat">
          <li
            v-for="column in shownFields"
            :key="column"
            class="pivot-line group"
            draggable="true"
            @dragstart="onDragStart($event, column, null)"
            @dragend="onDragEnd"
          >
            <span class="pivot-mark" aria-hidden="true">
              <template v-if="marks[column]">
                <span v-if="typeof marks[column] === 'string'" class="pivot-glyph">
                  {{ marks[column] }}
                </span>
                <component :is="marks[column]" v-else class="size-4" />
              </template>
            </span>
            <span class="min-w-0 flex-1 truncate" v-tooltip="types[column]?.name">
              {{ names[column] }}
            </span>
            <ui-dropdown-menu :items="addItems(column)" @close-auto-focus="onMenuClosed">
              <ui-button
                :icon="Plus"
                variant="ghost"
                small
                :aria-label="`Ajouter ${names[column]}`"
                class="invisible group-hover:visible group-focus-within:visible data-[state=open]:visible"
              />
            </ui-dropdown-menu>
          </li>
          <li v-if="shownFields.length === 0" class="px-2 py-1 text-fg-secondary">
            Aucune colonne
          </li>
        </ul>
      </div>
    </split-panel>
    <split-handle
      :controls="`${sectionsId}-fields`"
      tooltip="Glisser pour redimensionner les colonnes du résultat, double clic pour leur rendre leur largeur par défaut"
      @activate="resetWidth('fields')"
    />
    <split-panel
      :id="`${sectionsId}-zones`"
      v-model:size="widths.zones.value"
      default-size="var(--pivot-zones-width)"
      min-size="var(--pivot-zones-min-width)"
      class="flex"
    >
      <div class="flex min-w-0 flex-1 px-2">
        <!-- The room between two zones is the padding of the upper one: a drag never leaves them there. -->
        <div class="flex min-h-0 min-w-0 flex-1 flex-col overflow-y-auto">
          <section
            v-for="{ zone, label, icon } in ZONES"
            ref="zoneSection"
            :key="zone"
            class="flex flex-col gap-1 not-last:pb-3"
            :aria-label="label"
            :data-zone="zone"
            @dragenter="onDragOver($event, zone)"
            @dragover="onDragOver($event, zone)"
            @dragleave="onDragLeave"
            @drop="onDrop($event, zone)"
          >
            <h3 class="flex items-center gap-2 px-2 text-sm font-medium text-fg-secondary">
              <component :is="icon" class="size-4" aria-hidden="true" />
              {{ label }}
            </h3>
            <ul class="pivot-zone" :class="{ 'drop-target': dropAt?.zone === zone }">
              <li
                v-for="(element, index) in elements[zone]"
                :key="`${zone}-${index}`"
                data-element
                class="pivot-line pivot-element group"
                :class="[
                  markerClass(zone, index),
                  { invalid: element.invalid, masked: element.hidden },
                ]"
                :draggable="!isNaming(zone, index)"
                @dragstart="onDragStart($event, element.column, { zone, index })"
                @dragend="onDragEnd"
              >
                <span class="pivot-mark" aria-hidden="true">
                  <template v-if="marks[element.column]">
                    <span v-if="typeof marks[element.column] === 'string'" class="pivot-glyph">
                      {{ marks[element.column] }}
                    </span>
                    <component :is="marks[element.column]" v-else class="size-4" />
                  </template>
                </span>
                <input
                  v-if="isNaming(zone, index)"
                  ref="nameInput"
                  v-model="draft"
                  :aria-label="`Nom de ${element.expression}`"
                  class="h-7 min-w-0 flex-1 rounded-lg border border-line bg-surface px-1 text-base text-fg-strong focus-visible:outline-(length:--focus-width) focus-visible:outline-offset-(--focus-offset) focus-visible:outline-primary/60"
                  @blur="commitName"
                  @keydown.stop
                  @keydown.enter.prevent="commitName"
                  @keydown.escape.prevent="cancelName"
                  @mousedown.stop
                />
                <span
                  v-else
                  class="min-w-0 flex-1 truncate"
                  v-tooltip="
                    zone === 'filters' || element.label !== element.expression
                      ? element.expression
                      : undefined
                  "
                  @dblclick="nameAtOnce({ zone, index })"
                >
                  {{ element.label }}
                </span>
                <!-- The buttons side by side, without the gap that sets the name apart. -->
                <span class="flex items-center">
                  <ui-button
                    :icon="element.hidden ? EyeOff : Eye"
                    variant="ghost"
                    :tone="element.hidden ? 'primary' : 'neutral'"
                    small
                    :aria-pressed="element.hidden"
                    :aria-label="`${element.hidden ? 'Afficher' : 'Masquer'} : ${element.label}`"
                    v-tooltip="
                      element.hidden
                        ? 'Afficher : l’élément revient dans le calcul'
                        : 'Masquer : l’élément sort du calcul et garde ses réglages'
                    "
                    @click="toggleHidden({ zone, index })"
                  />
                  <ui-dropdown-menu
                    :groups="elementGroups(zone, index)"
                    @close-auto-focus="onMenuClosed"
                  >
                    <ui-button
                      :icon="EllipsisVertical"
                      variant="ghost"
                      small
                      :aria-label="`Actions : ${element.label}`"
                    />
                  </ui-dropdown-menu>
                </span>
              </li>
              <li v-if="elements[zone].length === 0" class="px-2 py-1 text-sm text-fg-faint">
                Glisser une colonne ici
              </li>
            </ul>
          </section>
        </div>
      </div>
    </split-panel>
    <split-handle
      :controls="`${sectionsId}-zones`"
      tooltip="Glisser pour redimensionner les zones, double clic pour leur rendre leur largeur par défaut"
      @activate="resetWidth('zones')"
    />
    <!-- The table, given by the panel. -->
    <split-panel :id="`${sectionsId}-table`" fill class="flex">
      <div class="flex min-w-0 flex-1 ps-2">
        <slot />
      </div>
    </split-panel>
  </split-group>
  <pivot-filter-popover
    v-if="panel && panelFilter"
    :key="`filters-${panel.index}`"
    :anchor="panel.anchor"
    :filter="panelFilter"
    :index="panel.index"
    :config="config"
    :names="names"
    :types="types"
    :read-values="readValues"
    :receiving="receiving"
    @change="changePanelFilter"
    @name="nameAtOnce({ zone: 'filters', index: panel?.index ?? 0 })"
    @close="panel = null"
  />
</template>

<style scoped>
@reference "@/assets/main.css";

/* Line of a column or of an element: its mark, its name, its menu button. */
.pivot-line {
  display: flex;
  flex-shrink: 0;
  align-items: center;
  gap: --spacing(2);
  block-size: --spacing(8);
  padding-inline: --spacing(2);
  border-radius: var(--radius-lg);
  cursor: grab;

  &:hover {
    background: var(--color-surface-sunken);
  }
}

/* Zone: its elements, in a frame that receives the drops. */
.pivot-zone {
  display: flex;
  flex-direction: column;
  gap: --spacing(1);
  padding: --spacing(1);
  border: var(--line-width) dashed var(--color-line);
  border-radius: var(--radius-lg);

  &.drop-target {
    border-color: var(--color-primary);
  }
}

.pivot-element {
  border: var(--line-width) solid var(--color-line);
  background: var(--color-surface);

  /* Insertion marker of a dragged element, as for the grid's columns. */
  &.drop-before {
    box-shadow: inset 0 var(--marker-width) 0 var(--color-primary);
  }

  &.drop-after {
    box-shadow: inset 0 calc(-1 * var(--marker-width)) 0 var(--color-primary);
  }

  &.invalid {
    border-color: var(--color-error);
  }

  /* A hidden element, greyed: its name takes the color of its mark. */
  &.masked {
    color: var(--color-fg-faint);
  }
}

/* Mark of the type, in a box of an icon's size, as in the grid's headers. */
.pivot-mark {
  display: flex;
  flex: none;
  align-items: center;
  justify-content: center;
  inline-size: --spacing(5);
  color: var(--color-fg-faint);
}

.pivot-glyph {
  font-family: var(--font-mono);
  font-size: var(--text-sm);
  font-weight: 500;
  letter-spacing: -0.05em;
}
</style>
