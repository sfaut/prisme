<!--
  List of the distinct values of a column, in the order of the grid, or of the
  periods of a precision of its dates, in a filter of the Pivoteur (F-21): a
  search, the values to check, and "Tout" and "Aucun" on the values shown. Each
  tick applies at once; a value of the filter the column no longer holds leaves
  at the next tick. The values can be many: the list draws those in sight
  (`ListboxVirtualizer`).
-->
<script setup lang="ts">
import { Check } from '@lucide/vue'
import {
  ListboxContent,
  ListboxFilter,
  ListboxItem,
  ListboxItemIndicator,
  ListboxRoot,
  ListboxVirtualizer,
  useFilter,
} from 'reka-ui'
import { computed, ref, shallowRef } from 'vue'

import type { Cell } from '@/api/querySession'
import { cssPixels } from '@/cssLength'

import { NULL_TEXT } from './resultGrid'
import { INPUT_FIELD } from './ui/inputField'
import { MENU_ENTRY } from './ui/menuItem'
import UiButton from './ui/UiButton.vue'

/**
 * Height of a value, in pixels (`--row-height` of the theme).
 *
 * Fixed size: `estimate-size` of Reka UI's `ListboxVirtualizer` only takes
 * pixels, so this theme length is read once (`docs/DEVELOPER.md`, section
 * "Lengths in pixels").
 */
const ROW_HEIGHT = cssPixels('--row-height', 32)

const props = defineProps<{
  /**
   * Name of the column.
   */
  label: string
  /**
   * Reads the distinct values of the column.
   */
  read: () => Promise<Cell[]>
  /**
   * Does the list offer NULL? `NOT IN` leaves it out: as in SQL, a NULL in its
   * list would keep no row.
   */
  offersNull: boolean
}>()

/**
 * Values the filter lists.
 */
const kept = defineModel<Cell[]>('kept', { required: true })

/**
 * Distinct values of the column, `null` until they are read.
 */
const columnValues = shallowRef<Cell[] | null>(null)

/**
 * Could the values not be read?
 */
const failed = ref(false)

props.read().then(
  (values) => (columnValues.value = values),
  () => (failed.value = true),
)

/**
 * Values the list offers.
 */
const values = computed(() =>
  (columnValues.value ?? []).filter((value) => props.offersNull || value !== null),
)

/**
 * Searched text.
 */
const search = ref('')

const { contains } = useFilter({ sensitivity: 'base' })

/**
 * Text of a value of the list.
 *
 * @param index Position of the value.
 */
function text(index: number): string {
  return values.value[index] ?? NULL_TEXT
}

/**
 * Positions of the values that hold the searched text. A list of numbers: NULL
 * could not be a value of the listbox.
 */
const shown = computed(() =>
  values.value.flatMap((_, index) => (contains(text(index), search.value) ? [index] : [])),
)

/**
 * Positions of the values the filter lists.
 */
const checked = computed(() => {
  const keptValues = new Set(kept.value)
  return values.value.flatMap((value, index) => (keptValues.has(value) ? [index] : []))
})

/**
 * Keeps some values, in the order of the list: the same values give the same
 * request.
 *
 * @param positions Positions of the values.
 */
function keep(positions: number[]): void {
  kept.value = [...positions]
    .sort((a, b) => a - b)
    .map((position) => values.value[position] ?? null)
}

/**
 * Checks or unchecks all the values shown.
 *
 * @param check Check them?
 */
function checkShown(check: boolean): void {
  const positions = new Set(checked.value)
  for (const position of shown.value) {
    if (check) {
      positions.add(position)
    } else {
      positions.delete(position)
    }
  }
  keep([...positions])
}
</script>

<template>
  <p v-if="failed" role="alert" class="py-1 text-error">Les valeurs n’ont pas pu être lues</p>
  <p v-else-if="columnValues === null" role="status" class="py-1 text-fg-secondary">
    Lecture des valeurs…
  </p>
  <template v-else>
    <listbox-root
      multiple
      highlight-on-hover
      :model-value="checked"
      class="flex min-h-0 flex-col gap-2"
      @update:model-value="keep($event as number[])"
    >
      <listbox-filter
        v-model="search"
        auto-focus
        :class="INPUT_FIELD"
        placeholder="Rechercher une valeur"
        aria-label="Rechercher une valeur"
      />
      <listbox-content class="max-h-64 overflow-y-auto" :aria-label="`Valeurs de ${label}`">
        <listbox-virtualizer
          v-slot="{ option }"
          :options="shown"
          :estimate-size="ROW_HEIGHT"
          :text-content="(option) => text(option as number)"
        >
          <listbox-item :value="option" :class="[MENU_ENTRY, 'w-full text-fg']">
            <span class="flex size-5 shrink-0 items-center justify-center" aria-hidden="true">
              <listbox-item-indicator>
                <Check class="size-5" />
              </listbox-item-indicator>
            </span>
            <span
              class="min-w-0 flex-1 truncate"
              :class="{ null: values[option as number] === null }"
            >
              {{ text(option as number) }}
            </span>
          </listbox-item>
        </listbox-virtualizer>
      </listbox-content>
      <p v-if="shown.length === 0" class="py-1 text-fg-secondary">Aucune valeur</p>
    </listbox-root>
    <div class="flex justify-end gap-2">
      <ui-button label="Tout" variant="ghost" small @click="checkShown(true)" />
      <ui-button label="Aucun" variant="ghost" small @click="checkShown(false)" />
    </div>
  </template>
</template>

<style scoped>
@reference "@/assets/main.css";

/* NULL in small capitals, as in the grid. */
.null {
  color: var(--color-fg-faint);
  font-style: italic;
  font-variant-caps: all-small-caps;
}
</style>
