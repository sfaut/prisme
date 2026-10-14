<!--
  Pastille d'avancement d'une requête, à gauche du libellé de son onglet.

  Grise et pulsant très lentement tant que la requête attend son tour, bleue
  et pulsante pendant l'exécution (lentement) puis pendant la réception des
  lignes (vite), verte quand le résultat est complet, rouge en cas d'échec.
-->
<script setup lang="ts">
import { computed } from 'vue'

import { type QueryTab, tabProgress } from '@/api/querySession'

const props = defineProps<{
  /**
   * Requête suivie.
   */
  tab: QueryTab
}>()

/**
 * Libellé de chaque avancement, pour les lecteurs d'écran.
 */
const LABELS = {
  waiting: 'En attente',
  executing: 'Exécution',
  loading: 'Chargement des lignes',
  done: 'Terminé',
  error: 'Échec',
} as const

const progress = computed(() => tabProgress(props.tab))
</script>

<template>
  <span class="progress-dot" :class="progress" role="img" :aria-label="LABELS[progress]" />
</template>

<style scoped>
.progress-dot {
  flex: none;
  width: var(--status-dot-size);
  aspect-ratio: 1;
  border-radius: 50%;
}

.waiting {
  background: var(--fg-dimmed);
  animation: pulse 3s ease-in-out infinite;
}

.executing,
.loading {
  background: var(--accent);
  animation: pulse 1.4s ease-in-out infinite;
}

.loading {
  animation-duration: 0.6s;
}

.done {
  background: var(--success);
}

.error {
  background: var(--error);
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
