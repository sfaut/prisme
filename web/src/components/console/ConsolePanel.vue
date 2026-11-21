<!--
  Console, at the bottom of an editor's tab, under its side panels: what
  Prisme does besides the results, in tabs, right of its title. The History
  tab lists the queries sent to the target servers by the session of the
  editor (`ConsoleHistoryTable.vue`); the Debug tab, the lines of the
  server's log written for it (`ConsoleDebugTable.vue`). "Tous les éditeurs"
  shows those of all the editors instead, with the queries Prisme sends
  itself, the rest of the server's log and the notifications of the
  interface (`console` store). A button, at the right of the tabs, clears
  the list of the tab shown, whatever its filter: the elements of the editor,
  or all of them with "Tous les éditeurs";
  another one, before it, shows or hides the filters of both tabs, and
  takes the primary color while the filter of the tab shown keeps only some
  elements, hidden or not. The console opens and closes from the activity
  bar of the editor's tab (`PanelActivityBar.vue`); its height is set by
  dragging the line above it (the splitter of `App.vue`). The console of the
  active editor alone is mounted, which shows its lists (`follow`).
-->
<script setup lang="ts">
import { Funnel, MessageCircleOff } from '@lucide/vue'
import { TabsContent, TabsList, TabsRoot, TabsTrigger } from 'reka-ui'
import { computed, ref } from 'vue'

import { type ConsoleTab, isFiltering, useConsoleStore } from '@/stores/console'

import ConsoleDebugTable from './ConsoleDebugTable.vue'
import ConsoleHistoryTable from './ConsoleHistoryTable.vue'
import PanelHeader from '@/components/panel/PanelHeader.vue'
import UiButton from '@/components/ui/UiButton.vue'
import { vTooltip } from '@/components/ui/tooltip'

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
const shown = computed(() => {
  const whose = store.scope === null ? 'de tous les éditeurs' : 'de l’éditeur'
  const scopeNote = store.scope === null ? 'De tous les éditeurs' : 'De l’éditeur'
  return store.tab === 'history'
    ? {
        list: store.history,
        filter: store.historyFilter,
        clearLabel: `Effacer l’historique ${whose}`,
        clearTip: { title: 'Effacer l’historique', description: scopeNote },
      }
    : {
        list: store.debug,
        filter: store.debugFilter,
        clearLabel: `Effacer le debug ${whose}`,
        clearTip: { title: 'Effacer le debug', description: scopeNote },
      }
})

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
    `${store.filtersShown ? 'Masquer les filtres' : 'Filtrer'}${filtering.value ? ' (filtres actifs)' : ''}`,
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
</script>

<template>
  <tabs-root v-model="store.tab" class="flex h-full min-h-0 flex-col gap-1" aria-label="Console">
    <panel-header title="Console">
      <template #inline>
        <!-- Tabs of the look of the editor and result tabs (`tab-pill`). -->
        <tabs-list class="flex items-center gap-2" aria-label="Console">
          <tabs-trigger
            v-for="tab in TABS"
            :key="tab.value"
            :value="tab.value"
            class="tab-pill h-8 px-3 text-base"
          >
            {{ tab.label }}
          </tabs-trigger>
        </tabs-list>
      </template>
      <template #actions>
        <label
          class="flex items-center gap-2 text-base text-fg-secondary"
          v-tooltip="{
            title: 'Afficher tous les éditeurs',
            description: 'Avec les éléments de Prisme hors des éditeurs',
          }"
        >
          <input v-model="store.allEditors" type="checkbox" class="size-5 accent-primary" />
          Tous les éditeurs
        </label>
        <ui-button
          :icon="Funnel"
          variant="ghost"
          :tone="filtering ? 'primary' : 'neutral'"
          small
          :class="{ 'bg-surface-raised': store.filtersShown }"
          :aria-pressed="store.filtersShown"
          :aria-label="filtersLabel"
          v-tooltip="{
            title: store.filtersShown ? 'Masquer les filtres' : 'Filtrer',
            description: filtering ? 'Filtres actifs' : '',
          }"
          @click="store.filtersShown = !store.filtersShown"
        />
        <ui-button
          :icon="MessageCircleOff"
          variant="ghost"
          small
          :loading="clearing"
          :disabled="nothingToClear"
          :aria-label="shown.clearLabel"
          v-tooltip="shown.clearTip"
          @click="clearShown"
        />
      </template>
    </panel-header>
    <!-- The scrolling area of each tab is the content of the tab itself. -->
    <tabs-content value="history" as-child>
      <console-history-table />
    </tabs-content>
    <tabs-content value="debug" as-child>
      <console-debug-table />
    </tabs-content>
  </tabs-root>
</template>
