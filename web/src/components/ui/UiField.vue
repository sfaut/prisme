<!--
  Champ de formulaire : libellé à gauche, contrôle à droite. Le libellé
  enveloppe le contrôle : un clic dessus donne le focus au champ.
-->
<script setup lang="ts">
defineProps<{
  /**
   * Libellé du champ.
   */
  label: string
}>()
</script>

<template>
  <label class="flex items-center justify-between gap-gap">
    <span class="text-fg-muted">{{ label }}</span>
    <slot />
  </label>
</template>
