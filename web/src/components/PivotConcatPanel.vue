<!--
  Settings of a `GROUP_CONCAT` of the Pivoteur, in a popover beside its
  measure: the measure opens it when it takes this aggregate, and its menu opens
  it again ("Réglages…"). They follow MariaDB: `DISTINCT`, the keys of the
  order (`ORDER BY`), any column of the result, ascending or descending, the
  separator (`SEPARATOR`), then `LIMIT` and `OFFSET`, which only goes with a
  `LIMIT`. Without a key, the values follow the order of the rows of the result.

  A tick or a choice applies at once; a text typed applies at Enter or on
  leaving its field, a number refused showing why and changing nothing. The
  separator is kept as it is typed, its spaces included. Escape closes the
  panel and forgets what is being typed; a click outside applies it. "Nommer…"
  closes the panel and names the measure. The longest text is a setting of the
  user (`SettingsDialog.vue`).
-->
<script setup lang="ts">
import { ArrowDownWideNarrow, ArrowUpNarrowWide, PenLine, Plus, X } from '@lucide/vue'
import { PopoverAnchor, PopoverContent, PopoverPortal, PopoverRoot } from 'reka-ui'
import { computed, reactive, watch } from 'vue'

import type { ColumnTypeInfo } from '@/api/generated/ColumnTypeInfo'
import type { PivotConcatKey } from '@/api/generated/PivotConcatKey'
import { cssPixels } from '@/cssLength'

import { type PivotConcat, type PivotConfig, concatOf, elementExpression } from './pivotConfig'
import { MENU_PANEL } from './ui/menuItem'
import UiButton from './ui/UiButton.vue'
import UiInput from './ui/UiInput.vue'
import UiSelect from './ui/UiSelect.vue'
import { vTooltip } from './ui/tooltip'

/**
 * Gap with the element, in pixels (`--popup-offset` of the theme).
 *
 * Fixed size: Reka UI's `side-offset` only takes pixels, so this theme
 * length is read once (`docs/DEVELOPER.md`, section "Lengths in pixels").
 */
const OFFSET = cssPixels('--popup-offset', 4)

/**
 * Largest `LIMIT` or `OFFSET`: the engine reads them on 32 bits, its largest
 * word standing for no `LIMIT`.
 */
const LARGEST = 0xffff_fffe

/**
 * Fields of the numbers of the window of the values.
 */
const NUMBER_FIELDS = ['limit', 'offset'] as const

/**
 * Field of a number of the window.
 */
type NumberField = (typeof NUMBER_FIELDS)[number]

const props = defineProps<{
  /**
   * Element the panel opens beside.
   */
  anchor: HTMLElement
  /**
   * Position of the measure.
   */
  index: number
  /**
   * Configuration, for the measure and its expression.
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
}>()

const emit = defineEmits<{
  /**
   * The user changes the settings.
   */
  change: [concat: PivotConcat]
  /**
   * The user asks for the name of the measure.
   */
  name: []
  /**
   * The user closes the panel.
   */
  close: []
}>()

/**
 * Settings of the measure.
 */
const concat = computed(() => {
  const measure = props.config.measures[props.index]
  return measure === undefined
    ? concatOf({ aggregate: 'group_concat', column: 0 })
    : concatOf(measure)
})

/**
 * Expression of the measure, which names the panel.
 */
const expression = computed(() =>
  elementExpression(
    props.config,
    { zone: 'measures', index: props.index },
    props.names,
    props.types,
  ),
)

/**
 * Keys of the order of the values.
 */
const keys = computed(() => concat.value.order ?? [])

/**
 * Changes some settings.
 *
 * @param changes What changes.
 */
function change(changes: Partial<PivotConcat>): void {
  emit('change', { ...concat.value, ...changes })
}

/**
 * Takes or leaves `DISTINCT`.
 *
 * @param event Change of the box.
 */
function chooseDistinct(event: Event): void {
  change({ distinct: (event.target as HTMLInputElement).checked })
}

/**
 * Columns a key of the order can read.
 */
const columns = computed(() => props.names.map((label, value) => ({ label, value })))

/**
 * Changes the keys of the order.
 *
 * @param order New keys.
 */
function changeOrder(order: PivotConcatKey[]): void {
  change({ order })
}

/**
 * Adds a key: the column of the measure, or the first column that no key reads.
 */
function addKey(): void {
  const used = new Set(keys.value.map(({ column }) => column))
  const measure = props.config.measures[props.index]?.column ?? 0
  const column = used.has(measure) ? props.names.findIndex((_, index) => !used.has(index)) : measure
  changeOrder([...keys.value, { column: Math.max(0, column) }])
}

/**
 * Changes the column of a key.
 *
 * @param position Position of the key.
 * @param column Column chosen.
 */
function chooseKeyColumn(position: number, column: number | undefined): void {
  if (column !== undefined) {
    changeOrder(keys.value.map((key, index) => (index === position ? { ...key, column } : key)))
  }
}

/**
 * Turns the direction of a key over.
 *
 * @param position Position of the key.
 */
function turnKey(position: number): void {
  changeOrder(
    keys.value.map((key, index) =>
      index === position ? { ...key, descending: key.descending !== true } : key,
    ),
  )
}

/**
 * Removes a key.
 *
 * @param position Position of the key.
 */
function removeKey(position: number): void {
  changeOrder(keys.value.filter((_, index) => index !== position))
}

/**
 * Text of a number of the window as the field shows it, empty without one.
 *
 * @param name Field.
 */
function numberText(name: NumberField): string {
  const value = concat.value[name]
  return value === undefined || (name === 'offset' && value === 0) ? '' : String(value)
}

/**
 * Texts being typed: the separator and the numbers of the window.
 */
const drafts = reactive({
  separator: concat.value.separator,
  limit: numberText('limit'),
  offset: numberText('offset'),
})

/**
 * Text of each number when the user last confirmed it, which its refusal reads: the field does not
 * complain during the typing.
 */
const confirmed = reactive({ limit: drafts.limit, offset: drafts.offset })

watch(concat, () => {
  drafts.separator = concat.value.separator
  for (const name of NUMBER_FIELDS) {
    drafts[name] = numberText(name)
    confirmed[name] = drafts[name]
  }
})

/**
 * Reads a number typed.
 *
 * @param text Text typed, trimmed.
 * @returns The number, `undefined` for an empty field, `null` for a refused text.
 */
function readNumber(text: string): number | undefined | null {
  if (text === '') {
    return undefined
  }
  const value = Number(text)
  return /^\d+$/.test(text) && value <= LARGEST ? value : null
}

/**
 * Why the text of a number is refused, `null` otherwise.
 *
 * @param name Field.
 */
function refusal(name: NumberField): string | null {
  return readNumber(confirmed[name]) === null
    ? `Nombre entier de 0 à ${LARGEST.toLocaleString('fr-FR')} attendu`
    : null
}

/**
 * Has Escape been pressed? The field that loses the focus with it applies nothing.
 */
let leaving = false

/**
 * Applies some changes, unless Escape has been pressed or nothing changes.
 *
 * @param changes What changes.
 */
function apply(changes: Partial<PivotConcat>): void {
  if (!leaving && Object.keys(changes).length > 0) {
    change(changes)
  }
}

/**
 * Change of the separator typed: it applies as it is typed.
 *
 * @returns The separator, nothing when it is the same.
 */
function separatorChange(): Partial<PivotConcat> {
  return drafts.separator === concat.value.separator ? {} : { separator: drafts.separator }
}

/**
 * Change of the numbers of the window typed: the settings take those that are not refused.
 * Without `LIMIT`, `OFFSET` leaves, as MariaDB writes it only after a `LIMIT`.
 *
 * @returns Both numbers, nothing when they are the same.
 */
function numbersChange(): Partial<PivotConcat> {
  const typed = (name: NumberField) => {
    const text = drafts[name].trim()
    drafts[name] = text
    confirmed[name] = text
    const value = readNumber(text)
    return value === null ? concat.value[name] : value
  }
  const limit = typed('limit')
  const offset = typed('offset')
  const kept = limit === undefined || offset === 0 ? undefined : offset
  return limit === concat.value.limit && kept === (concat.value.offset || undefined)
    ? {}
    : { limit, offset: kept }
}

/**
 * Confirms the separator, at Enter or on leaving its field.
 */
function confirmSeparator(): void {
  apply(separatorChange())
}

/**
 * Confirms the numbers of the window, at Enter or on leaving their field.
 */
function confirmNumbers(): void {
  apply(numbersChange())
}

/**
 * Closes the panel, after a click outside or Escape: a click outside can remove the field before
 * it loses the focus, so what is being typed applies here, in one change: the settings received
 * follow a change only once the page has drawn it.
 */
function close(): void {
  apply({ ...separatorChange(), ...numbersChange() })
  emit('close')
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
        @escape-key-down="onEscape"
      >
        <label class="flex items-center gap-3 text-base text-fg-strong">
          <input
            :checked="concat.distinct === true"
            type="checkbox"
            class="size-5 accent-primary"
            @change="chooseDistinct"
          />
          Valeurs distinctes (DISTINCT)
        </label>
        <div class="field">
          <span class="field-label" aria-hidden="true">Ordre (ORDER BY)</span>
          <p v-if="keys.length === 0" class="text-sm text-fg-secondary">
            Ordre des lignes du résultat
          </p>
          <div v-for="(key, position) in keys" :key="position" class="flex items-center gap-1">
            <!-- The select draws no element of its own: this column stretches its button. -->
            <div class="flex min-w-0 flex-1 flex-col">
              <ui-select
                :model-value="key.column"
                :items="columns"
                :label="`Colonne de la clef ${position + 1}`"
                @update:model-value="chooseKeyColumn(position, $event)"
              />
            </div>
            <ui-button
              :icon="key.descending ? ArrowDownWideNarrow : ArrowUpNarrowWide"
              variant="ghost"
              small
              :aria-label="
                key.descending
                  ? `Clef ${position + 1} décroissante (DESC)`
                  : `Clef ${position + 1} croissante (ASC)`
              "
              v-tooltip="key.descending ? 'Décroissant (DESC)' : 'Croissant (ASC)'"
              @click="turnKey(position)"
            />
            <ui-button
              :icon="X"
              variant="ghost"
              small
              :aria-label="`Retirer la clef ${position + 1}`"
              @click="removeKey(position)"
            />
          </div>
          <div class="flex">
            <ui-button
              label="Ajouter une clef"
              :icon="Plus"
              variant="ghost"
              small
              :disabled="names.length === 0"
              @click="addKey"
            />
          </div>
        </div>
        <div class="field">
          <span class="field-label" aria-hidden="true">Séparateur (SEPARATOR)</span>
          <ui-input
            v-model="drafts.separator"
            aria-label="Séparateur"
            @blur="confirmSeparator"
            @keydown.enter.prevent="confirmSeparator"
          />
        </div>
        <div class="flex gap-3">
          <div v-for="name in NUMBER_FIELDS" :key="name" class="field min-w-0 flex-1">
            <span class="field-label" aria-hidden="true">{{ name.toUpperCase() }}</span>
            <ui-input
              v-model="drafts[name]"
              inputmode="numeric"
              :aria-label="name.toUpperCase()"
              :aria-invalid="refusal(name) !== null"
              :disabled="name === 'offset' && concat.limit === undefined"
              :placeholder="name === 'limit' ? 'Toutes' : '0'"
              v-tooltip="
                name === 'offset' && concat.limit === undefined
                  ? 'OFFSET suit un LIMIT, comme dans MariaDB'
                  : undefined
              "
              @blur="confirmNumbers"
              @keydown.enter.prevent="confirmNumbers"
            />
          </div>
        </div>
        <p
          v-for="name in NUMBER_FIELDS.filter((field) => refusal(field) !== null)"
          :key="name"
          role="alert"
          class="text-sm text-error"
        >
          {{ name.toUpperCase() }} : {{ refusal(name) }}
        </p>
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
