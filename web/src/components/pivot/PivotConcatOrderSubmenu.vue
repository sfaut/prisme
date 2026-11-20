<!--
  Order of the values of a `GROUP_CONCAT` (`ORDER BY`), in the submenu of its
  measure (`PivotSections.vue`): its keys, each on any column of the result,
  ascending or descending. Without a key, the values follow the order of the
  rows of the result. Each change applies at once.
-->
<script setup lang="ts">
import { ArrowDownWideNarrow, ArrowUpNarrowWide, Plus, X } from '@lucide/vue'
import { computed } from 'vue'

import type { PivotConcatKey } from '@/api/generated/PivotConcatKey'

import UiButton from '@/components/ui/UiButton.vue'
import UiSelect from '@/components/ui/UiSelect.vue'
import { vTooltip } from '@/components/ui/tooltip'

const props = defineProps<{
  /**
   * Keys of the order, the first one first.
   */
  order: PivotConcatKey[]
  /**
   * Names of the columns of the result.
   */
  names: string[]
  /**
   * Column of the measure, which the first key reads.
   */
  column: number
}>()

const emit = defineEmits<{
  /**
   * The user changes the keys of the order.
   */
  change: [order: PivotConcatKey[]]
}>()

/**
 * Columns a key can read.
 */
const columns = computed(() => props.names.map((label, value) => ({ label, value })))

/**
 * Adds a key: the column of the measure, or the first column that no key reads.
 */
function addKey(): void {
  const used = new Set(props.order.map(({ column }) => column))
  const column = used.has(props.column)
    ? props.names.findIndex((_, index) => !used.has(index))
    : props.column
  emit('change', [...props.order, { column: Math.max(0, column) }])
}

/**
 * Changes the column of a key.
 *
 * @param position Position of the key.
 * @param column Column chosen.
 */
function chooseColumn(position: number, column: number | undefined): void {
  if (column !== undefined) {
    emit(
      'change',
      props.order.map((key, index) => (index === position ? { ...key, column } : key)),
    )
  }
}

/**
 * Turns the direction of a key over.
 *
 * @param position Position of the key.
 */
function turnKey(position: number): void {
  emit(
    'change',
    props.order.map((key, index) =>
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
  emit(
    'change',
    props.order.filter((_, index) => index !== position),
  )
}
</script>

<template>
  <div class="flex w-80 flex-col gap-1">
    <p v-if="order.length === 0" class="text-sm text-fg-secondary">Ordre des lignes du résultat</p>
    <div v-for="(key, position) in order" :key="position" class="flex items-center gap-1">
      <ui-select
        :model-value="key.column"
        :items="columns"
        :label="`Colonne de la clef ${position + 1}`"
        class="min-w-0 flex-1"
        @update:model-value="chooseColumn(position, $event)"
      />
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
</template>
