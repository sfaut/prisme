<!--
  Settings of a filter of the Pivoteur (F-19, F-21, F-44), in a popover beside
  its element: the filter opens it at its drop, and its menu opens it again
  ("Réglages…").

  A filter on a column sets its precision, when it holds dates, and its condition;
  a filter on a measure, its aggregate, its precision for `COUNT(DISTINCT)`, `MIN`
  and `MAX` of dates, its condition and its level. The condition
  shows what it asks: a value, two bounds, the list of the values of the column
  (`PivotValuesList.vue`) or nothing. A choice or a tick applies at once; a value
  typed applies at Enter or on leaving its field, which refuses a value that cannot
  compare, showing why, and changes nothing. Escape closes the panel and forgets
  what is being typed; a click outside applies it. A filter keeps the values typed
  under another condition. Changing the precision empties them: they have the
  form of the former one. "Nommer…" closes the panel and names the element.
-->
<script setup lang="ts">
import {
  ChevronLeft,
  ChevronRight,
  ChevronsLeft,
  ChevronsRight,
  Circle,
  CircleDot,
  Columns3,
  Equal,
  EqualNot,
  FoldHorizontal,
  Layers,
  ListChecks,
  ListX,
  PenLine,
  Rows3,
  UnfoldHorizontal,
} from '@lucide/vue'
import { PopoverAnchor, PopoverContent, PopoverPortal, PopoverRoot } from 'reka-ui'
import { type Component, computed, reactive, watch } from 'vue'

import type { ColumnTypeInfo } from '@/api/generated/ColumnTypeInfo'
import type { Cell } from '@/api/querySession'
import { cssPixels } from '@/cssLength'

import { AGGREGATES, PRECISION_ICONS } from './pivotChoices'
import {
  type DatePrecision,
  type PivotColumnFilter,
  type PivotCondition,
  type PivotConfig,
  type PivotFilter,
  type PivotLevel,
  type PivotMeasureFilter,
  addsNumbers,
  chosenLevel,
  elementExpression,
  elementLabel,
  keptPrecision,
  measureExpression,
  measurePrecision,
  operandCount,
  operandExample,
  operandRefusal,
  precisionChoices,
  summable,
  takesPrecision,
} from './pivotConfig'
import PivotValuesList from './PivotValuesList.vue'
import { MENU_PANEL } from './ui/menuItem'
import UiButton from './ui/UiButton.vue'
import UiInput from './ui/UiInput.vue'
import UiSelect from './ui/UiSelect.vue'

/**
 * Gap with the element, in pixels (`--popup-offset` of the theme).
 *
 * Fixed size: Reka UI's `side-offset` only takes pixels, so this theme
 * length is read once (`docs/DEVELOPER.md`, section "Lengths in pixels").
 */
const OFFSET = cssPixels('--popup-offset', 4)

/**
 * Conditions of a filter, with their icon. A filter on a measure has no list.
 */
const CONDITIONS: { value: PivotCondition; label: string; icon: Component }[] = [
  { value: 'in_list', label: 'Dans la liste', icon: ListChecks },
  { value: 'not_in_list', label: 'Hors de la liste', icon: ListX },
  { value: 'equal', label: 'Égal à', icon: Equal },
  { value: 'not_equal', label: 'Différent de', icon: EqualNot },
  { value: 'less', label: 'Inférieur à', icon: ChevronLeft },
  { value: 'less_or_equal', label: 'Inférieur ou égal à', icon: ChevronsLeft },
  { value: 'greater', label: 'Supérieur à', icon: ChevronRight },
  { value: 'greater_or_equal', label: 'Supérieur ou égal à', icon: ChevronsRight },
  { value: 'between', label: 'Entre', icon: FoldHorizontal },
  { value: 'not_between', label: 'N’est pas entre', icon: UnfoldHorizontal },
  { value: 'is_null', label: 'Est NULL', icon: Circle },
  { value: 'is_not_null', label: 'N’est pas NULL', icon: CircleDot },
]

/**
 * Value of the choice of the default level.
 */
const DEFAULT_LEVEL = 'default'

/**
 * Fields of the values of a test.
 */
const VALUE_FIELDS = ['operand', 'upper'] as const

const props = defineProps<{
  /**
   * Element the panel opens beside.
   */
  anchor: HTMLElement
  /**
   * Filter set.
   */
  filter: PivotFilter
  /**
   * Position of the filter.
   */
  index: number
  /**
   * Configuration, for the levels a filter on a measure can read.
   */
  config: PivotConfig
  /**
   * Names of the columns of the result.
   */
  names: string[]
  /**
   * Type of each column of the result.
   */
  types: ColumnTypeInfo[]
  /**
   * Reads the distinct values of a column, or the periods of a precision of its dates.
   */
  readValues: (column: number, precision: DatePrecision) => Promise<Cell[]>
}>()

const emit = defineEmits<{
  /**
   * The user changes the filter.
   */
  change: [filter: PivotFilter]
  /**
   * The user asks for the name of the filter.
   */
  name: []
  /**
   * The user closes the panel.
   */
  close: []
}>()

/**
 * Filter on a column, when it is one.
 */
const columnFilter = computed<PivotColumnFilter | null>(() =>
  props.filter.kind === 'column' ? props.filter : null,
)

/**
 * Filter on a measure, when it is one.
 */
const measureFilter = computed<PivotMeasureFilter | null>(() =>
  props.filter.kind === 'measure' ? props.filter : null,
)

/**
 * Expression of the filter, which names the panel.
 */
const expression = computed(() =>
  elementExpression(
    props.config,
    { zone: 'filters', index: props.index },
    props.names,
    props.types,
  ),
)

/**
 * Changes some settings of the filter.
 *
 * @param changes What changes.
 */
function change(changes: Partial<PivotColumnFilter> | Partial<PivotMeasureFilter>): void {
  emit('change', { ...props.filter, ...changes } as PivotFilter)
}

/**
 * Conditions the filter offers.
 */
const conditions = computed(() =>
  CONDITIONS.filter(
    ({ value }) =>
      props.filter.kind === 'column' || (value !== 'in_list' && value !== 'not_in_list'),
  ),
)

/**
 * Takes the condition chosen.
 *
 * @param condition Condition.
 */
function chooseCondition(condition: PivotCondition | undefined): void {
  if (condition !== undefined) {
    change({ condition } as Partial<PivotColumnFilter>)
  }
}

/**
 * What the filter keeps of the dates of its column.
 */
const filterPrecision = computed(
  () => columnFilter.value?.precision ?? measurePrecision(measureFilter.value ?? undefined),
)

/**
 * Precisions the column of the filter offers: none for a column that holds no dates, nor for a
 * measure whose aggregate takes none.
 */
const precisions = computed(() => {
  const measure = measureFilter.value
  if (measure !== null && !takesPrecision(measure.aggregate)) {
    return []
  }
  return precisionChoices(props.types[props.filter.column]?.family).map(({ precision, label }) => ({
    value: precision,
    label,
    icon: PRECISION_ICONS[label],
  }))
})

/**
 * Takes the precision chosen: what the filter compared to has the form of the former one, or for a
 * count, another meaning.
 *
 * @param chosen Precision.
 */
function choosePrecision(chosen: DatePrecision | undefined): void {
  if (chosen === undefined || chosen === filterPrecision.value) {
    return
  }
  for (const name of VALUE_FIELDS) {
    drafts[name] = ''
    confirmed[name] = ''
  }
  const measure = measureFilter.value
  change(
    measure === null
      ? { precision: chosen, values: [], operand: '', upper: '' }
      : { precision: keptPrecision(measure.aggregate, chosen), operand: '', upper: '' },
  )
}

/**
 * Takes the aggregate chosen, which keeps the precision if it takes one.
 *
 * @param aggregate Aggregate.
 */
function chooseAggregate(aggregate: PivotMeasureFilter['aggregate'] | undefined): void {
  const measure = measureFilter.value
  if (aggregate !== undefined && measure !== null) {
    change({ aggregate, precision: keptPrecision(aggregate, measure.precision) })
  }
}

/**
 * Aggregates of the filter on a measure; those that add numbers up need a column of numbers.
 */
const aggregates = computed(() => {
  const filter = measureFilter.value
  if (filter === null) {
    return []
  }
  const numbers = summable(props.types[filter.column]?.family)
  return AGGREGATES.map(({ aggregate, icon }) => {
    const refused = addsNumbers(aggregate) && !numbers
    const { column, precision } = filter
    return {
      value: aggregate,
      label: measureExpression({ aggregate, column, precision }, props.names),
      icon,
      disabled: refused,
      hint: refused ? 'Demande une colonne de nombres' : undefined,
    }
  })
})

/**
 * Value of the choice of a level: its column and its precision.
 *
 * @param level Level.
 */
function levelValue({ column, precision }: Pick<PivotLevel, 'column' | 'precision'>): string {
  return `${column}:${precision}`
}

/**
 * Levels the filter on a measure can read: the default one, then those of the two axes, the same
 * level placed twice being one choice.
 */
const levels = computed(() => {
  const axes = [
    { zone: 'rows', icon: Rows3 },
    { zone: 'columns', icon: Columns3 },
  ] as const
  const seen = new Set<string>()
  return [
    { value: DEFAULT_LEVEL, label: 'Le niveau le plus fin', icon: Layers as Component },
    ...axes.flatMap(({ zone, icon }) =>
      props.config[zone].flatMap((level, index) => {
        const value = levelValue(level)
        if (seen.has(value)) {
          return []
        }
        seen.add(value)
        const label = elementLabel(props.config, { zone, index }, props.names)
        return [{ value, label, icon: icon as Component }]
      }),
    ),
  ]
})

/**
 * Choice of the level the filter on a measure reads.
 */
const chosen = computed(() => {
  const level = measureFilter.value && chosenLevel(props.config, measureFilter.value)
  return level ? levelValue(level) : DEFAULT_LEVEL
})

/**
 * Takes the level chosen.
 *
 * @param value Choice of the level.
 */
function chooseLevel(value: string | undefined): void {
  const level = [...props.config.rows, ...props.config.columns].find(
    (item) => levelValue(item) === value,
  )
  change({ level: level?.column, level_precision: level?.precision ?? 'none' })
}

/**
 * Field of a value of a test.
 */
type ValueField = (typeof VALUE_FIELDS)[number]

/**
 * Fields the condition asks for.
 */
const fields = computed<{ name: ValueField; label: string }[]>(() => {
  switch (operandCount(props.filter.condition)) {
    case 2:
      return [
        { name: 'operand', label: 'De' },
        { name: 'upper', label: 'À' },
      ]
    case 1:
      return [{ name: 'operand', label: 'Valeur' }]
    default:
      return []
  }
})

/**
 * Text being typed in each field.
 */
const drafts = reactive({ operand: props.filter.operand, upper: props.filter.upper })

/**
 * Text of each field when the user last confirmed it, which its refusal reads: the field does not
 * complain during the typing.
 */
const confirmed = reactive({ operand: props.filter.operand, upper: props.filter.upper })

for (const name of VALUE_FIELDS) {
  watch(
    () => props.filter[name],
    (value) => {
      drafts[name] = value
      confirmed[name] = value
    },
  )
}

/**
 * Why the text of a field is refused, `null` otherwise.
 *
 * @param name Field.
 */
function refusal(name: ValueField): string | null {
  const text = confirmed[name]
  return text === '' ? null : operandRefusal(props.filter, text, props.types)
}

/**
 * Has Escape been pressed? The field that loses the focus with it applies nothing.
 */
let leaving = false

/**
 * Confirms the text of some fields: the filter takes those that are not refused, in one change.
 *
 * @param names Fields.
 */
function confirm(names: readonly ValueField[]): void {
  if (leaving) {
    return
  }
  const changes: Partial<Record<ValueField, string>> = {}
  for (const name of names) {
    const text = drafts[name].trim()
    drafts[name] = text
    confirmed[name] = text
    if (refusal(name) === null && text !== props.filter[name]) {
      changes[name] = text
    }
  }
  if (Object.keys(changes).length > 0) {
    change(changes)
  }
}

/**
 * Closes the panel, after a click outside or Escape: a click outside can remove the field before
 * it loses the focus, so what is being typed applies here.
 */
function close(): void {
  confirm(fields.value.map(({ name }) => name))
  emit('close')
}

/**
 * Gives the first field the focus at the opening, so that its value can be typed at once.
 *
 * @param event Focus of the opening.
 */
function focusField(event: Event): void {
  const field = (event.target as HTMLElement).querySelector('input')
  if (field !== null) {
    event.preventDefault()
    field.focus()
    field.select()
  }
}

/**
 * Escape forgets what is being typed, and gives the focus back to the menu button of the element.
 */
function onEscape(): void {
  leaving = true
  props.anchor.querySelector('button')?.focus()
}
</script>

<template>
  <popover-root :open="true" @update:open="close">
    <!-- The element of the zones is the anchor: this part draws nothing among their children. -->
    <popover-anchor as-child :reference="anchor" />
    <popover-portal>
      <popover-content
        side="right"
        align="start"
        :side-offset="OFFSET"
        :collision-padding="OFFSET"
        :class="[
          MENU_PANEL,
          'flex max-h-(--reka-popover-content-available-height) w-80 flex-col gap-3 overflow-y-auto p-3',
        ]"
        :aria-label="`Réglages de ${expression}`"
        @open-auto-focus="focusField"
        @escape-key-down="onEscape"
      >
        <div v-if="measureFilter" class="field">
          <span class="field-label" aria-hidden="true">Agrégat</span>
          <ui-select
            :model-value="measureFilter.aggregate"
            :items="aggregates"
            label="Agrégat"
            class="w-full"
            @update:model-value="chooseAggregate"
          />
        </div>
        <div v-if="precisions.length > 0" class="field">
          <span class="field-label" aria-hidden="true">Précision</span>
          <ui-select
            :model-value="filterPrecision"
            :items="precisions"
            label="Précision"
            class="w-full"
            @update:model-value="choosePrecision"
          />
        </div>
        <div class="field">
          <span class="field-label" aria-hidden="true">Condition</span>
          <ui-select
            :model-value="filter.condition"
            :items="conditions"
            label="Condition"
            class="w-full"
            @update:model-value="chooseCondition"
          />
        </div>
        <div v-for="{ name, label } in fields" :key="name" class="field">
          <span class="field-label" aria-hidden="true">{{ label }}</span>
          <ui-input
            v-model="drafts[name]"
            :aria-label="label"
            :aria-invalid="refusal(name) !== null"
            :placeholder="operandExample(filter, types)"
            @blur="confirm([name])"
            @keydown.enter.prevent="confirm([name])"
          />
          <p v-if="refusal(name)" role="alert" class="text-sm text-error">{{ refusal(name) }}</p>
        </div>
        <div
          v-if="
            columnFilter && (filter.condition === 'in_list' || filter.condition === 'not_in_list')
          "
          class="field"
        >
          <pivot-values-list
            :key="columnFilter.precision"
            :label="names[columnFilter.column] ?? '?'"
            :read="() => readValues(columnFilter?.column ?? -1, columnFilter?.precision ?? 'none')"
            :offers-null="filter.condition === 'in_list'"
            :kept="columnFilter.values"
            @update:kept="change({ values: $event })"
          />
        </div>
        <div v-if="measureFilter" class="field">
          <span class="field-label" aria-hidden="true">Niveau</span>
          <ui-select
            :model-value="chosen"
            :items="levels"
            label="Niveau"
            class="w-full"
            @update:model-value="chooseLevel"
          />
        </div>
        <div class="flex justify-end">
          <ui-button label="Nommer…" :icon="PenLine" variant="ghost" small @click="emit('name')" />
        </div>
      </popover-content>
    </popover-portal>
  </popover-root>
</template>

<style scoped>
@reference "@/assets/main.css";

/* One setting: its label, then its control and what it asks. */
.field {
  display: flex;
  flex-direction: column;
  gap: --spacing(1);
}

.field-label {
  color: var(--color-fg-secondary);
  font-size: var(--text-sm);
  font-weight: 500;
  line-height: var(--text-sm--line-height);
}
</style>
