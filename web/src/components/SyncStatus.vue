<!--
  State of the synchronisation with the internal database, in the top bar:
  the editors' text, names and connections are saved there as they change.
  "Synchronisé" when everything is saved, "Synchronisation…" while a save is
  pending, and an error when the server refused the last one (the message
  then shows in the editors' error alert and in the console).
-->
<script setup lang="ts">
import { CircleAlert, CircleCheck, LoaderCircle } from 'lucide-vue-next'
import { computed } from 'vue'

import { useEditorsStore } from '@/stores/editors'

const editors = useEditorsStore()

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
const current = computed(() => LABELS[editors.syncState])
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
      :class="{ 'animate-spin': editors.syncState === 'pending' }"
    />
    <span>{{ current.text }}</span>
  </span>
</template>
