<!--
  Pastille d'avancement d'une requête, à gauche du libellé de son onglet.

  Grise et pulsant très lentement tant que la requête attend son tour, bleue
  et pulsante pendant l'exécution (lentement) puis pendant la réception des
  lignes (vite), verte quand le résultat est complet, rouge en cas d'échec.
  Au survol, une info-bulle donne le statut, la durée (qui court pendant
  l'exécution) et le nombre de lignes.
-->
<script setup lang="ts">
import { NTooltip } from 'naive-ui'
import { computed } from 'vue'

import { type QueryTab, progressSummary, tabProgress } from '@/api/querySession'

const props = defineProps<{
  /**
   * Requête suivie.
   */
  tab: QueryTab
  /**
   * Instant présent, rafraîchi pendant l'exécution pour faire courir la durée.
   */
  now: number
}>()

const progress = computed(() => tabProgress(props.tab))
const summary = computed(() => progressSummary(props.tab, props.now))
</script>

<template>
  <n-tooltip placement="bottom-start" :show-arrow="false">
    <template #trigger>
      <span class="progress-dot" :class="progress" role="img" :aria-label="summary[0]" />
    </template>
    <div v-for="line in summary" :key="line">{{ line }}</div>
  </n-tooltip>
</template>

<style scoped>
.progress-dot {
  display: inline-block;
  width: 8px;
  height: 8px;
  margin-right: 6px;
  border-radius: 50%;
  vertical-align: middle;
}

.waiting {
  background: #9ca3af;
  animation: pulse 3s ease-in-out infinite;
}

.executing,
.loading {
  background: #3b82f6;
  animation: pulse 1.4s ease-in-out infinite;
}

.loading {
  animation-duration: 0.6s;
}

.done {
  background: #22c55e;
}

.error {
  background: #ef4444;
}

@keyframes pulse {
  0%,
  100% {
    opacity: 1;
    transform: scale(1);
  }

  50% {
    opacity: 0.35;
    transform: scale(0.7);
  }
}
</style>
