<!--
  Contenu de l'onglet d'une requête : état, erreur éventuelle et grille de résultats.
-->
<script setup lang="ts">
import { NAlert, NTag } from 'naive-ui'

import type { QueryTab } from '@/api/querySession'

import ResultGrid from './ResultGrid.vue'

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
  <div class="query-tab">
    <n-alert v-if="tab.error" type="error" :show-icon="false">{{ tab.error }}</n-alert>
    <div v-if="tab.result" class="tab-status">
      <span v-if="tab.done">{{ lines(tab.result.rowCount) }} · {{ tab.result.elapsedMs }} ms</span>
      <span v-else>{{ lines(tab.result.rowCount) }} reçues…</span>
      <n-tag v-if="tab.result.truncated" type="warning" size="small">
        Résultat tronqué à {{ lines(tab.result.rowCount) }}
      </n-tag>
      <code v-if="tab.result.limit === 'added'" class="tab-sql" :title="tab.result.sql">
        {{ tab.result.sql }}
      </code>
    </div>
    <div v-else-if="!tab.done" class="tab-status">Exécution…</div>
    <result-grid
      v-if="tab.result && tab.result.columns.length > 0"
      :columns="tab.result.columns"
      :rows="tab.result.rows"
      :row-count="tab.result.rowCount"
    />
  </div>
</template>

<style scoped>
.query-tab {
  display: flex;
  flex: 1;
  flex-direction: column;
  gap: 6px;
  height: 100%;
  min-height: 0;
}

.tab-status {
  display: flex;
  align-items: center;
  gap: 8px;
  color: #666;
  font-size: 0.8rem;
}

.tab-sql {
  overflow: hidden;
  font-size: 0.75rem;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
