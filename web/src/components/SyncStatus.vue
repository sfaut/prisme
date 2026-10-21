<!--
  State of the synchronisation with the internal database, in the top bar:
  every change (editors, connections, settings, profile) is saved there.
  "Synchronisé" when everything is saved, "Synchronisation…" while a write is
  running or waiting, and an error when the server refused the last one (its
  message shows where the write came from).
-->
<script setup lang="ts">
import { CircleAlert, CircleCheck, LoaderCircle } from '@lucide/vue'
import { computed } from 'vue'

import { syncState } from '@/api/sync'

/**
 * Text and icon of each state.
 */
const LABELS = {
  synced: { text: 'Synchronisé', icon: CircleCheck, tone: 'text-fg-muted' },
  pending: { text: 'Synchronisation…', icon: LoaderCircle, tone: 'text-fg-muted' },
  failed: { text: 'Échec de la synchronisation', icon: CircleAlert, tone: 'text-error' },
} as const

/**
 * Text, icon and tone of the current state.
 */
const current = computed(() => LABELS[syncState.value])
</script>

<template>
  <span
    role="status"
    class="inline-flex items-center gap-1 text-ui"
    :class="current.tone"
    :title="`${current.text} avec la base locale`"
  >
    <component
      :is="current.icon"
      class="size-icon shrink-0"
      :class="{ 'animate-spin': syncState === 'pending' }"
    />
    <span>{{ current.text }}</span>
  </span>
</template>
