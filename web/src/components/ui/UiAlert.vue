<!--
  Message in a colored banner: information, success, warning or error, each
  with its icon.

  With `closable`, a cross on the right emits `close`. The default slot holds
  actions on the right of the message ("Reconnecter" button...).
-->
<script setup lang="ts">
import { CircleAlert, CircleCheckBig, CircleX, Info, X } from 'lucide-vue-next'

import UiButton from './UiButton.vue'

defineProps<{
  /**
   * Tone of the banner.
   */
  tone: 'info' | 'success' | 'warning' | 'error'
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
 * Classes and icon of each tone, the classes written out so that Tailwind finds them.
 */
const TONES = {
  info: { classes: 'bg-accent-soft text-accent', icon: Info },
  success: { classes: 'bg-success-soft text-success', icon: CircleCheckBig },
  warning: { classes: 'bg-warning-soft text-warning', icon: CircleAlert },
  error: { classes: 'bg-error-soft text-error', icon: CircleX },
} as const
</script>

<template>
  <div
    role="alert"
    class="flex items-center gap-2 rounded-control px-2 py-2 text-ui"
    :class="TONES[tone].classes"
  >
    <component :is="TONES[tone].icon" class="size-icon shrink-0" aria-hidden="true" />
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
