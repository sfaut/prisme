<!--
  Tab of a query, in the result tab bar of its editor (a `DocumentTab`).

  - Progress dot, then label: the name given by the user, or the number and
    the start of the query; a double click renames it (`DocumentTabLabel`).
  - Stop, while the query runs or waits for its turn (`stoppable`): it
    stops this query alone, the rows received kept. Always visible then.
  - Pin: a pinned tab stays when the query is run again. It is always
    visible on a pinned tab; otherwise, like the cross, on hovering the tab
    or on the active tab (the tab carries the `group` class).
  - Cross (or middle click): closes the tab; the editor view stops its query if
    it is running.
  - Progress bar on the bottom edge of the tab (`DocumentTab`), following
    the completed share of the query (`tabFraction`).
-->
<script setup lang="ts">
import { Pin, Square } from '@lucide/vue'

import { type QueryTab, tabFraction, tabProgress } from '@/api/querySession'

import DocumentTab from '@/components/tab/DocumentTab.vue'
import DocumentTabLabel from '@/components/tab/DocumentTabLabel.vue'
import type { TabDrag } from '@/components/tab/tabDrag'
import UiButton from '@/components/ui/UiButton.vue'
import { vTooltip } from '@/components/ui/tooltip'

defineProps<{
  /**
   * Query of the tab.
   */
  tab: QueryTab
  /**
   * Default label, without a given name.
   */
  defaultLabel: string
  /**
   * Position of the tab in the bar.
   */
  index: number
  /**
   * Number of tabs in the bar.
   */
  count: number
  /**
   * Dragging of the bar's tabs.
   */
  drag: TabDrag
  /**
   * Can the query be stopped (`stoppable`)? The stop button then shows.
   */
  stoppable: boolean
}>()

const emit = defineEmits<{
  /**
   * The user closes the tab.
   */
  close: []
  /**
   * The user stops the query of the tab.
   */
  stop: []
  /**
   * The user pins or unpins the tab.
   */
  'toggle-pin': []
  /**
   * The user renames the tab; `null` gives back the default label.
   */
  rename: [name: string | null]
}>()
</script>

<template>
  <document-tab
    :value="tab.id"
    :index="index"
    :count="count"
    :drag="drag"
    :progress="tabProgress(tab)"
    :fraction="tabFraction(tab)"
    close-label="Fermer l’onglet"
    :close-tooltip="{ title: 'Fermer l’onglet', description: 'Arrête sa requête si elle tourne' }"
    @close="emit('close')"
  >
    <document-tab-label
      :name="tab.name"
      :default-label="defaultLabel"
      @rename="emit('rename', $event)"
    />
    <template #actions>
      <ui-button
        v-if="stoppable"
        :icon="Square"
        variant="ghost"
        tone="error"
        small
        aria-label="Arrêter la requête"
        v-tooltip="{ title: 'Arrêter la requête', description: 'Les lignes reçues restent' }"
        @mousedown.stop
        @click.stop="emit('stop')"
      />
      <ui-button
        :icon="Pin"
        variant="ghost"
        :tone="tab.pinned ? 'primary' : 'neutral'"
        small
        :aria-pressed="tab.pinned"
        :aria-label="tab.pinned ? 'Désépingler l’onglet' : 'Épingler l’onglet'"
        v-tooltip="
          tab.pinned
            ? {
                title: 'Désépingler l’onglet',
                description: 'Il sera remplacé à la prochaine exécution',
              }
            : { title: 'Épingler l’onglet', description: 'Il reste quand la requête est relancée' }
        "
        :class="
          tab.pinned ? 'visible' : 'invisible group-hover:visible group-data-[state=active]:visible'
        "
        @mousedown.stop
        @click.stop="emit('toggle-pin')"
      />
    </template>
  </document-tab>
</template>
