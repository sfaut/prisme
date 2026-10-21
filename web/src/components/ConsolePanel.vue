<!--
  Console, at the bottom of the main area, between the side panels: what
  Prisme does besides the results, in tabs. The History tab lists all the
  queries sent to the target servers, by the editors and by Prisme itself
  (`HistoryTable.vue`); the Debug tab, the lines of the server's log and the
  notifications of the interface (`DebugTable.vue`). A button, at the right
  of the tabs, clears the list of the tab shown. The console folds
  towards the bottom edge, where it leaves a rail (`PanelRail.vue`); its
  height is set by dragging its top edge (`PanelResizer.vue`, in
  `App.vue`).
-->
<script setup lang="ts">
import { MessageCircleOff } from 'lucide-vue-next'
import { TabsContent, TabsList, TabsRoot, TabsTrigger } from 'reka-ui'
import { computed, onMounted, onUnmounted, ref } from 'vue'

import { type ConsoleTab, useConsoleStore } from '@/stores/console'

import DebugTable from './DebugTable.vue'
import HistoryTable from './HistoryTable.vue'
import PanelHeader from './PanelHeader.vue'
import UiButton from './ui/UiButton.vue'

const store = useConsoleStore()

/**
 * Tabs of the console.
 */
const TABS: { value: ConsoleTab; label: string }[] = [
  { value: 'history', label: 'Historique' },
  { value: 'debug', label: 'Debug' },
]

/**
 * List of the tab shown, and the label of the button that clears it.
 */
const shown = computed(() =>
  store.tab === 'history'
    ? { list: store.history, clearLabel: 'Effacer l’historique' }
    : { list: store.debug, clearLabel: 'Effacer le debug' },
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
          class="flex h-control-sm items-center rounded-control px-2 text-ui font-medium text-fg-muted outline-none select-none hover:text-fg-strong focus-visible:bg-surface-muted data-[state=active]:bg-surface-elevated data-[state=active]:text-fg-strong"
        >
          {{ tab.label }}
        </tabs-trigger>
      </tabs-list>
      <template #actions>
        <ui-button
          :icon="MessageCircleOff"
          variant="ghost"
          small
          :loading="clearing"
          :disabled="shown.list.entries.length === 0 && !shown.list.hasOlder"
          :aria-label="shown.clearLabel"
          :title="shown.clearLabel"
          @click="clearShown"
        />
      </template>
    </panel-header>
    <tabs-content value="history" class="flex min-h-0 flex-1 flex-col outline-none">
      <history-table />
    </tabs-content>
    <tabs-content value="debug" class="flex min-h-0 flex-1 flex-col outline-none">
      <debug-table />
    </tabs-content>
  </tabs-root>
</template>
