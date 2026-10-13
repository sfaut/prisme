<!--
  Message dans un bandeau coloré : succès, avertissement ou erreur.

  Avec `closable`, une croix à droite émet `close`. L'emplacement par défaut
  accueille des actions à droite du message (bouton « Reconnecter »...).
-->
<script setup lang="ts">
import { X } from 'lucide-vue-next'

import UiButton from './UiButton.vue'

defineProps<{
  /**
   * Ton du bandeau.
   */
  tone: 'success' | 'warning' | 'error'
  /**
   * Message.
   */
  title: string
  /**
   * Affiche une croix pour fermer le bandeau.
   */
  closable?: boolean
}>()

defineEmits<{
  /**
   * L'utilisateur ferme le bandeau.
   */
  close: []
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
  <div
    role="alert"
    class="flex items-center gap-gap rounded-control px-control-x py-1.5 text-ui"
    :class="TONES[tone]"
  >
    <span class="min-w-0 flex-1 break-words">{{ title }}</span>
    <slot />
    <ui-button
      v-if="closable"
      :icon="X"
      variant="ghost"
      small
      aria-label="Fermer"
      class="text-current"
      @click="$emit('close')"
    />
  </div>
</template>
