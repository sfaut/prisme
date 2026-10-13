<!--
  Pastille d'avancement d'une requête, à gauche du libellé de son onglet.

  Grise et pulsant très lentement tant que la requête attend son tour, bleue
  et pulsante pendant l'exécution (lentement) puis pendant la réception des
  lignes (vite), verte quand le résultat est complet, rouge en cas d'échec.
  Au survol, une info-bulle donne le statut, la durée (qui court pendant
  l'exécution) et le nombre de lignes.
-->
<script setup lang="ts">
import { computed } from 'vue'

import UiTooltip from './ui/UiTooltip.vue'

import { type QueryTab, progressSummary, tabProgress } from '@/api/querySession'

import { useClock } from './clock'

const props = defineProps<{
  /**
   * Requête suivie.
   */
  tab: QueryTab
}>()

const clock = useClock()
const progress = computed(() => tabProgress(props.tab))

// L'horloge n'est lue que pendant l'exécution : une requête terminée n'est
// plus redessinée à chaque tic.
const summary = computed(() => {
  const running = props.tab.startedAt !== null && !props.tab.done
  return progressSummary(props.tab, running ? clock.value : Date.now())
})
</script>

<template>
  <ui-tooltip>
    <span class="progress-dot" :class="progress" role="img" :aria-label="summary[0]" />
    <template #content>
      <div v-for="line in summary" :key="line">{{ line }}</div>
    </template>
  </ui-tooltip>
</template>

<style scoped>
.progress-dot {
  flex: none;
  width: 8px;
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
