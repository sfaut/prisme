<!--
  Progress dot of a query, on the left of its tab's label.

  Grey and pulsing very slowly while the query waits for its turn, blue and
  pulsing while running (slowly) then while receiving the rows (fast), green
  when the result is complete, red on failure.
-->
<script setup lang="ts">
import type { TabProgress } from '@/api/querySession'

defineProps<{
  /**
   * Progress shown (`tabProgress` for a query).
   */
  progress: TabProgress
}>()

/**
 * Label of each progress, for screen readers.
 */
const LABELS = {
  waiting: 'En attente',
  executing: 'Exécution',
  loading: 'Chargement des lignes',
  done: 'Terminé',
  error: 'Échec',
} as const
</script>

<template>
  <span class="progress-dot" :class="progress" role="img" :aria-label="LABELS[progress]" />
</template>

<style scoped>
@reference "@/assets/main.css";

.progress-dot {
  flex: none;
  width: --spacing(3);
  aspect-ratio: 1;
  border-radius: 50%;
}

.waiting {
  background: var(--color-fg-faint);
  animation: pulse 3s ease-in-out infinite;
}

.executing,
.loading {
  background: var(--color-primary);
  animation: pulse 1.4s ease-in-out infinite;
}

.loading {
  animation-duration: 0.6s;
}

.done {
  background: var(--color-success);
}

.error {
  background: var(--color-error);
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
