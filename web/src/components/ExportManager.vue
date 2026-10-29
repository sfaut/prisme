<!--
  Exports manager, in the top bar: its button, with a progress dot while
  exports run, opens the list of the user's exports (`ExportItem.vue`), the
  most recent first, on Reka UI's `Popover`.

  Its head gives the room the files of the exports take: the exports folder
  is only cleaned from here, by removing exports. Files of Prisme that no
  export references (copied into the folder, internal database replaced)
  are counted apart, with their removal.

  The manager follows the exports from its mounting (`stores/exports.ts`),
  so that the end of an export is announced while the list is closed.
-->
<script setup lang="ts">
import { FileDown } from '@lucide/vue'
import { PopoverContent, PopoverPortal, PopoverRoot, PopoverTrigger } from 'reka-ui'
import { computed, onMounted, onUnmounted } from 'vue'

import type { FolderUsage } from '@/api/generated/FolderUsage'
import { formatBytes } from '@/api/querySession'
import { cssPixels } from '@/cssLength'
import { useExportsStore } from '@/stores/exports'

import ExportItem from './ExportItem.vue'
import ProgressDot from './ProgressDot.vue'
import UiAlert from './ui/UiAlert.vue'
import UiButton from './ui/UiButton.vue'
import { vTooltip } from './ui/tooltip'

/**
 * Gap with the button, in pixels (`--popup-offset` of the theme).
 *
 * Fixed size: Reka UI's `side-offset` only takes pixels, so this theme
 * length is read once (`docs/DEVELOPER.md`, section "Lengths in pixels").
 */
const OFFSET = cssPixels('--popup-offset', 4)

const store = useExportsStore()

onMounted(() => store.start())
onUnmounted(() => store.dispose())

/**
 * Label of the button, with the number of running exports.
 */
const title = computed(() =>
  store.runningCount === 0 ? 'Exports' : `Exports : ${store.runningCount} en cours`,
)

/**
 * Number of files and their size: "4 fichiers · 1,2 Go".
 *
 * @param usage Files and their total size.
 */
function formatUsage(usage: FolderUsage): string {
  return `${usage.count.toLocaleString('fr-FR')} ${usage.count > 1 ? 'fichiers' : 'fichier'} · ${formatBytes(usage.bytes)}`
}
</script>

<template>
  <popover-root>
    <popover-trigger as-child>
      <button
        type="button"
        class="inline-flex h-8 shrink-0 cursor-pointer items-center justify-center gap-2 rounded-lg text-fg-secondary hover:text-fg-strong data-[state=open]:text-fg-strong"
        :class="store.runningCount === 0 ? 'w-8' : 'px-2'"
        :aria-label="title"
        v-tooltip="title"
      >
        <progress-dot v-if="store.runningCount > 0" progress="loading" />
        <file-down class="size-5 shrink-0" aria-hidden="true" />
      </button>
    </popover-trigger>
    <popover-portal>
      <popover-content
        align="end"
        :side-offset="OFFSET"
        class="z-50 flex max-h-(--reka-popover-content-available-height) w-120 flex-col gap-3 rounded-xl border border-line bg-surface p-4 text-base shadow-lg"
      >
        <div class="flex items-center justify-between gap-3">
          <h2 class="font-semibold text-fg-strong">Exports</h2>
          <span
            class="text-sm text-fg-secondary"
            v-tooltip="
              'Fichiers des exports : ils restent dans le dossier des exports jusqu’à leur retrait'
            "
          >
            {{ formatUsage(store.usage) }}
          </span>
        </div>
        <ui-alert
          v-if="store.error"
          tone="error"
          :title="store.error"
          @close="store.error = null"
        />
        <div
          v-if="store.orphans.count > 0"
          class="flex items-center justify-between gap-3 text-sm text-fg-secondary"
        >
          <span
            v-tooltip="
              'Fichiers de Prisme dans le dossier des exports qu’aucun export ne référence : copiés dans le dossier, ou base interne remplacée'
            "
          >
            Non référencés : {{ formatUsage(store.orphans) }}
          </span>
          <ui-button
            label="Retirer"
            tone="error"
            small
            v-tooltip="'Supprimer les fichiers non référencés'"
            @click="store.clearOrphans()"
          />
        </div>
        <ul
          v-if="store.exports.length > 0"
          class="flex min-h-0 flex-col divide-y divide-line overflow-y-auto"
        >
          <export-item v-for="entry in store.exports" :key="entry.id" :entry="entry" />
        </ul>
        <p v-else class="text-fg-secondary">
          Aucun export : « Résultat complet en CSV », dans le menu de téléchargement d’un résultat,
          en lance un.
        </p>
      </popover-content>
    </popover-portal>
  </popover-root>
</template>
