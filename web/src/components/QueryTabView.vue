<!--
  Contenu de l'onglet d'une requête : état, erreur éventuelle et grille de résultats.
-->
<script setup lang="ts">
import type { QueryTab } from '@/api/querySession'

import ResultGrid from './ResultGrid.vue'
import UiAlert from './ui/UiAlert.vue'
import UiBadge from './ui/UiBadge.vue'

defineProps<{
  /**
   * Requête affichée.
   */
  tab: QueryTab
}>()

/**
 * Formate un nombre de lignes à la française (espace des milliers).
 *
 * @param count Nombre de lignes.
 */
function lines(count: number): string {
  return `${count.toLocaleString('fr-FR')} lignes`
}
</script>

<template>
  <!--
    min-w-0 : sans lui, l'onglet (élément flexible d'une rangée) ne rétrécit
    jamais sous la largeur de son contenu, et une grille large débordait,
    rognée à droite, au lieu de défiler dans sa propre zone.
  -->
  <div class="flex h-full min-h-0 min-w-0 flex-1 flex-col gap-gap">
    <ui-alert v-if="tab.error" tone="error" :title="tab.error" />
    <div v-if="tab.result" class="tab-status">
      <span v-if="tab.done">{{ lines(tab.result.rowCount) }} · {{ tab.result.elapsedMs }} ms</span>
      <span v-else>{{ lines(tab.result.rowCount) }} reçues…</span>
      <ui-badge v-if="tab.result.truncated" tone="warning">
        Résultat tronqué à {{ lines(tab.result.rowCount) }}
      </ui-badge>
      <code v-if="tab.result.limit === 'added'" class="tab-sql" :title="tab.result.sql">
        {{ tab.result.sql }}
      </code>
    </div>
    <div v-else-if="!tab.done" class="tab-status">Exécution…</div>
    <result-grid
      v-if="tab.result && tab.result.columns.length > 0"
      :columns="tab.result.columns"
      :right-aligned="tab.result.rightAligned"
      :rows="tab.result.rows"
      :row-count="tab.result.rowCount"
      :sql="tab.sql"
    />
  </div>
</template>

<style scoped>
.tab-status {
  display: flex;
  align-items: center;
  gap: var(--gap);
  color: var(--fg-muted);
  font-size: var(--font-size-ui);

  /* Le nombre de lignes et la durée gardent leur largeur ; seule la requête,
     tronquée (…), cède la place. */
  & > :not(.tab-sql) {
    flex: none;
    white-space: nowrap;
  }
}

.tab-sql {
  min-width: 0;
  overflow: hidden;
  font-family: var(--font-code);
  font-size: var(--font-size-small);
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
