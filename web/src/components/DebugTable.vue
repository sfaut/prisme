<!--
  Debug tab of the console: the lines of the server's log, those its
  terminal shows, and the notifications of the interface, with their
  number, time, level, source and message. Scrolling and older pages:
  `ConsoleList.vue`.
-->
<script setup lang="ts">
import { useConsoleStore } from '@/stores/console'

import ConsoleList from './ConsoleList.vue'
import { LEVEL_LABELS, LEVEL_TONES, SOURCE_LABELS, dateTimeOf, timeOf } from './consoleFormat'

const debug = useConsoleStore().debug
</script>

<template>
  <console-list :list="debug" empty="Aucun message pour le moment">
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
          <td class="end text-fg-dimmed">{{ entry.id }}</td>
          <td :title="dateTimeOf(entry.logged_at)">{{ timeOf(entry.logged_at) }}</td>
          <td :class="LEVEL_TONES[entry.level]">{{ LEVEL_LABELS[entry.level] }}</td>
          <td :title="entry.target || undefined">{{ SOURCE_LABELS[entry.source] }}</td>
          <td class="font-mono text-fg-strong" :title="entry.message">
            {{ entry.message }}
          </td>
        </tr>
      </tbody>
    </table>
  </console-list>
</template>

<style scoped>
/* Columns sized to their content, in characters; the message takes the rest. */
col.index {
  inline-size: 7ch;
}

col.time {
  inline-size: 9ch;
}

col.level {
  inline-size: 14ch;
}

col.source {
  inline-size: 10ch;
}
</style>
