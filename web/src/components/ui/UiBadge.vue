<!--
  Petite étiquette colorée (« Résultat tronqué »...).
-->
<script setup lang="ts">
defineProps<{
  /**
   * Ton de l'étiquette.
   */
  tone: 'success' | 'warning' | 'error'
}>()

/**
 * Classes de chaque ton, écrites en entier pour que Tailwind les trouve.
 */
const TONES = {
  success: 'bg-success-soft text-success',
  warning: 'bg-warning-soft text-warning',
  error: 'bg-error-soft text-error',
} as const
</script>

<template>
  <span
    class="inline-flex h-control-sm items-center rounded-control px-1.5 text-small font-medium"
    :class="TONES[tone]"
  >
    <span class="text-trim"><slot /></span>
  </span>
</template>
