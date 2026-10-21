<!--
  State of the synchronisation with the internal database, in the top bar:
  every change (editors, connections, settings, profile) is saved there, and
  read back from it. An icon per state, explained by its tooltip: nothing
  exchanged yet, reading, writing (a save running or waiting), refused write,
  everything synchronised.
-->
<script setup lang="ts">
import { Database, DatabaseArrowDown, DatabaseArrowUp, DatabaseCheck, DatabaseX } from '@lucide/vue'
import { computed } from 'vue'

import { syncState } from '@/api/sync'
import { vTooltip } from './ui/tooltip'

/**
 * Text, icon and tone of each state.
 */
const STATES = {
  idle: { text: 'Base locale : aucun échange', icon: Database, tone: 'text-fg-muted' },
  reading: { text: 'Lecture de la base locale…', icon: DatabaseArrowDown, tone: 'text-fg-muted' },
  writing: {
    text: 'Enregistrement dans la base locale…',
    icon: DatabaseArrowUp,
    tone: 'text-fg-muted',
  },
  failed: {
    text: 'Échec de l’enregistrement dans la base locale',
    icon: DatabaseX,
    tone: 'text-error',
  },
  synced: { text: 'Synchronisé avec la base locale', icon: DatabaseCheck, tone: 'text-fg-muted' },
} as const

/**
 * Text, icon and tone of the current state.
 */
const current = computed(() => STATES[syncState.value])
</script>

<template>
  <span
    v-tooltip="current.text"
    role="status"
    :aria-label="current.text"
    class="inline-flex"
    :class="current.tone"
  >
    <component :is="current.icon" class="size-icon shrink-0" aria-hidden="true" />
  </span>
</template>
