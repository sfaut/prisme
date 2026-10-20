<!--
  History tab of the console: one line per query sent to a target server,
  with its number, its time, the action that sent it, its message (its SQL,
  then its failure if any), its number of rows, its execution time and the
  transfer time of its rows.
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
  failureOf,
  rowsOf,
  rowsTitleOf,
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
        <col class="rows" />
        <col class="duration" />
        <col class="duration" />
      </colgroup>
      <thead>
        <tr>
          <th class="end">#</th>
          <th>Heure</th>
          <th>Action</th>
          <th>Message</th>
          <th class="end">Lignes</th>
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
              <span v-if="entry.error" class="outcome text-error" :title="entry.error">{{
                failureOf(entry)
              }}</span>
            </div>
          </td>
          <td
            class="end"
            :class="{ 'text-fg-dimmed': rowsOf(entry) === '—' }"
            :title="rowsTitleOf(entry)"
          >
            {{ rowsOf(entry) }}
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
/* Columns sized to their content, in characters, plus the padding of the
   cells; the message takes the rest. */
col.index {
  inline-size: calc(6ch + 2 * var(--control-padding-x));
}

col.time {
  inline-size: calc(9ch + 2 * var(--control-padding-x));
}

col.action {
  inline-size: calc(18ch + 2 * var(--control-padding-x));
}

col.rows {
  inline-size: calc(9ch + 2 * var(--control-padding-x));
}

col.duration {
  inline-size: calc(13ch + 2 * var(--control-padding-x));
}

/* Message: the SQL shrinks first, with an ellipsis; the failure keeps its
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
