<!--
  Barre des onglets d'éditeur, tout en haut de la zone de travail : un
  onglet par éditeur SQL ouvert, chacun avec sa connexion, sa session et ses
  onglets de résultat (`QueryCell`).

  - Libellé : le nom donné par l'utilisateur, sinon le nom de la connexion
    de l'éditeur ; un double clic le renomme (`TabLabel`).
  - Croix (ou clic du milieu) : ferme l'éditeur ; il reste rouvrable
    24 heures depuis le menu « Rouvrir ».
  - Glisser un onglet le déplace, comme une colonne de la grille.
  - « + » ouvre un nouvel éditeur, sur la connexion de l'éditeur actif.

  À placer dans le `TabsRoot` de Reka UI qui porte les éditeurs (`App.vue`).
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
 * Libellé par défaut d'un éditeur : le nom de sa connexion, ou « Nouvel éditeur ».
 *
 * @param connectionId Connexion de l'éditeur, `null` si aucune.
 */
function defaultLabel(connectionId: number | null): string {
  const connection = connections.connections.find((item) => item.id === connectionId)
  return connection?.name ?? 'Nouvel éditeur'
}

/**
 * Déplacement d'un onglet d'éditeur par glissement ; le nouvel ordre est enregistré.
 */
const tabDrag = useTabDrag((from, to) => {
  void editors.reorder(movedItems(editors.editors, from, to).map((editor) => editor.id))
})

/**
 * Renomme un éditeur.
 *
 * @param editor Éditeur visé.
 * @param name Nouveau nom, `null` pour le libellé par défaut.
 */
function rename(editor: EditorEntry, name: string | null): void {
  void editors.rename(editor.id, name ?? '')
}

/**
 * Choix du menu « Rouvrir » : éditeurs fermés depuis moins de 24 heures.
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
    <tabs-list class="flex min-w-0 overflow-x-auto" aria-label="Éditeurs">
      <tabs-trigger
        v-for="(editor, index) in editors.editors"
        :key="editor.id"
        :value="editor.id"
        as="div"
        class="group relative -mb-px flex h-control max-w-tab min-w-0 shrink-0 cursor-pointer items-center gap-1 rounded-t-control border-b-2 border-transparent ps-control-x pe-1 text-ui font-medium text-fg-muted outline-none select-none hover:text-fg-strong focus-visible:bg-surface-muted data-[state=active]:border-accent data-[state=active]:text-fg-strong"
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
/* Repère d'insertion d'un onglet déplacé, comme pour les colonnes de la grille. */
.drop-before {
  box-shadow: inset var(--focus-width) 0 0 var(--accent);
}

.drop-after {
  box-shadow: inset calc(-1 * var(--focus-width)) 0 0 var(--accent);
}
</style>
