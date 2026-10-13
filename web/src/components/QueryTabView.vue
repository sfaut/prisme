<!--
  Contenu de l'onglet d'une requête : erreur éventuelle, grille de résultats
  et, sous la grille, le nombre de lignes chargées et la durée de la requête.

  Pendant la réception, ce nombre et la durée changent une fois par seconde
  (horloge partagée, `clock.ts`) ; à la fin de la requête, ils prennent
  aussitôt leur valeur finale (durée mesurée par le serveur). Une requête
  sans résultat à afficher (en attente, en échec, ou sans colonnes) ne
  montre que sa durée.
-->
<script setup lang="ts">
import { computed, ref, watch } from 'vue'

import { type QueryTab, formatDuration, tabDuration } from '@/api/querySession'

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

watch(
  [clock, () => props.tab.done, () => props.tab.result],
  () => {
    shownCount.value = props.tab.result?.rowCount ?? 0
  },
  { immediate: true },
)

/**
 * Durée de la requête, formatée ; vide tant qu'elle attend son tour.
 * L'horloge n'est lue que pendant l'exécution : une requête terminée n'est
 * plus redessinée à chaque tic.
 */
const duration = computed(() => {
  const running = props.tab.startedAt !== null && !props.tab.done
  const elapsed = tabDuration(props.tab, running ? clock.value : Date.now())
  return elapsed === null ? '' : formatDuration(elapsed)
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
        <span v-if="duration">· {{ duration }}</span>
      </template>
    </result-grid>
    <div v-else-if="duration" class="text-trim text-fg-muted">{{ duration }}</div>
  </div>
</template>
