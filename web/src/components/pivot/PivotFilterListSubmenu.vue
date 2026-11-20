<!--
  Values a filter on a column keeps or leaves out (F-21), in the submenu of its
  menu (`PivotSections.vue`): the list of the values of the column, or of the
  periods of the precision of its dates (`PivotFilterValues.vue`). Each tick
  applies at once.
-->
<script setup lang="ts">
import type { Cell } from '@/api/querySession'

import PivotFilterValues from './PivotFilterValues.vue'

defineProps<{
  /**
   * Name of the column, which names the list.
   */
  label: string
  /**
   * Reads the values of the column, or the periods of the precision of its dates.
   */
  read: () => Promise<Cell[]>
  /**
   * Does the list offer NULL?
   */
  offersNull: boolean
  /**
   * Are the rows of the result still being received?
   */
  partial?: boolean
  /**
   * Values checked.
   */
  kept: Cell[]
}>()

const emit = defineEmits<{
  /**
   * The user checks or unchecks values.
   */
  change: [values: Cell[]]
}>()
</script>

<template>
  <div class="flex w-80 flex-col gap-2">
    <pivot-filter-values
      :label="label"
      :read="read"
      :offers-null="offersNull"
      :partial="partial"
      :kept="kept"
      @update:kept="emit('change', $event)"
    />
  </div>
</template>
