<!--
  Champ de formulaire : libellé à gauche, dans une colonne étroite
  (`--field-label-width`), contrôle à droite, sur toute la largeur restante.
  Le libellé enveloppe le contrôle : un clic dessus donne le focus au champ. Avec
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
  <label
    class="grid grid-cols-[var(--field-label-width)_minmax(0,1fr)] gap-gap"
    :class="top ? 'items-start' : 'items-center'"
  >
    <span class="text-fg-muted" :class="{ 'pt-1': top }">{{ label }}</span>
    <slot />
  </label>
</template>
