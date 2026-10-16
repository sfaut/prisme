<!--
  Shows the transient messages of `toasts.ts`, at the bottom right, on
  Reka UI's `Toast` (duration, pause on hover, announcement to screen
  readers). To mount only once, in `App.vue`.
-->
<script setup lang="ts">
import { CircleAlert, CircleCheck } from 'lucide-vue-next'
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
      class="flex items-center gap-gap rounded-panel border border-line bg-surface px-panel py-2 text-ui text-fg-strong shadow-lg"
      @update:open="(open: boolean) => open || dismiss(toast.id)"
    >
      <CircleCheck v-if="toast.tone === 'success'" class="size-4 shrink-0 text-success" />
      <CircleAlert v-else class="size-4 shrink-0 text-error" />
      <toast-title class="text-trim">{{ toast.title }}</toast-title>
    </toast-root>
    <toast-viewport
      class="fixed end-panel bottom-panel z-50 flex w-toast flex-col gap-gap outline-none"
    />
  </toast-provider>
</template>
