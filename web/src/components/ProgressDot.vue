<!--
  Pastille d'avancement d'une requête, à gauche du libellé de son onglet.

  Bleue et pulsante pendant l'exécution (lentement) puis pendant la réception
  des lignes (vite), verte quand le résultat est complet, rouge en cas d'échec.
-->
<script setup lang="ts">
import { computed } from 'vue'

import type { TabProgress } from '@/api/querySession'

const props = defineProps<{
  /**
   * Avancement de la requête.
   */
  progress: TabProgress
}>()

/**
 * Texte d'aide au survol, lu aussi par les lecteurs d'écran.
 */
const label = computed(
  () =>
    ({
      executing: 'Exécution',
      loading: 'Chargement des lignes',
      done: 'Résultat complet',
      error: 'Échec',
    })[props.progress],
)
</script>

<template>
  <span class="progress-dot" :class="progress" :title="label" role="img" :aria-label="label" />
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
