<!--
  Buttons of the result tab shown, right of the result tabs, side by side
  with the fold button of the results, as in the console's header:

  - Refresh: runs the query again in the tab; during the refresh, or while it
    waits for the running batch, it stops it.
  - User filter, when the tab shows a grid: a funnel, as in the console,
    expands or collapses the filter's form. Collapsed, the filter stays
    applied; the button shows it in the primary color.
  - Download, when the tab shows a grid: its menu, given by the view of the
    tab (`QueryTabView.vue`), downloads the selection or the displayed
    result as CSV, or exports the full result.
-->
<script setup lang="ts">
import { Download, Funnel, RefreshCw, RefreshCwOff } from '@lucide/vue'
import { computed } from 'vue'

import { type QueryTab, type RefreshState, showsGrid } from '@/api/querySession'

import type { MenuItem } from './ui/menuItem'
import UiButton from './ui/UiButton.vue'
import UiDropdownMenu from './ui/UiDropdownMenu.vue'
import { vTooltip } from './ui/tooltip'

const props = defineProps<{
  /**
   * Tab shown.
   */
  tab: QueryTab
  /**
   * Refresh of the tab: running, waiting, possible or not.
   */
  refresh: RefreshState
  /**
   * Choices of the download menu, given by the view of the tab; absent until
   * that view is mounted.
   */
  download?: MenuItem[]
}>()

const emit = defineEmits<{
  /**
   * The user asks to run the query again.
   */
  refresh: []
  /**
   * The user stops the refresh, or cancels it while it waits.
   */
  'stop-refresh': []
  /**
   * The user expands or collapses the form of the user filter.
   */
  'toggle-filter': []
}>()

/**
 * Is a refresh running or waiting? Its button then stops it.
 */
const refreshing = computed(() => props.refresh === 'running' || props.refresh === 'queued')

/**
 * Hover text of the refresh button.
 */
const refreshTitle = computed(() => {
  switch (props.refresh) {
    case 'running':
      return 'Arrêter le rafraîchissement : le résultat actuel est gardé'
    case 'queued':
      return 'Annuler le rafraîchissement, en attente de la fin des requêtes en cours'
    default:
      return 'Rafraîchir : relance la requête'
  }
})

/**
 * Label of the user filter button: what a click does, and whether the
 * filter keeps only some rows.
 */
const filterLabel = computed(
  () =>
    `${props.tab.filterOpen ? 'Masquer' : 'Afficher'} le filtre${props.tab.filter.text === '' ? '' : ' (filtre actif)'}`,
)
</script>

<template>
  <ui-button
    :icon="refreshing ? RefreshCwOff : RefreshCw"
    variant="ghost"
    :tone="refreshing ? 'primary' : 'neutral'"
    small
    :disabled="refresh === 'unavailable'"
    :aria-label="refreshing ? 'Arrêter le rafraîchissement' : 'Rafraîchir'"
    v-tooltip="refreshTitle"
    @click="refreshing ? emit('stop-refresh') : emit('refresh')"
  />
  <ui-button
    v-if="showsGrid(tab)"
    :icon="Funnel"
    variant="ghost"
    :tone="tab.filter.text === '' ? 'neutral' : 'primary'"
    small
    :class="{ 'bg-surface-raised': tab.filterOpen }"
    :aria-pressed="tab.filterOpen"
    :aria-label="filterLabel"
    v-tooltip="filterLabel"
    @click="emit('toggle-filter')"
  />
  <ui-dropdown-menu v-if="showsGrid(tab) && download" :items="download">
    <ui-button
      :icon="Download"
      variant="ghost"
      small
      aria-label="Télécharger"
      v-tooltip="'Télécharger'"
    />
  </ui-dropdown-menu>
</template>
