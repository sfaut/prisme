<!--
  Content of a query's tab: error if any, result grid and, under the grid,
  the number of loaded rows and two durations: the execution time of the
  query by the server (until the columns arrive), then the transfer and
  display time of the rows, followed by the received volume and the
  throughput.

  During the query, the number of rows, the durations and the volume change once
  per second (shared clock, `clock.ts`); at the end, they take their final
  value at once. A query without a grid (waiting, failed, or without
  columns) only shows its durations.
-->
<script setup lang="ts">
import { computed, ref, watch } from 'vue'

import {
  type QueryTab,
  executionDuration,
  formatBytes,
  formatDuration,
  transferDuration,
} from '@/api/querySession'

import { useClock } from './clock'
import ResultGrid from './ResultGrid.vue'
import UiAlert from './ui/UiAlert.vue'

const props = defineProps<{
  /**
   * Displayed query.
   */
  tab: QueryTab
  /**
   * Is the tab on screen (active tab of the active editor)? Its grid then
   * feeds the data explorer.
   */
  displayed: boolean
}>()

const clock = useClock()

/**
 * Number of rows shown under the grid: read on each tick of the clock, and
 * at the end of the query.
 */
const shownCount = ref(props.tab.result?.rowCount ?? 0)

/**
 * Received volume shown under the grid, in bytes, read like the number of rows.
 */
const shownBytes = ref(props.tab.bytes)

watch(
  [clock, () => props.tab.done, () => props.tab.result],
  () => {
    shownCount.value = props.tab.result?.rowCount ?? 0
    shownBytes.value = props.tab.bytes
  },
  { immediate: true },
)

/**
 * Durations of the query, formatted: "exécution 120 ms · transfert 2,4 s ·
 * 3,1 Mo · 1,3 Mo/s" (received volume and throughput during the transfer);
 * empty while it waits for its turn. The clock is only read during the
 * query: an ended query is no longer redrawn on each tick.
 */
const durations = computed(() => {
  const running = props.tab.startedAt !== null && !props.tab.done
  const now = running ? clock.value : performance.now()
  const execution = executionDuration(props.tab, now)
  const transfer = transferDuration(props.tab, now)
  const volume = transfer === null ? null : formatBytes(shownBytes.value)
  const rate =
    transfer === null || transfer <= 0
      ? null
      : `${formatBytes((shownBytes.value * 1000) / transfer)}/s`
  return [
    execution === null ? null : `exécution ${formatDuration(execution)}`,
    transfer === null ? null : `transfert ${formatDuration(transfer)}`,
    volume,
    rate,
  ]
    .filter((part) => part !== null)
    .join(' · ')
})
</script>

<template>
  <!--
    min-w-0: without it, the tab (flex item of a row) never shrinks below the
    width of its content, and a wide grid overflowed, cut off on the right,
    instead of scrolling in its own area.
  -->
  <div class="flex h-full min-h-0 min-w-0 flex-1 flex-col gap-gap">
    <ui-alert v-if="tab.error" tone="error" :title="tab.error" />
    <result-grid
      v-if="tab.result && tab.result.columns.length > 0"
      :columns="tab.result.columns"
      :right-aligned="tab.result.rightAligned"
      :column-types="tab.result.columnTypes"
      :rows="tab.result.rows"
      :row-count="tab.result.rowCount"
      :shown-count="shownCount"
      :sql="tab.sql"
      :displayed="displayed"
    >
      <template #footer>
        <template v-if="durations">&nbsp;· {{ durations }}</template>
      </template>
    </result-grid>
    <div v-else-if="durations" class="text-trim text-fg-muted">{{ durations }}</div>
  </div>
</template>
