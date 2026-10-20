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
          <td :title="entry.connection_name || undefined">
            {{ ACTION_LABELS[entry.action] }}
          </td>
          <td :title="entry.sql">
            <div class="message">
              <span class="sql font-mono text-fg-strong">{{ oneLine(entry.sql) }}</span>
              <span
                class="outcome"
                :class="entry.error === '' ? 'text-fg-muted' : 'text-error'"
                :title="entry.error || undefined"
                >— {{ outcomeOf(entry) }}</span
              >
            </div>
          </td>
          <td class="end">{{ formatMicros(entry.duration_us) }}</td>
          <td class="end" :class="{ 'text-fg-dimmed': entry.transfer_us === 0 }">
            {{ transferOf(entry) }}
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

col.action {
  inline-size: 16ch;
}

col.duration {
  inline-size: 13ch;
}

/* Message: the SQL shrinks first, with an ellipsis; the outcome keeps its
   place, up to half of the cell. */
.message {
  display: flex;
  gap: 0.5ch;
  min-inline-size: 0;
}

.sql,
.outcome {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.sql {
  flex: 0 1 auto;
  min-inline-size: 0;
}

.outcome {
  flex: none;
  max-inline-size: 50%;
}
</style>
