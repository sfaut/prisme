<!--
  List of the distinct values of a column, in the order of the grid, which a
  filter of the Pivoteur keeps (F-21), in a popover beside its element: a
  search, the values to check, and "Tout" and "Aucun" on the values shown. Each
  tick applies at once; a value of the filter the column no longer holds leaves
  at the next tick. Escape or a click outside closes the list. The values can be
  many: the list draws those in sight (`ListboxVirtualizer`).
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
  PopoverAnchor,
  PopoverContent,
  PopoverPortal,
  PopoverRoot,
  useFilter,
} from 'reka-ui'
import { computed, ref, shallowRef } from 'vue'

import type { Cell } from '@/api/querySession'
import { cssPixels } from '@/cssLength'

import { NULL_TEXT } from './resultGrid'
import { INPUT_FIELD } from './ui/inputField'
import { MENU_ENTRY, MENU_PANEL } from './ui/menuItem'
import UiButton from './ui/UiButton.vue'

/**
 * Gap with the element, in pixels (`--popup-offset` of the theme).
 *
 * Fixed size: Reka UI's `side-offset` only takes pixels, so this theme
 * length is read once (`docs/DEVELOPER.md`, section "Lengths in pixels").
 */
const OFFSET = cssPixels('--popup-offset', 4)

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
   * Element the list opens beside.
   */
  anchor: HTMLElement
  /**
   * Name of the column.
   */
  label: string
  /**
   * Reads the distinct values of the column.
   */
  read: () => Promise<Cell[]>
}>()

const emit = defineEmits<{
  /**
   * The user closes the list.
   */
  close: []
}>()

/**
 * Values the filter keeps.
 */
const kept = defineModel<Cell[]>('kept', { required: true })

/**
 * Distinct values of the column, `null` until they are read.
 */
const values = shallowRef<Cell[] | null>(null)

/**
 * Could the values not be read?
 */
const failed = ref(false)

props.read().then(
  (read) => (values.value = read),
  () => (failed.value = true),
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
  return values.value?.[index] ?? NULL_TEXT
}

/**
 * Positions of the values that hold the searched text. A list of numbers: NULL
 * could not be a value of the listbox.
 */
const shown = computed(() =>
  (values.value ?? []).flatMap((_, index) => (contains(text(index), search.value) ? [index] : [])),
)

/**
 * Positions of the values the filter keeps.
 */
const checked = computed(() => {
  const keptValues = new Set(kept.value)
  return (values.value ?? []).flatMap((value, index) => (keptValues.has(value) ? [index] : []))
})

/**
 * Keeps some values, in the order of the list: the same values give the same
 * request.
 *
 * @param positions Positions of the values.
 */
function keep(positions: number[]): void {
  const list = values.value ?? []
  kept.value = [...positions].sort((a, b) => a - b).map((position) => list[position] ?? null)
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
  <popover-root :open="true" @update:open="emit('close')">
    <popover-anchor :reference="anchor" />
    <popover-portal>
      <popover-content
        side="right"
        align="start"
        :side-offset="OFFSET"
        :collision-padding="OFFSET"
        :class="[MENU_PANEL, 'flex w-80 flex-col gap-2 p-2']"
        :aria-label="`Valeurs de ${label}`"
      >
        <p v-if="failed" role="alert" class="px-2 py-1 text-error">
          Les valeurs n’ont pas pu être lues
        </p>
        <p v-else-if="values === null" role="status" class="px-2 py-1 text-fg-secondary">
          Lecture des valeurs…
        </p>
        <listbox-root
          v-else
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
          <p v-if="shown.length === 0" class="px-2 py-1 text-fg-secondary">Aucune valeur</p>
        </listbox-root>
        <div v-if="values !== null" class="flex justify-end gap-2">
          <ui-button label="Tout" variant="ghost" small @click="checkShown(true)" />
          <ui-button label="Aucun" variant="ghost" small @click="checkShown(false)" />
        </div>
      </popover-content>
    </popover-portal>
  </popover-root>
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
