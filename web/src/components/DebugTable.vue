<!--
  Debug tab of the console: the lines of the server's log, those its
  terminal shows, and the notifications of the interface, with their
  number, time, level, source and message. Above, on request, the levels
  and sources kept (`ConsoleFilter.vue`). Scrolling and older pages:
  `ConsoleList.vue`.
-->
<script setup lang="ts">
import { isFiltering, useConsoleStore } from '@/stores/console'

import ConsoleFilter from './ConsoleFilter.vue'
import ConsoleList from './ConsoleList.vue'
import { LEVEL_LABELS, LEVEL_TONES, SOURCE_LABELS, dateTimeOf, timeOf } from './consoleFormat'
import { vTooltip } from './ui/tooltip'

const store = useConsoleStore()
const debug = store.debug
</script>

<template>
  <div class="flex min-h-0 flex-1 flex-col gap-1">
    <div v-if="store.filtersShown" class="flex flex-wrap items-center gap-4">
      <console-filter v-model="store.debugFilter.level" label="Niveau" :labels="LEVEL_LABELS" />
      <console-filter v-model="store.debugFilter.source" label="Source" :labels="SOURCE_LABELS" />
    </div>
    <console-list
      :list="debug"
      :empty="
        isFiltering(store.debugFilter)
          ? 'Aucun message ne correspond aux filtres'
          : 'Aucun message pour le moment'
      "
    >
      <table class="console-table">
        <colgroup>
          <col class="index" />
          <col class="time" />
          <col class="level" />
          <col class="source" />
          <col />
        </colgroup>
        <thead>
          <tr>
            <th class="end">#</th>
            <th>Heure</th>
            <th>Niveau</th>
            <th>Source</th>
            <th>Message</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="entry in debug.entries" :key="entry.id">
            <td class="end text-fg-faint">{{ entry.id }}</td>
            <td v-tooltip="dateTimeOf(entry.logged_at)">{{ timeOf(entry.logged_at) }}</td>
            <td :class="LEVEL_TONES[entry.level]">{{ LEVEL_LABELS[entry.level] }}</td>
            <td v-tooltip="entry.target || undefined">{{ SOURCE_LABELS[entry.source] }}</td>
            <td class="font-mono text-fg-strong" v-tooltip="entry.message">
              {{ entry.message }}
            </td>
          </tr>
        </tbody>
      </table>
    </console-list>
  </div>
</template>

<style scoped>
@reference "@/assets/main.css";

/* Columns sized to their content, in characters, plus the padding of the
   cells; the message takes the rest. */
col.index {
  inline-size: calc(6ch + 2 * --spacing(3));
}

col.time {
  inline-size: calc(9ch + 2 * --spacing(3));
}

col.level {
  inline-size: calc(14ch + 2 * --spacing(3));
}

col.source {
  inline-size: calc(10ch + 2 * --spacing(3));
}
</style>
