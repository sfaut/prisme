<!--
  Content of a query's tab: error if any, result grid and, under the grid,
  the number of loaded rows and two durations: the execution time of the
  query by the server (until the columns arrive), then the transfer and
  display time of the rows, followed by the received volume, the
  throughput and the end of the execution (date and time to the second,
  the instant the result reflects).

  During the query, the number of rows, the durations and the volume change once
  per second (shared clock, `clock.ts`); at the end, they take their final
  value at once. A query without a grid (waiting, failed, or without
  columns) only shows its durations.

  The user filter belongs to the tab (`filter`, `v-model:filter`): its form
  (`ResultFilter.vue`) shows above the grid while its button, right of the
  result tabs, keeps it expanded. New columns make it global, its text kept:
  the index of a targeted column would point to another column.

  The grid comes right under the line of the result tabs, which takes the
  place of its frame's top border; the error, the filter's form or the
  durations, when they come first, keep a little room under that line.

  During a refresh, the previous result stays displayed until the new
  columns arrive.
-->
<script setup lang="ts">
import { computed, ref, watch } from 'vue'

import {
  type QueryTab,
  type UserFilter,
  executionDuration,
  formatBytes,
  formatDuration,
  formatInstant,
  shownResult,
  showsGrid,
  transferDuration,
} from '@/api/querySession'

import { useClock } from './clock'
import { sameColumns } from './resultGrid'
import ResultFilter from './ResultFilter.vue'
import ResultGrid from './ResultGrid.vue'
import UiAlert from './ui/UiAlert.vue'
import UiStatusBar from './ui/UiStatusBar.vue'

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

/**
 * User filter of the tab's grid.
 */
const filter = defineModel<UserFilter>('filter', { required: true })

const clock = useClock()

/**
 * Displayed result (see `shownResult`).
 */
const shown = computed(() => shownResult(props.tab))

/**
 * Does the grid come first, right under the result tabs (no error nor
 * filter form above it)?
 */
const attached = computed(() => !props.tab.error && !props.tab.filterOpen && showsGrid(props.tab))

// New columns: a targeted filter becomes global, its text kept. The same
// columns again (a session resumed after a reconnection) keep it.
watch(
  () => shown.value?.columns,
  (columns, previous) => {
    if (
      columns !== undefined &&
      previous !== undefined &&
      !sameColumns(columns, previous) &&
      filter.value.column !== null
    ) {
      filter.value = { ...filter.value, column: null }
    }
  },
)

/**
 * Number of rows shown under the grid: read on each tick of the clock, and
 * at the end of the query.
 */
const shownCount = ref(shown.value?.rowCount ?? 0)

/**
 * Received volume shown under the grid, in bytes, read like the number of rows.
 */
const shownBytes = ref(props.tab.bytes)

watch(
  [clock, () => props.tab.done, shown],
  () => {
    shownCount.value = shown.value?.rowCount ?? 0
    shownBytes.value = props.tab.bytes
  },
  { immediate: true },
)

/**
 * Durations of the query, formatted, one item each of the status bar:
 * "exécution 120 ms", "transfert 2,4 s", "3,1 Mo", "1,3 Mo/s", "20/10/2026
 * 14:32:05" (received volume and throughput during the transfer, then end of
 * the execution, to the second); none while it waits for its turn. The clock is only read during the query: an ended
 * query is no longer redrawn on each tick.
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
    shown.value?.executedAt ? formatInstant(shown.value.executedAt) : null,
  ].filter((part) => part !== null)
})
</script>

<template>
  <!--
    min-w-0: without it, the tab (flex item of a row) never shrinks below the
    width of its content, and a wide grid overflowed, cut off on the right,
    instead of scrolling in its own area.
  -->
  <div class="flex h-full min-h-0 min-w-0 flex-1 flex-col gap-3" :class="{ 'pt-1': !attached }">
    <ui-alert v-if="tab.error" tone="error" :title="tab.error" />
    <result-filter
      v-if="shown && showsGrid(tab) && tab.filterOpen"
      v-model="filter"
      :columns="shown.columns"
    />
    <result-grid
      v-if="shown && showsGrid(tab)"
      :attached="attached"
      :filter="filter"
      :columns="shown.columns"
      :right-aligned="shown.rightAligned"
      :column-types="shown.columnTypes"
      :rows="shown.rows"
      :row-count="shown.rowCount"
      :shown-count="shownCount"
      :sql="tab.sql"
      :displayed="displayed"
    >
      <template #status>
        <span v-for="duration in durations" :key="duration">{{ duration }}</span>
      </template>
    </result-grid>
    <!-- Without a grid, the durations alone. -->
    <ui-status-bar v-else-if="durations.length > 0">
      <span v-for="duration in durations" :key="duration">{{ duration }}</span>
    </ui-status-bar>
  </div>
</template>
