<!--
  History tab of the console: one line per query sent to a target server,
  with its number, its time, the action that sent it, its message (its SQL,
  then its outcome), its execution time and the transfer time of its rows.
  Scrolling and older pages: `ConsoleList.vue`.
-->
<script setup lang="ts">
import { useConsoleStore } from '@/stores/console'

import ConsoleList from './ConsoleList.vue'
import {
  ACTION_LABELS,
  dateTimeOf,
  formatMicros,
  oneLine,
  outcomeOf,
  timeOf,
  transferOf,
} from './consoleFormat'

const history = useConsoleStore().history
</script>

<template>
  <console-list :list="history" empty="Aucune requête envoyée pour le moment">
    <table class="console-table">
      <colgroup>
        <col class="index" />
        <col class="time" />
        <col class="action" />
        <col />
        <col class="duration" />
        <col class="duration" />
      </colgroup>
      <thead>
        <tr>
          <th class="end">#</th>
          <th>Heure</th>
          <th>Action</th>
          <th>Message</th>
          <th class="end">Durée</th>
          <th class="end" title="Délai de transmission des lignes">Transmission</th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="entry in history.entries" :key="entry.id">
          <td class="end text-fg-dimmed">{{ entry.id }}</td>
          <td :title="dateTimeOf(entry.started_at)">{{ timeOf(entry.started_at) }}</td>
          <td class="truncate" :title="entry.connection_name">
            {{ ACTION_LABELS[entry.action] }}
          </td>
          <td :title="entry.sql">
            <div class="flex min-w-0 gap-1">
              <span class="truncate font-mono text-fg-strong">{{ oneLine(entry.sql) }}</span>
              <span
                class="shrink-0 truncate"
                :class="entry.error === '' ? 'text-fg-muted' : 'text-error'"
                :title="entry.error || undefined"
                >— {{ outcomeOf(entry) }}</span
              >
            </div>
          </td>
          <td class="end">{{ formatMicros(entry.duration_us) }}</td>
          <td class="end">{{ transferOf(entry) }}</td>
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

col.action {
  inline-size: 16ch;
}

col.duration {
  inline-size: 13ch;
}
</style>
