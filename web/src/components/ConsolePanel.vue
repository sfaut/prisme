<!--
  Console, at the bottom of the main area, between the side panels: what
  Prisme does besides the results, in tabs. The History tab lists all the
  queries sent to the target servers, by the editors and by Prisme itself
  (`HistoryTable.vue`). The console folds towards the bottom edge, where it
  leaves a rail (`PanelRail.vue`); its height is set by dragging its top
  edge (`PanelResizer.vue`, in `App.vue`).
-->
<script setup lang="ts">
import { TabsContent, TabsList, TabsRoot, TabsTrigger } from 'reka-ui'
import { onMounted, onUnmounted } from 'vue'

import { useConsoleStore } from '@/stores/console'

import HistoryTable from './HistoryTable.vue'
import PanelHeader from './PanelHeader.vue'

const store = useConsoleStore()

onMounted(() => store.start())
onUnmounted(() => store.dispose())
</script>

<template>
  <tabs-root v-model="store.tab" class="flex h-full min-h-0 flex-col gap-1" aria-label="Console">
    <panel-header title="Console" side="bottom" @collapse="store.open = false">
      <tabs-list class="flex items-center gap-1" aria-label="Console">
        <tabs-trigger
          value="history"
          class="flex h-control-sm items-center rounded-control px-control-x text-ui font-medium text-fg-muted outline-none select-none hover:text-fg-strong focus-visible:bg-surface-muted data-[state=active]:bg-surface-elevated data-[state=active]:text-fg-strong"
        >
          Historique
        </tabs-trigger>
      </tabs-list>
    </panel-header>
    <tabs-content value="history" class="flex min-h-0 flex-1 flex-col outline-none">
      <history-table />
    </tabs-content>
  </tabs-root>
</template>
