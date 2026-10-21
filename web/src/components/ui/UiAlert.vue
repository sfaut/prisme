<!--
  Message in a colored banner: information, success, warning or error, each
  with its icon. The line breaks of the message show: a text quoted from
  elsewhere (`messages.ts`) comes after a blank line.

  Every banner can be closed: the cross on the right hides it and emits
  `close`, for the parent that holds the message to forget it. A new message
  shows the banner again. The default slot holds actions on the right of the
  message ("Reconnecter" button...).
-->
<script setup lang="ts">
import { CircleAlert, CircleCheckBig, CircleX, Info, X } from '@lucide/vue'
import { ref, watch } from 'vue'

import UiButton from './UiButton.vue'

const props = defineProps<{
  /**
   * Tone of the banner.
   */
  tone: 'info' | 'success' | 'warning' | 'error'
  /**
   * Message.
   */
  title: string
}>()

const emit = defineEmits<{
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

/**
 * Banner closed by the user, until its message changes.
 */
const closed = ref(false)

watch(
  () => props.title,
  () => (closed.value = false),
)

/**
 * Hides the banner and tells the parent.
 */
function close(): void {
  closed.value = true
  emit('close')
}
</script>

<template>
  <div
    v-if="!closed"
    role="alert"
    class="flex items-center gap-2 rounded-control px-2 py-2 text-ui"
    :class="TONES[tone].classes"
  >
    <component :is="TONES[tone].icon" class="size-icon shrink-0" aria-hidden="true" />
    <span class="min-w-0 flex-1 wrap-break-word whitespace-pre-line">{{ title }}</span>
    <slot />
    <ui-button
      :icon="X"
      variant="ghost"
      small
      aria-label="Fermer"
      class="text-current"
      @click="close"
    />
  </div>
</template>
