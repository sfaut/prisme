<!--
  Settings of the Pivoteur (`PivotPanel.vue`), in two columns: the columns
  of the result, with the mark of their type and a search by name above
  them (F-10), then the zones Lignes, Colonnes, Mesures and Filtres, stacked,
  which scroll in their column when the panel is low (F-05).

  A column enters a zone by dragging it there, or by the "+" menu of its
  line (F-11); the order in a zone sets the nesting, the first element
  being the outermost level (F-27). An element moves by dragging it, from
  one zone to another too, a level to the other axis by its menu too. It
  leaves its zone by its menu ("Retirer"), or released outside the zones
  (Escape cancels the drag).
  A column dropped into the measures gives its default aggregate
  (`defaultAggregate`, F-15); the menu of a measure changes it. The menu of a
  level of dates sets its precision, what it keeps of them (`precisionChoices`).
  A column can be placed several times, in a zone and on both axes: dropping it
  from the list always adds an element, dragging an element moves it.
  An element takes a name by its menu ("Nommer…") or a double click on its label
  (F-17): Enter confirms, Escape cancels, an empty name gives back its
  expression, which the tooltip of a named element recalls.

  A column or a measure dropped into the filters, or a measure by its menu
  ("Filtrer sur cette mesure"), gives a filter, whose settings open beside it
  (`PivotFilterPanel.vue`, F-21, F-44); its menu opens them again ("Réglages…").
  The logic of the zones is in `pivotConfig.ts`.
-->
<script setup lang="ts">
import {
  ArrowLeftRight,
  Check,
  Columns3,
  EllipsisVertical,
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
  useTemplateRef,
} from 'vue'

import type { ColumnTypeInfo } from '@/api/generated/ColumnTypeInfo'
import type { Cell } from '@/api/querySession'

import { AGGREGATES, PRECISION_ICONS } from './pivotChoices'
import {
  type DatePrecision,
  type PivotConfig,
  type PivotFilter,
  type PivotZone,
  type ZonePlace,
  addsNumbers,
  defaultAggregate,
  elementExpression,
  elementLabel,
  isRefused,
  measureExpression,
  placeColumn,
  precisionChoices,
  removeElement,
  summable,
  withAggregate,
  withFilter,
  withName,
  withPrecision,
} from './pivotConfig'
import type { PivotMeasure } from './pivotEngine'
import PivotFilterPanel from './PivotFilterPanel.vue'
import { dropPosition } from './resultGrid'
import type { TypeMark } from './typeIcons'
import type { MenuGroup, MenuItem } from './ui/menuItem'
import UiButton from './ui/UiButton.vue'
import UiDropdownMenu from './ui/UiDropdownMenu.vue'
import UiInput from './ui/UiInput.vue'
import { vTooltip } from './ui/tooltip'

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
  config.value = placed
  if (to.zone === 'filters' && from?.zone !== 'filters') {
    pendingPanel = Math.min(to.index, placed.filters.length - 1)
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
 * Choices that set, name, move or remove an element, the settings of a filter first.
 *
 * @param zone Zone of the element.
 * @param index Position of the element.
 */
function elementItems(zone: PivotZone, index: number): MenuItem[] {
  const column = elements.value[zone][index]!.column
  const from = { zone, index }
  const items: MenuItem[] = []
  if (zone === 'filters') {
    items.push({
      label: 'Réglages…',
      icon: SlidersHorizontal,
      // They open beside the element once the menu has closed (`onMenuClosed`).
      onSelect: () => (pendingPanel = index),
    })
  }
  items.push({
    label: 'Nommer…',
    icon: PenLine,
    // The field takes the focus once the menu has closed (`onMenuClosed`).
    onSelect: () => startNaming(from),
  })
  if (zone === 'measures') {
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
  items.push({
    label: 'Retirer',
    icon: X,
    onSelect: () => (config.value = removeElement(config.value, from)),
  })
  return items
}

/**
 * Choices of the precision of a level, none for a column that holds no dates.
 *
 * @param zone Axis of the level.
 * @param index Position of the level.
 */
function precisionItems(zone: 'rows' | 'columns', index: number): MenuItem[] {
  const level = config.value[zone][index]
  if (level === undefined) {
    return []
  }
  return precisionChoices(props.types[level.column]?.family).map(({ precision, label }) => ({
    label,
    icon: precision === level.precision ? Check : PRECISION_ICONS[label],
    onSelect: () => (config.value = withPrecision(config.value, { zone, index }, precision)),
  }))
}

/**
 * Choices of the aggregate of a measure.
 *
 * @param measure Measure.
 * @param choose Takes the aggregate chosen.
 */
function aggregateItems(
  measure: PivotMeasure,
  choose: (aggregate: PivotMeasure['aggregate']) => void,
): MenuItem[] {
  const numbers = summable(props.types[measure.column]?.family)
  return AGGREGATES.map(({ aggregate, icon }) => {
    const refused = addsNumbers(aggregate) && !numbers
    return {
      label: measureExpression({ aggregate, column: measure.column }, props.names),
      icon: aggregate === measure.aggregate ? Check : icon,
      disabled: refused,
      hint: refused ? 'Demande une colonne de nombres' : undefined,
      onSelect: () => choose(aggregate),
    }
  })
}

/**
 * Choices of the menu of an element: for a measure, its aggregate, for a
 * level of dates, its precision, then the settings of a filter, its name, its
 * place and its removal.
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
      items: aggregateItems(
        measure,
        (aggregate) => (config.value = withAggregate(config.value, index, aggregate)),
      ),
    })
  }
  if (zone === 'rows' || zone === 'columns') {
    const items = precisionItems(zone, index)
    if (items.length > 0) {
      groups.push({ label: 'Précision', items })
    }
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
 * Position of the filter whose settings are to open, `null` without one.
 */
let pendingPanel: number | null = null

/**
 * Filter whose settings are open, with the element they open beside.
 */
const panel = shallowRef<{ index: number; anchor: HTMLElement } | null>(null)

/**
 * Filter whose settings are open.
 */
const panelFilter = computed(() => panel.value && config.value.filters[panel.value.index])

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
  const index = pendingPanel
  pendingPanel = null
  const section = zoneSections.value?.find((zone) => zone.dataset.zone === 'filters')
  const anchor = index === null ? undefined : section?.querySelectorAll('[data-element]')[index]
  if (index !== null && anchor instanceof HTMLElement) {
    // The zones scroll in their column: the panel opens beside an element in sight.
    anchor.scrollIntoView({ block: 'nearest' })
    panel.value = { index, anchor }
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
 * Double click on the label of an element, or the choice of the settings of a
 * filter: names it at once.
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
 * Accepts the dragged element over a zone, and follows it with the insertion
 * marker. Both events ask for it: when the pointer changes of element, the
 * browser sends only `dragenter`, and refused, it shows the forbidden cursor
 * until the next `dragover`.
 *
 * @param event Entry in the zone or in one of its elements, or passage over it.
 * @param zone Zone.
 */
function onDragOver(event: DragEvent, zone: PivotZone): void {
  if (dragged.value !== null) {
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
  if (dragged.value !== null) {
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
  <div class="flex min-h-0 gap-4">
    <div class="flex min-h-0 w-56 shrink-0 flex-col gap-2">
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
        <li v-if="shownFields.length === 0" class="px-2 py-1 text-fg-secondary">Aucune colonne</li>
      </ul>
    </div>

    <!-- The room between two zones is the padding of the upper one: a drag never leaves them there. -->
    <div class="flex min-h-0 w-64 shrink-0 flex-col overflow-y-auto">
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
            :class="[markerClass(zone, index), { invalid: element.invalid }]"
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
            <ui-dropdown-menu :groups="elementGroups(zone, index)" @close-auto-focus="onMenuClosed">
              <ui-button
                :icon="EllipsisVertical"
                variant="ghost"
                small
                :aria-label="`Actions : ${element.label}`"
              />
            </ui-dropdown-menu>
          </li>
          <li v-if="elements[zone].length === 0" class="px-2 py-1 text-sm text-fg-faint">
            Glisser une colonne ici
          </li>
        </ul>
      </section>
    </div>
    <pivot-filter-panel
      v-if="panel && panelFilter"
      :key="panel.index"
      :anchor="panel.anchor"
      :filter="panelFilter"
      :index="panel.index"
      :config="config"
      :names="names"
      :types="types"
      :read-values="readValues"
      @change="changePanelFilter"
      @name="nameAtOnce({ zone: 'filters', index: panel?.index ?? 0 })"
      @close="panel = null"
    />
  </div>
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
