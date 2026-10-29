<!--
  Shows the transient messages of `toasts.ts`, at the bottom right, on
  Reka UI's `Toast` (duration, pause on hover, announcement to screen
  readers). A message with an action shows its button and a close button,
  and stays until one of them is used. To mount only once, in `App.vue`.
-->
<script setup lang="ts">
import { CircleCheckBig, CircleX, X } from '@lucide/vue'
import {
  ToastAction,
  ToastClose,
  ToastProvider,
  ToastRoot,
  ToastTitle,
  ToastViewport,
} from 'reka-ui'

import UiButton from './UiButton.vue'
import { dismiss, toasts } from './toasts'

/**
 * Display duration of a message, in milliseconds.
 */
const DURATION = 3000

/**
 * Display duration of a message with an action: it waits for the user.
 * Reka UI starts no timer for an infinite duration.
 */
const UNTIL_CLOSED = Number.POSITIVE_INFINITY
</script>

<template>
  <toast-provider :duration="DURATION">
    <toast-root
      v-for="toast in toasts"
      :key="toast.id"
      :duration="toast.action ? UNTIL_CLOSED : DURATION"
      class="flex items-center gap-3 rounded-xl border border-line bg-surface px-4 py-3 text-base text-fg-strong shadow-lg"
      @update:open="(open: boolean) => open || dismiss(toast.id)"
    >
      <CircleCheckBig v-if="toast.tone === 'success'" class="size-5 shrink-0 text-success" />
      <CircleX v-else class="size-5 shrink-0 text-error" />
      <toast-title class="min-w-0 flex-1 text-trim whitespace-pre-line">{{
        toast.title
      }}</toast-title>
      <template v-if="toast.action">
        <toast-action :alt-text="toast.action.alternative" as-child>
          <ui-button
            :label="toast.action.label"
            variant="solid"
            tone="primary"
            small
            @click="toast.action.run()"
          />
        </toast-action>
        <toast-close as-child>
          <ui-button :icon="X" variant="ghost" small aria-label="Fermer" />
        </toast-close>
      </template>
    </toast-root>
    <toast-viewport class="fixed inset-e-3 bottom-4 z-50 flex w-96 flex-col gap-3" />
  </toast-provider>
</template>
