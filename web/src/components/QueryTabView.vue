<!--
  Contenu de l'onglet d'une requête : erreur éventuelle, grille de résultats
  et, sous la grille, le nombre de lignes chargées et deux durées : le temps
  d'exécution de la requête par le serveur (jusqu'à l'arrivée des colonnes),
  puis le temps de transfert et d'affichage des lignes, suivi du volume
  reçu et du débit.

  Pendant la requête, le nombre de lignes, les durées et le volume changent une fois
  par seconde (horloge partagée, `clock.ts`) ; à la fin, ils prennent
  aussitôt leur valeur finale. Une requête sans grille (en attente, en
  échec, ou sans colonnes) ne montre que ses durées.
-->
<script setup lang="ts">
import { computed, ref, watch } from 'vue'

import {
  type QueryTab,
  executionDuration,
  formatBytes,
  formatDuration,
  transferDuration,
} from '@/api/querySession'

import { useClock } from './clock'
import ResultGrid from './ResultGrid.vue'
import UiAlert from './ui/UiAlert.vue'

const props = defineProps<{
  /**
   * Requête affichée.
   */
  tab: QueryTab
}>()

const clock = useClock()

/**
 * Nombre de lignes affiché sous la grille : relevé à chaque tic de
 * l'horloge, et à la fin de la requête.
 */
const shownCount = ref(props.tab.result?.rowCount ?? 0)

/**
 * Volume reçu affiché sous la grille, en octets, relevé comme le nombre de lignes.
 */
const shownBytes = ref(props.tab.bytes)

watch(
  [clock, () => props.tab.done, () => props.tab.result],
  () => {
    shownCount.value = props.tab.result?.rowCount ?? 0
    shownBytes.value = props.tab.bytes
  },
  { immediate: true },
)

/**
 * Durées de la requête, formatées : « exécution 120 ms · transfert 2,4 s ·
 * 3,1 Mo · 1,3 Mo/s » (volume reçu et débit pendant le transfert) ; vide
 * tant qu'elle attend son tour. L'horloge n'est lue que pendant la
 * requête : une requête terminée n'est plus redessinée à chaque tic.
 */
const durations = computed(() => {
  const running = props.tab.startedAt !== null && !props.tab.done
  const now = running ? clock.value : performance.now()
  const execution = executionDuration(props.tab, now)
  const transfer = transferDuration(props.tab, now)
  const volume = transfer === null ? null : formatBytes(shownBytes.value)
  const rate =
    transfer === null || transfer <= 0
      ? null
      : `${formatBytes((shownBytes.value * 1000) / transfer)}/s`
  return [
    execution === null ? null : `exécution ${formatDuration(execution)}`,
    transfer === null ? null : `transfert ${formatDuration(transfer)}`,
    volume,
    rate,
  ]
    .filter((part) => part !== null)
    .join(' · ')
})
</script>

<template>
  <!--
    min-w-0 : sans lui, l'onglet (élément flexible d'une rangée) ne rétrécit
    jamais sous la largeur de son contenu, et une grille large débordait,
    rognée à droite, au lieu de défiler dans sa propre zone.
  -->
  <div class="flex h-full min-h-0 min-w-0 flex-1 flex-col gap-gap">
    <ui-alert v-if="tab.error" tone="error" :title="tab.error" />
    <result-grid
      v-if="tab.result && tab.result.columns.length > 0"
      :columns="tab.result.columns"
      :right-aligned="tab.result.rightAligned"
      :rows="tab.result.rows"
      :row-count="tab.result.rowCount"
      :shown-count="shownCount"
      :sql="tab.sql"
    >
      <template #footer>
        <template v-if="durations">&nbsp;· {{ durations }}</template>
      </template>
    </result-grid>
    <div v-else-if="durations" class="text-trim text-fg-muted">{{ durations }}</div>
  </div>
</template>
