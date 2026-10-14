<!--
  Champ de formulaire : libellé à gauche, contrôle à droite. Le libellé
  enveloppe le contrôle : un clic dessus donne le focus au champ. Avec
  `top`, le libellé s'aligne sur le haut du contrôle (champ sur plusieurs
  lignes).
-->
<script setup lang="ts">
defineProps<{
  /**
   * Libellé du champ.
   */
  label: string
  /**
   * Aligne le libellé sur le haut du contrôle plutôt qu'en son milieu.
   */
  top?: boolean
}>()
</script>

<template>
  <label class="flex justify-between gap-gap" :class="top ? 'items-start' : 'items-center'">
    <span class="text-fg-muted" :class="{ 'pt-1': top }">{{ label }}</span>
    <slot />
  </label>
</template>
