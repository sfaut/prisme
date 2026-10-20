<!--
  History tab of the console: one line per query sent to a target server,
  the oldest at the top, with its number, its time, the action that sent it,
  its message (its SQL, then its outcome), its execution time and the
  transfer time of its rows. The table stays scrolled to the bottom while
  the user does not scroll up, so that the new queries show. The older
  queries are read on request, from the button at the top.
-->
<script setup lang="ts">
import { nextTick, useTemplateRef, watch } from 'vue'

import { useConsoleStore } from '@/stores/console'

import {
  ACTION_LABELS,
  dateTimeOf,
  formatMicros,
  oneLine,
  outcomeOf,
  timeOf,
  transferOf,
} from './consoleFormat'
import UiAlert from './ui/UiAlert.vue'
import UiButton from './ui/UiButton.vue'

const store = useConsoleStore()

/**
 * Scrolling area of the table.
 */
const scroller = useTemplateRef<HTMLElement>('scroller')

/**
 * Is the area scrolled to the bottom, or almost?
 */
function atBottom(element: HTMLElement): boolean {
  return element.scrollHeight - element.scrollTop - element.clientHeight < element.clientHeight / 4
}

// Before the lines are drawn: stuck at the bottom, the area follows them.
watch(
  () => store.entries,
  (entries, previous) => {
    const element = scroller.value
    const appended = previous.length === 0 || entries[0]?.id === previous[0]?.id
    if (element !== null && appended && atBottom(element)) {
      void nextTick(() => element.scrollTo({ top: element.scrollHeight }))
    }
  },
  { flush: 'pre' },
)
</script>

<template>
  <div ref="scroller" class="min-h-0 flex-1 overflow-auto">
    <ui-alert
      v-if="store.error"
      tone="error"
      :title="store.error"
      closable
      @close="store.error = null"
    />
    <div v-if="store.hasOlder" class="flex justify-center py-1">
      <ui-button variant="ghost" small @click="store.loadOlder()">
        Afficher les requêtes précédentes
      </ui-button>
    </div>
    <p v-if="store.entries.length === 0" class="p-panel text-center text-fg-muted">
      Aucune requête envoyée pour le moment
    </p>
    <table v-else class="history w-full table-fixed border-collapse text-small tabular-nums">
      <colgroup>
        <col class="index" />
        <col class="time" />
        <col class="action" />
        <col />
        <col class="duration" />
        <col class="duration" />
      </colgroup>
      <thead class="sticky top-0 bg-surface text-start text-fg-muted">
        <tr>
          <th class="text-end">#</th>
          <th>Heure</th>
          <th>Action</th>
          <th>Message</th>
          <th class="text-end">Durée</th>
          <th class="text-end" title="Délai de transmission des lignes">Transmission</th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="entry in store.entries" :key="entry.id" class="hover:bg-surface-muted">
          <td class="text-end text-fg-dimmed">{{ entry.id }}</td>
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
          <td class="text-end">{{ formatMicros(entry.duration_us) }}</td>
          <td class="text-end">{{ transferOf(entry) }}</td>
        </tr>
      </tbody>
    </table>
  </div>
</template>

<style scoped>
/* Columns sized to their content, in characters; the message takes the rest. */
.history {
  & col.index {
    inline-size: 7ch;
  }

  & col.time {
    inline-size: 9ch;
  }

  & col.action {
    inline-size: 16ch;
  }

  & col.duration {
    inline-size: 13ch;
  }

  /* Lines as dense as those of the result grid. */
  & tr {
    block-size: var(--grid-row-height);
  }

  & th,
  & td {
    padding-block: 0;
    padding-inline: var(--grid-cell-padding-x);
    font-weight: normal;
    text-align: start;
    white-space: nowrap;
  }

  & th.text-end,
  & td.text-end {
    text-align: end;
  }

  & thead th {
    border-block-end: var(--line-width) solid var(--line);
  }
}
</style>
