<!--
  Settings of the Pivoteur (`PivotPanel.vue`), in two columns: the columns
  of the result, with the mark of their type and a search by name above
  them (F-10), then the zones Lignes, Colonnes and Mesures, stacked, which
  scroll in their column when the panel is low (F-05).

  A column enters a zone by dragging it there, or by the "+" menu of its
  line (F-11); the order in a zone sets the nesting, the first element
  being the outermost level (F-27). An element moves by dragging it, from
  one zone to another too, or by its menu, which also removes it. A column
  dropped into the measures gives `SUM` for a number, `COUNT` otherwise
  (F-15); the menu of a measure changes its aggregate. A column is a level
  once at most: placing it again on an axis moves it there. An element
  takes a name by its menu ("Nommer…") or a double click on its label
  (F-17): Enter confirms, Escape cancels, an empty name gives back its
  expression, which the tooltip of a named element recalls. The logic of
  the zones is in `pivotConfig.ts`.
-->
<script setup lang="ts">
import {
  ArrowDown,
  ArrowDownToLine,
  ArrowLeftRight,
  ArrowUp,
  ArrowUpToLine,
  Asterisk,
  Check,
  Columns3,
  Divide,
  EllipsisVertical,
  FingerprintPattern,
  Hash,
  PenLine,
  Plus,
  Rows3,
  Search,
  Sigma,
  SquareDivide,
  SquareSigma,
  X,
} from '@lucide/vue'
import { type Component, computed, nextTick, ref, useTemplateRef } from 'vue'

import type { ColumnTypeInfo } from '@/api/generated/ColumnTypeInfo'

import {
  type PivotConfig,
  type PivotZone,
  type ZonePlace,
  addsNumbers,
  elementExpression,
  elementLabel,
  measureExpression,
  placeColumn,
  removeElement,
  summable,
  withAggregate,
  withName,
} from './pivotConfig'
import type { PivotMeasure } from './pivotEngine'
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
   * measure.
   */
  expression: string
}

/**
 * Elements of each zone.
 */
const elements = computed<Record<PivotZone, ZoneElement[]>>(() => {
  const shown = (zone: PivotZone) =>
    config.value[zone].map((element, index) => ({
      column: element.column,
      label: elementLabel(config.value, { zone, index }, props.names),
      expression: elementExpression(config.value, { zone, index }, props.names),
    }))
  return { rows: shown('rows'), columns: shown('columns'), measures: shown('measures') }
})

/**
 * Places a column in a zone, a new element or a moved one.
 *
 * @param column Index of the column.
 * @param to Zone and position.
 * @param from Place of the moved element, `null` for a new one.
 */
function place(column: number, to: ZonePlace, from: ZonePlace | null): void {
  config.value = placeColumn(config.value, column, props.types[column]?.family, to, from)
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
 * Choices that name, move or remove an element.
 *
 * @param zone Zone of the element.
 * @param index Position of the element.
 */
function elementItems(zone: PivotZone, index: number): MenuItem[] {
  const column = elements.value[zone][index]!.column
  const from = { zone, index }
  const count = elements.value[zone].length
  const items: MenuItem[] = [
    {
      label: 'Nommer…',
      icon: PenLine,
      // The field takes the focus once the menu has closed (`onMenuClosed`).
      onSelect: () => startNaming(from),
    },
    {
      label: 'Monter',
      icon: ArrowUp,
      disabled: index === 0,
      onSelect: () => place(column, { zone, index: index - 1 }, from),
    },
    {
      label: 'Descendre',
      icon: ArrowDown,
      disabled: index === count - 1,
      onSelect: () => place(column, { zone, index: index + 2 }, from),
    },
  ]
  if (zone !== 'measures') {
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
 * Aggregates a measure can take, with their icon.
 */
const AGGREGATES: { aggregate: PivotMeasure['aggregate']; icon: Component }[] = [
  { aggregate: 'sum', icon: Sigma },
  { aggregate: 'sum_distinct', icon: SquareSigma },
  { aggregate: 'count', icon: Hash },
  { aggregate: 'count_distinct', icon: FingerprintPattern },
  { aggregate: 'count_star', icon: Asterisk },
  { aggregate: 'min', icon: ArrowDownToLine },
  { aggregate: 'max', icon: ArrowUpToLine },
  { aggregate: 'avg', icon: Divide },
  { aggregate: 'avg_distinct', icon: SquareDivide },
]

/**
 * Choices of the menu of an element: for a measure, its aggregate, then
 * its name and its place.
 *
 * @param zone Zone of the element.
 * @param index Position of the element.
 */
function elementGroups(zone: PivotZone, index: number): MenuGroup[] {
  const groups: MenuGroup[] = []
  const measure = zone === 'measures' ? config.value.measures[index] : undefined
  if (measure !== undefined) {
    const numbers = summable(props.types[measure.column]?.family)
    groups.push({
      label: 'Agrégat',
      items: AGGREGATES.map(({ aggregate, icon }) => {
        const refused = addsNumbers(aggregate) && !numbers
        return {
          label: measureExpression({ aggregate, column: measure.column }, props.names),
          icon: aggregate === measure.aggregate ? Check : icon,
          disabled: refused,
          hint: refused ? 'Demande une colonne de nombres' : undefined,
          onSelect: () => (config.value = withAggregate(config.value, index, aggregate)),
        }
      }),
    })
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
 * Focuses the field of the name, its text selected.
 */
function focusName(): void {
  const input = nameInputs.value?.[0]
  input?.focus()
  input?.select()
}

/**
 * Double click on the label of an element: names it at once.
 *
 * @param place Place of the element.
 */
async function nameOnDoubleClick(place: ZonePlace): Promise<void> {
  startNaming(place)
  await nextTick()
  focusName()
}

/**
 * Closing of the menu of an element: when "Nommer…" was chosen, the field
 * of the name takes the focus, which the menu would give back to its
 * button. The menu sends the event while it is removed, before the field
 * is reachable: the focus waits for the end of the update.
 *
 * @param event Return of the focus to the button.
 */
async function onMenuClosed(event: Event): Promise<void> {
  if (naming.value !== null) {
    event.preventDefault()
    await nextTick()
    focusName()
  }
}

/**
 * Confirms the name typed.
 */
function commitName(): void {
  const place = naming.value
  if (place !== null) {
    naming.value = null
    config.value = withName(config.value, place, draft.value, props.names)
  }
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
}

/**
 * Ends a drag, dropped or not.
 */
function onDragEnd(): void {
  dragged.value = null
  dropAt.value = null
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
 * Follows the dragged element over a zone, with the insertion marker.
 *
 * @param event Passage over the zone.
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
          <ui-dropdown-menu :items="addItems(column)">
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

    <div class="flex min-h-0 w-64 shrink-0 flex-col gap-3 overflow-y-auto">
      <section
        v-for="{ zone, label, icon } in ZONES"
        :key="zone"
        class="flex flex-col gap-1"
        :aria-label="label"
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
            :class="markerClass(zone, index)"
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
              @keydown.escape.prevent="naming = null"
              @mousedown.stop
            />
            <span
              v-else
              class="min-w-0 flex-1 truncate"
              v-tooltip="element.label === element.expression ? undefined : element.expression"
              @dblclick="nameOnDoubleClick({ zone, index })"
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
