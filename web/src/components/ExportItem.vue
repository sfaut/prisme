<!--
  An export in the exports manager (`ExportManager.vue`).

  - Its name, the query in its tooltip, and its state: a progress dot while
    it runs, then "prêt", "échoué", "arrêté" or "interrompu".
  - Its connection, its rows (with the completed share while it runs, when
    the total is known in advance), the size of its file, its duration and
    its throughput. While it runs, the numbers follow its progress and the
    duration runs (shared clock, `clock.ts`). An interrupted export gives
    the rows of its partial file, and ends with its last write.
  - Warnings: a file missing from the folder, more rows than Excel opens,
    the cause of a failure.
  - Actions: stop a running export, download a ready one, remove any of
    them with its file.
-->
<script setup lang="ts">
import { CircleStop, Download, Trash2 } from '@lucide/vue'
import { computed } from 'vue'

import type { ExportInfo } from '@/api/generated/ExportInfo'
import { describe } from '@/api/messages'
import { formatBytes, formatDuration, formatRowCount } from '@/api/querySession'
import { EXCEL_MAX_ROWS, useExportsStore } from '@/stores/exports'

import { useClock } from './clock'
import ProgressDot from './ProgressDot.vue'
import UiButton from './ui/UiButton.vue'
import UiStatusBar from './ui/UiStatusBar.vue'
import { vTooltip } from './ui/tooltip'

const props = defineProps<{
  /**
   * The export shown.
   */
  entry: ExportInfo
}>()

const store = useExportsStore()
const clock = useClock()

/**
 * Label, tone and explanation of each ended state.
 */
const ENDED = {
  ready: { label: 'prêt', tone: 'text-success', title: 'Fichier complet' },
  failed: { label: 'échoué', tone: 'text-error', title: 'Fichier partiel : l’export a échoué' },
  cancelled: { label: 'arrêté', tone: 'text-fg-secondary', title: 'Fichier partiel : arrêté' },
  interrupted: {
    label: 'interrompu',
    tone: 'text-fg-secondary',
    title: 'Fichier partiel : Prisme s’est arrêté pendant l’export',
  },
} as const

/**
 * Label, tone and explanation of the state, `null` while the export runs.
 */
const ended = computed(() => (props.entry.state === 'running' ? null : ENDED[props.entry.state]))

/**
 * Duration of the export, in milliseconds: until now while it runs.
 */
const duration = computed(() => {
  const start = Date.parse(props.entry.created_at)
  if (props.entry.ended_at !== null) {
    return Date.parse(props.entry.ended_at) - start
  }
  // Read so that the duration runs.
  void clock.value
  return Date.now() - start
})

/**
 * Throughput of the writing, in bytes per second; `null` before it has lasted.
 */
const rate = computed(() =>
  duration.value > 0 ? (props.entry.bytes * 1000) / duration.value : null,
)

/**
 * Completed share of a running export, in percent, when its total is known
 * in advance; `null` otherwise.
 */
const percent = computed(() => {
  const expected = props.entry.expected_rows
  return props.entry.state === 'running' && expected !== null && expected > 0
    ? Math.min(100, Math.floor((props.entry.rows * 100) / expected))
    : null
})

/**
 * Rows written, with the completed share if known.
 */
const rows = computed(() => {
  const count = formatRowCount(props.entry.rows, null)
  return percent.value === null ? count : `${count} (${percent.value} %)`
})
</script>

<template>
  <li class="flex flex-col gap-2 py-3">
    <div class="flex items-center gap-2">
      <progress-dot v-if="entry.state === 'running'" progress="loading" />
      <span class="min-w-0 flex-1 truncate font-medium text-fg-strong" v-tooltip="entry.sql">
        {{ entry.name }}
      </span>
      <span v-if="ended" class="text-sm" :class="ended.tone" v-tooltip="ended.title">
        {{ ended.label }}
      </span>
      <ui-button
        v-if="entry.state === 'running'"
        :icon="CircleStop"
        variant="ghost"
        small
        aria-label="Arrêter l’export"
        v-tooltip="'Arrêter l’export (le fichier partiel reste)'"
        @click="store.cancel(entry.id)"
      />
      <ui-button
        v-if="entry.state === 'ready'"
        :icon="Download"
        variant="ghost"
        small
        :disabled="entry.file_missing"
        aria-label="Télécharger"
        v-tooltip="'Télécharger'"
        @click="store.download(entry.id)"
      />
      <ui-button
        :icon="Trash2"
        variant="ghost"
        tone="error"
        small
        aria-label="Retirer"
        v-tooltip="
          entry.state === 'running'
            ? 'Retirer : arrête l’export et supprime le fichier'
            : 'Retirer : supprime le fichier'
        "
        @click="store.remove(entry.id)"
      />
    </div>
    <ui-status-bar>
      <span>{{ entry.connection_name }}</span>
      <span>{{ rows }}</span>
      <span>{{ formatBytes(entry.bytes) }}</span>
      <span>{{ formatDuration(duration) }}</span>
      <span v-if="rate !== null">{{ formatBytes(rate) }}/s</span>
    </ui-status-bar>
    <p v-if="entry.file_missing" class="text-sm text-error">
      Fichier manquant : supprimé ou déplacé hors de Prisme.
    </p>
    <p v-if="entry.rows > EXCEL_MAX_ROWS" class="text-sm text-warning">
      Excel n’en ouvre que les {{ EXCEL_MAX_ROWS.toLocaleString('fr-FR') }} premières lignes.
    </p>
    <p v-if="entry.error" class="text-sm whitespace-pre-line text-error">
      {{ describe(entry.error) }}
    </p>
  </li>
</template>
