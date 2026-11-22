<!--
  Left pane of the connections manager: a filter, the "Nouvelle connexion"
  button, the connection being created right under it, then the saved
  connections grouped by family of sources (`sourceTypes.ts`).

  The servers' family folds and unfolds; the files' family has no
  connection yet: it shows "à venir". A creation starts from the "Nouvelle
  connexion" button alone.

  Each connection shows its icon, its name and where it points, and a dot:
  green once connected, grey never. A connection changed and not saved shows
  its name as typed, "modifiée, non enregistrée" in place of its place, and
  an amber dot; so does the creation in progress, with its step. The filter
  keeps the connections whose name or place holds the text, case aside.
-->
<script setup lang="ts">
import { ChevronDown, ChevronRight, Plus, Search } from '@lucide/vue'
import { computed, ref } from 'vue'

import type { Creation, ListEntry, ManagerSelection } from './connectionManager'
import { SOURCE_FAMILIES } from './sourceTypes'

import UiButton from '@/components/ui/UiButton.vue'
import UiInput from '@/components/ui/UiInput.vue'

const props = defineProps<{
  /**
   * Saved connections, by name.
   */
  entries: ListEntry[]
  /**
   * Creation in progress, `null` if none.
   */
  creation: Creation | null
  /**
   * What the right pane shows.
   */
  selected: ManagerSelection
}>()

const emit = defineEmits<{
  /**
   * Shows a connection, or the creation in progress (`new`).
   */
  select: [selection: number | 'new']
  /**
   * Starts a creation, or goes back to the one in progress.
   */
  create: []
}>()

/**
 * Text of the filter.
 */
const query = ref('')

/**
 * Is the servers' family folded?
 */
const folded = ref(false)

/**
 * Connections kept by the filter.
 */
const shown = computed(() => {
  const text = query.value.trim().toLowerCase()
  return text === ''
    ? props.entries
    : props.entries.filter(
        (entry) =>
          entry.name.toLowerCase().includes(text) || entry.place.toLowerCase().includes(text),
      )
})

/**
 * Classes of a line of the list, the one shown on the right raised.
 *
 * @param active Is it the line shown on the right?
 */
function lineClasses(active: boolean): string {
  return active
    ? 'border-line bg-surface text-fg-strong'
    : 'border-transparent text-fg hover:bg-surface-strong hover:text-fg-strong'
}
</script>

<template>
  <nav
    class="flex w-70 shrink-0 flex-col gap-3 overflow-y-auto border-e border-line bg-surface-sunken p-3"
    aria-label="Connexions enregistrées"
  >
    <ui-input
      v-model="query"
      :icon="Search"
      type="search"
      placeholder="Filtrer"
      aria-label="Filtrer les connexions"
    />
    <ui-button :icon="Plus" label="Nouvelle connexion" @click="emit('create')" />

    <button
      v-if="creation"
      type="button"
      class="group flex w-full cursor-pointer items-center gap-3 rounded-control border px-3 py-1 text-start"
      :class="lineClasses(selected === 'new')"
      :aria-current="selected === 'new' ? 'true' : undefined"
      @click="emit('select', 'new')"
    >
      <plus
        class="size-5 shrink-0"
        :class="selected === 'new' ? 'text-primary' : 'text-fg-secondary'"
        aria-hidden="true"
      />
      <span class="flex min-w-0 flex-1 flex-col">
        <span class="truncate font-medium italic">{{
          creation.form.name || 'Nouvelle connexion'
        }}</span>
        <span class="truncate text-xs text-warning">
          création en cours, étape {{ creation.step }} sur 3
        </span>
      </span>
      <span class="size-2 shrink-0 rounded-full bg-warning" aria-hidden="true" />
    </button>

    <section
      v-for="family in SOURCE_FAMILIES"
      :key="family.key"
      class="flex flex-col gap-1"
      :aria-label="family.title"
    >
      <div class="flex items-center gap-1 ps-1">
        <button
          v-if="family.ready"
          type="button"
          class="flex h-7 flex-1 cursor-pointer items-center gap-2 text-sm font-semibold text-fg-secondary hover:text-fg-strong"
          :aria-expanded="!folded"
          @click="folded = !folded"
        >
          <component :is="folded ? ChevronRight : ChevronDown" class="size-4" aria-hidden="true" />
          {{ family.title }}
          <span class="font-medium text-fg-faint">{{ entries.length }}</span>
        </button>
        <span
          v-else
          class="flex h-7 flex-1 items-center gap-2 text-sm font-semibold text-fg-secondary"
        >
          <chevron-right class="size-4" aria-hidden="true" />
          {{ family.title }}
          <span class="rounded-full bg-fg-secondary/15 px-1 text-xs font-medium text-fg-secondary">
            <span class="text-trim">à venir</span>
          </span>
        </span>
      </div>

      <div v-if="family.ready && !folded" class="flex flex-col gap-1">
        <button
          v-for="entry in shown"
          :key="entry.id"
          type="button"
          class="flex w-full cursor-pointer items-center gap-3 rounded-control border px-3 py-1 text-start"
          :class="lineClasses(selected === entry.id)"
          :aria-current="selected === entry.id ? 'true' : undefined"
          @click="emit('select', entry.id)"
        >
          <component
            :is="family.types[0]!.icon"
            class="size-5 shrink-0"
            :class="selected === entry.id ? 'text-primary' : 'text-fg-secondary'"
            aria-hidden="true"
          />
          <span class="flex min-w-0 flex-1 flex-col">
            <span class="truncate font-medium">{{ entry.name || '(sans nom)' }}</span>
            <span v-if="entry.changed" class="truncate text-xs text-warning">
              modifiée, non enregistrée
            </span>
            <span v-else class="truncate font-mono text-xs text-fg-secondary">
              {{ entry.place }}
            </span>
          </span>
          <span
            class="size-2 shrink-0 rounded-full"
            :class="entry.changed ? 'bg-warning' : entry.connected ? 'bg-success' : 'bg-fg-faint'"
            aria-hidden="true"
          />
        </button>
        <p v-if="shown.length === 0 && query.trim() !== ''" class="p-3 text-sm text-fg-secondary">
          Aucune connexion ne correspond.
        </p>
      </div>
    </section>
  </nav>
</template>
