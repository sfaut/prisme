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

  The grid comes right under the line of the result tabs, which takes the
  place of its frame's top border; the error or the durations, when they
  come first, keep a little room under that line.

  The refresh button, right of the result tabs with the grid's filter
  button (`actionsTarget`), runs the query again; during the refresh, or while it waits for the running batch, it
  stops it. The previous result stays displayed until the new columns
  arrive.
-->
<script setup lang="ts">
import { RefreshCw, RefreshCwOff } from '@lucide/vue'
import { computed, ref, watch } from 'vue'

import {
  type QueryTab,
  type RefreshState,
  executionDuration,
  formatBytes,
  formatDuration,
  formatInstant,
  transferDuration,
} from '@/api/querySession'

import { useClock } from './clock'
import ResultGrid from './ResultGrid.vue'
import UiAlert from './ui/UiAlert.vue'
import UiButton from './ui/UiButton.vue'
import UiStatusBar from './ui/UiStatusBar.vue'
import { vTooltip } from './ui/tooltip'

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
  /**
   * Refresh of the tab: running, waiting, possible or not.
   */
  refresh: RefreshState
  /**
   * Place of the grid's buttons (refresh, filter) in the bar of the result
   * tabs, for the tab shown; `null` for the others, hidden, which show none.
   */
  actionsTarget: HTMLElement | null
}>()

const emit = defineEmits<{
  /**
   * The user asks to run the query again.
   */
  refresh: []
  /**
   * The user stops the refresh, or cancels it while it waits.
   */
  'stop-refresh': []
}>()

const clock = useClock()

/**
 * Displayed result: that of the query, or, during a refresh, the previous
 * one until the new columns arrive.
 */
const shown = computed(() => props.tab.result ?? props.tab.previous?.result ?? null)

/**
 * Does the grid come first, right under the result tabs (no error above it)?
 */
const gridFirst = computed(() => !props.tab.error && (shown.value?.columns.length ?? 0) > 0)

/**
 * Is a refresh running or waiting? Its button then stops it.
 */
const refreshing = computed(() => props.refresh === 'running' || props.refresh === 'queued')

/**
 * Hover text of the refresh button.
 */
const refreshTitle = computed(() => {
  switch (props.refresh) {
    case 'running':
      return 'Arrêter le rafraîchissement : le résultat actuel est gardé'
    case 'queued':
      return 'Annuler le rafraîchissement, en attente de la fin des requêtes en cours'
    default:
      return 'Rafraîchir : relance la requête'
  }
})

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
  <div class="flex h-full min-h-0 min-w-0 flex-1 flex-col gap-3" :class="{ 'pt-1': !gridFirst }">
    <ui-alert v-if="tab.error" tone="error" :title="tab.error" />
    <result-grid
      v-if="shown && shown.columns.length > 0"
      :attached="gridFirst"
      :columns="shown.columns"
      :right-aligned="shown.rightAligned"
      :column-types="shown.columnTypes"
      :rows="shown.rows"
      :row-count="shown.rowCount"
      :shown-count="shownCount"
      :sql="tab.sql"
      :displayed="displayed"
      :actions-target="actionsTarget"
    >
      <template #actions>
        <ui-button
          :icon="refreshing ? RefreshCwOff : RefreshCw"
          variant="ghost"
          :tone="refreshing ? 'primary' : 'neutral'"
          small
          :disabled="refresh === 'unavailable'"
          :aria-label="refreshing ? 'Arrêter le rafraîchissement' : 'Rafraîchir'"
          v-tooltip="refreshTitle"
          @click="refreshing ? emit('stop-refresh') : emit('refresh')"
        />
      </template>
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
