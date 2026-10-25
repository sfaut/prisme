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
  - At the right end, the button that reopens a closed editor, then the
    actions of the active editor (`actions` slot: its fold button, sent by
    its cell).

  To place in the Reka UI `TabsRoot` that holds the editors (`App.vue`).
-->
<script setup lang="ts">
import { History, Plus } from '@lucide/vue'
import { TabsList } from 'reka-ui'
import { computed } from 'vue'

import { useConnectionsStore } from '@/stores/connections'
import { type EditorEntry, useEditorsStore } from '@/stores/editors'

import DocumentTab from './DocumentTab.vue'
import TabLabel from './TabLabel.vue'
import { movedItems, useTabDrag } from './tabDrag'
import UiButton from './ui/UiButton.vue'
import UiDropdownMenu from './ui/UiDropdownMenu.vue'
import type { MenuItem } from './ui/menuItem'
import { vTooltip } from './ui/tooltip'

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
  <div class="flex border-b border-line">
    <tabs-list class="flex min-w-0 overflow-x-auto" aria-label="Éditeurs">
      <document-tab
        v-for="(editor, index) in editors.editors"
        :key="editor.id"
        :value="editor.id"
        :index="index"
        :count="editors.editors.length"
        :drag="tabDrag"
        close-label="Fermer l’éditeur"
        close-tooltip="Fermer l’éditeur (rouvrable pendant 24 heures)"
        @close="editors.close(editor.id)"
      >
        <tab-label
          :name="editor.name === '' ? null : editor.name"
          :default-label="defaultLabel(editor.connectionId)"
          @rename="(name) => rename(editor, name)"
        />
      </document-tab>
    </tabs-list>
    <div class="flex flex-1 items-center justify-between gap-1 ps-1">
      <ui-button
        :icon="Plus"
        variant="ghost"
        small
        aria-label="Nouvel éditeur"
        v-tooltip="'Nouvel éditeur, sur la connexion de l’éditeur actif'"
        @click="editors.create(editors.active?.connectionId ?? null)"
      />
      <div class="flex items-center">
        <ui-dropdown-menu :items="reopenItems">
          <ui-button
            :icon="History"
            variant="ghost"
            small
            aria-label="Rouvrir un éditeur fermé"
            v-tooltip="'Rouvrir un éditeur fermé depuis moins de 24 heures'"
            @click="editors.loadClosed()"
          />
        </ui-dropdown-menu>
        <slot name="actions" />
      </div>
    </div>
  </div>
</template>
