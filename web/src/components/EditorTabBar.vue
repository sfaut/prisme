<!--
  Bar of the editor tabs, at the very top of the work area: one tab per open
  SQL editor, each with its connection, its session and its result tabs
  (`QueryCell`).

  - Label: the name given by the user, otherwise the name of the editor's
    connection; a double click renames it (`TabLabel`).
  - Cross (or middle click): closes the editor; it can be reopened for
    24 hours from the "Rouvrir" menu.
  - Dragging a tab moves it, like a column of the grid.
  - "+" opens a new editor, on the connection of the active editor.

  To place in the Reka UI `TabsRoot` that holds the editors (`App.vue`).
-->
<script setup lang="ts">
import { History, Plus, X } from 'lucide-vue-next'
import { TabsList, TabsTrigger } from 'reka-ui'
import { computed } from 'vue'

import { useConnectionsStore } from '@/stores/connections'
import { type EditorEntry, useEditorsStore } from '@/stores/editors'

import TabLabel from './TabLabel.vue'
import { movedItems, useTabDrag } from './tabDrag'
import UiButton from './ui/UiButton.vue'
import UiDropdownMenu from './ui/UiDropdownMenu.vue'
import type { MenuItem } from './ui/menuItem'

const editors = useEditorsStore()
const connections = useConnectionsStore()

/**
 * Default label of an editor: the name of its connection, or "Nouvel éditeur".
 *
 * @param connectionId Connection of the editor, `null` if none.
 */
function defaultLabel(connectionId: number | null): string {
  const connection = connections.connections.find((item) => item.id === connectionId)
  return connection?.name ?? 'Nouvel éditeur'
}

/**
 * Moving an editor tab by dragging; the new order is saved.
 */
const tabDrag = useTabDrag((from, to) => {
  void editors.reorder(movedItems(editors.editors, from, to).map((editor) => editor.id))
})

/**
 * Renames an editor.
 *
 * @param editor Targeted editor.
 * @param name New name, `null` for the default label.
 */
function rename(editor: EditorEntry, name: string | null): void {
  void editors.rename(editor.id, name ?? '')
}

/**
 * Choices of the "Rouvrir" menu: editors closed less than 24 hours ago.
 */
const reopenItems = computed<MenuItem[]>(() => {
  if (editors.closed.length === 0) {
    return [{ label: 'Aucun éditeur fermé depuis 24 heures', icon: History, disabled: true }]
  }
  return editors.closed.map((editor) => {
    const start = editor.sql.replace(/\s+/g, ' ').trim()
    const label = editor.name || (start === '' ? 'Éditeur vide' : start)
    return {
      label: label.length > 50 ? `${label.slice(0, 50)}…` : label,
      icon: History,
      onSelect: () => void editors.reopen(editor.id),
    }
  })
})
</script>

<template>
  <div class="flex items-center gap-1 border-b border-line">
    <tabs-list class="-mb-px flex min-w-0 overflow-x-auto" aria-label="Éditeurs">
      <tabs-trigger
        v-for="(editor, index) in editors.editors"
        :key="editor.id"
        :value="editor.id"
        as="div"
        class="group relative flex h-control max-w-80 min-w-0 shrink-0 cursor-pointer items-center gap-1 rounded-t-control border-b-2 border-transparent ps-2 pe-1 text-ui font-medium text-fg-muted outline-none select-none hover:text-fg-strong focus-visible:bg-surface-muted data-[state=active]:border-accent data-[state=active]:text-fg-strong"
        :class="{
          'drop-before': tabDrag.dropAt.value === index,
          'drop-after':
            tabDrag.dropAt.value === editors.editors.length && index === editors.editors.length - 1,
        }"
        @pointerdown="tabDrag.onDown($event, index)"
        @pointermove="tabDrag.onMove"
        @pointerup="tabDrag.onDrop"
        @pointercancel="tabDrag.onDrop"
        @auxclick.middle.prevent="editors.close(editor.id)"
      >
        <tab-label
          :name="editor.name === '' ? null : editor.name"
          :default-label="defaultLabel(editor.connectionId)"
          @rename="(name) => rename(editor, name)"
        />
        <ui-button
          :icon="X"
          variant="ghost"
          small
          aria-label="Fermer l’éditeur"
          title="Fermer l’éditeur (rouvrable pendant 24 heures)"
          class="invisible group-hover:visible group-data-[state=active]:visible"
          @mousedown.stop
          @click.stop="editors.close(editor.id)"
        />
      </tabs-trigger>
    </tabs-list>
    <ui-button
      :icon="Plus"
      variant="ghost"
      small
      aria-label="Nouvel éditeur"
      title="Nouvel éditeur, sur la connexion de l’éditeur actif"
      @click="editors.create(editors.active?.connectionId ?? null)"
    />
    <ui-dropdown-menu :items="reopenItems">
      <ui-button
        :icon="History"
        variant="ghost"
        small
        aria-label="Rouvrir un éditeur fermé"
        title="Rouvrir un éditeur fermé depuis moins de 24 heures"
        class="ms-auto"
        @click="editors.loadClosed()"
      />
    </ui-dropdown-menu>
  </div>
</template>

<style scoped>
/* Insertion marker of a moved tab, as for the grid's columns. */
.drop-before {
  box-shadow: inset var(--focus-width) 0 0 var(--accent);
}

.drop-after {
  box-shadow: inset calc(-1 * var(--focus-width)) 0 0 var(--accent);
}
</style>
