<!--
  Shows the transient messages of `toasts.ts`, at the bottom right, on
  Reka UI's `Toast` (duration, pause on hover, announcement to screen
  readers). To mount only once, in `App.vue`.
-->
<script setup lang="ts">
import { CircleCheckBig, CircleX } from '@lucide/vue'
import { ToastProvider, ToastRoot, ToastTitle, ToastViewport } from 'reka-ui'

import { dismiss, toasts } from './toasts'

/**
 * Display duration of a message, in milliseconds.
 */
const DURATION = 3000
</script>

<template>
  <toast-provider :duration="DURATION">
    <toast-root
      v-for="toast in toasts"
      :key="toast.id"
      class="flex items-center gap-2 rounded-panel border border-line bg-surface px-3 py-2 text-ui text-fg-strong shadow-lg"
      @update:open="(open: boolean) => open || dismiss(toast.id)"
    >
      <CircleCheckBig v-if="toast.tone === 'success'" class="size-icon shrink-0 text-success" />
      <CircleX v-else class="size-icon shrink-0 text-error" />
      <toast-title class="text-trim whitespace-pre-line">{{ toast.title }}</toast-title>
    </toast-root>
    <toast-viewport class="fixed inset-e-3 bottom-3 z-50 flex w-72 flex-col gap-2" />
  </toast-provider>
</template>
