<!--
  Message in a colored banner: success, warning or error.

  With `closable`, a cross on the right emits `close`. The default slot holds
  actions on the right of the message ("Reconnecter" button...).
-->
<script setup lang="ts">
import { X } from 'lucide-vue-next'

import UiButton from './UiButton.vue'

defineProps<{
  /**
   * Tone of the banner.
   */
  tone: 'success' | 'warning' | 'error'
  /**
   * Message.
   */
  title: string
  /**
   * Shows a cross to close the banner.
   */
  closable?: boolean
}>()

defineEmits<{
  /**
   * The user closes the banner.
   */
  close: []
}>()

/**
 * Classes of each tone, written out so that Tailwind finds them.
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
