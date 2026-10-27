<!--
  Console, at the bottom of the main area, between the side panels: what
  Prisme does besides the results, in tabs. The History tab lists all the
  queries sent to the target servers, by the editors and by Prisme itself
  (`HistoryTable.vue`); the Debug tab, the lines of the server's log and the
  notifications of the interface (`DebugTable.vue`). A button, at the right
  of the tabs, clears the list of the tab shown, whatever its filter;
  another one, before it, shows or hides the filters of both tabs, and
  takes the accent color while the filter of the tab shown keeps only some
  elements, hidden or not. The console folds towards the bottom edge, where
  it leaves a rail (`PanelRail.vue`); its height is set by dragging the line
  above it (the splitter of `App.vue`).
-->
<script setup lang="ts">
import { Funnel, MessageCircleOff } from '@lucide/vue'
import { TabsContent, TabsList, TabsRoot, TabsTrigger } from 'reka-ui'
import { computed, onMounted, onUnmounted, ref } from 'vue'

import { type ConsoleTab, isFiltering, useConsoleStore } from '@/stores/console'

import DebugTable from './DebugTable.vue'
import HistoryTable from './HistoryTable.vue'
import PanelHeader from './PanelHeader.vue'
import UiButton from './ui/UiButton.vue'
import { vTooltip } from './ui/tooltip'

const store = useConsoleStore()

/**
 * Tabs of the console.
 */
const TABS: { value: ConsoleTab; label: string }[] = [
  { value: 'history', label: 'Historique' },
  { value: 'debug', label: 'Debug' },
]

/**
 * List of the tab shown, its filter, and the label of the button that
 * clears it.
 */
const shown = computed(() =>
  store.tab === 'history'
    ? { list: store.history, filter: store.historyFilter, clearLabel: 'Effacer l’historique' }
    : { list: store.debug, filter: store.debugFilter, clearLabel: 'Effacer le debug' },
)

/**
 * Does the filter of the tab shown keep only some elements?
 */
const filtering = computed(() => isFiltering(shown.value.filter))

/**
 * Is the list of the tab shown empty, on the server too? A filtered list
 * that shows nothing may hold elements outside its filter.
 */
const nothingToClear = computed(
  () => shown.value.list.entries.length === 0 && !shown.value.list.hasOlder && !filtering.value,
)

/**
 * Label of the button that shows or hides the filters.
 */
const filtersLabel = computed(
  () =>
    `${store.filtersShown ? 'Masquer' : 'Afficher'} les filtres${filtering.value ? ' (filtres actifs)' : ''}`,
)

/**
 * Is a list being cleared?
 */
const clearing = ref(false)

/**
 * Clears the list of the tab shown.
 */
async function clearShown(): Promise<void> {
  clearing.value = true
  try {
    await shown.value.list.clear()
  } finally {
    clearing.value = false
  }
}

onMounted(() => store.start())
onUnmounted(() => store.dispose())
</script>

<template>
  <tabs-root v-model="store.tab" class="flex h-full min-h-0 flex-col gap-1" aria-label="Console">
    <panel-header title="Console" side="bottom" @collapse="store.open = false">
      <tabs-list class="flex items-center gap-1" aria-label="Console">
        <tabs-trigger
          v-for="tab in TABS"
          :key="tab.value"
          :value="tab.value"
          class="flex h-control-sm items-center rounded-control px-1 text-small font-medium text-fg-muted select-none hover:text-fg-strong data-[state=active]:bg-surface-elevated data-[state=active]:text-fg-strong"
        >
          {{ tab.label }}
        </tabs-trigger>
      </tabs-list>
      <template #actions>
        <ui-button
          :icon="Funnel"
          variant="ghost"
          :tone="filtering ? 'accent' : 'neutral'"
          small
          :class="{ 'bg-surface-elevated': store.filtersShown }"
          :aria-pressed="store.filtersShown"
          :aria-label="filtersLabel"
          v-tooltip="filtersLabel"
          @click="store.filtersShown = !store.filtersShown"
        />
        <ui-button
          :icon="MessageCircleOff"
          variant="ghost"
          small
          :loading="clearing"
          :disabled="nothingToClear"
          :aria-label="shown.clearLabel"
          v-tooltip="shown.clearLabel"
          @click="clearShown"
        />
      </template>
    </panel-header>
    <!-- The scrolling area of each tab is the content of the tab itself. -->
    <tabs-content value="history" as-child>
      <history-table />
    </tabs-content>
    <tabs-content value="debug" as-child>
      <debug-table />
    </tabs-content>
  </tabs-root>
</template>
