<!--
  Activity bar of an editor's tab: a dark column on its left edge, under the
  editors' tabs, whose buttons open and close the panels of the tab. First
  the object navigator; then the panels of the view, from top to bottom
  (editor, results, Pivoteur); then the data explorer and the console. A
  closed panel leaves no trace in the tab: its button, released, is the way
  back. A pressed button is lit, with a mark of the primary color on the
  bar's edge. The tooltip says what the click does, with the panel's name.

  The layouts come from the stores of the panels: `navigator`,
  `editorLayouts` (the view's panels, kept for this editor even while it
  has no connection), `explorer` and `console`.
-->
<script setup lang="ts">
import { Braces, Code, ListTree, SquareTerminal, Table, TableProperties } from '@lucide/vue'
import { type Component, computed } from 'vue'

import { useConsoleStore } from '@/stores/console'
import { useEditorLayoutsStore } from '@/stores/editorLayouts'
import { useExplorerStore } from '@/stores/explorer'
import { useNavigatorStore } from '@/stores/navigator'
import { vTooltip } from '@/components/ui/tooltip'

const props = defineProps<{
  /**
   * Id of the editor of the tab.
   */
  editorId: number
}>()

const navigator = useNavigatorStore()
const layouts = useEditorLayoutsStore()
const explorer = useExplorerStore()
const consoleStore = useConsoleStore()

/**
 * Button of a panel.
 */
interface PanelButton {
  /**
   * Name of the panel, its accessible label.
   */
  label: string
  /**
   * Name of the panel in a sentence, with its article.
   */
  named: string
  /**
   * Icon of the panel.
   */
  icon: Component
  /**
   * Is the panel open?
   */
  open: boolean
  /**
   * Opens or closes the panel.
   */
  toggle: () => void
}

/**
 * Buttons of the bar, in groups set apart by a line.
 */
const groups = computed<PanelButton[][]>(() => {
  const objects = navigator.layoutOf(props.editorId)
  const view = layouts.layoutOf(props.editorId)
  const data = explorer.layoutOf(props.editorId)
  return [
    [
      {
        label: 'Navigateur d’objets',
        named: 'le navigateur d’objets',
        icon: ListTree,
        open: objects.open,
        toggle: () => (objects.open = !objects.open),
      },
    ],
    [
      {
        label: 'Éditeur',
        named: 'l’éditeur',
        icon: Code,
        open: !view.editorFolded,
        toggle: () => (view.editorFolded = !view.editorFolded),
      },
      {
        label: 'Résultat',
        named: 'le résultat',
        icon: Table,
        open: !view.resultsFolded,
        toggle: () => (view.resultsFolded = !view.resultsFolded),
      },
      {
        label: 'Pivoteur',
        named: 'le Pivoteur',
        icon: TableProperties,
        open: !view.pivotFolded,
        toggle: () => (view.pivotFolded = !view.pivotFolded),
      },
    ],
    [
      {
        label: 'Explorateur de données',
        named: 'l’explorateur de données',
        icon: Braces,
        open: data.open,
        toggle: () => (data.open = !data.open),
      },
      {
        label: 'Console',
        named: 'la console',
        icon: SquareTerminal,
        open: consoleStore.open,
        toggle: () => (consoleStore.open = !consoleStore.open),
      },
    ],
  ]
})

/**
 * Tooltip of a button: what its click does.
 *
 * @param button Button of a panel.
 */
function tooltip(button: PanelButton): string {
  return `${button.open ? 'Fermer' : 'Ouvrir'} ${button.named}`
}
</script>

<template>
  <nav class="flex w-12 shrink-0 flex-col items-center gap-2 bg-activity py-2" aria-label="Volets">
    <template v-for="(group, index) in groups" :key="index">
      <span v-if="index > 0" class="w-5 border-t border-activity-line" aria-hidden="true" />
      <div class="flex flex-col gap-1">
        <button
          v-for="button in group"
          :key="button.label"
          type="button"
          class="activity-button relative inline-flex size-8 cursor-pointer items-center justify-center rounded-lg text-on-activity hover:text-on-activity-strong aria-pressed:bg-on-activity-strong/10 aria-pressed:text-on-activity-strong"
          :aria-label="button.label"
          :aria-pressed="button.open"
          v-tooltip="tooltip(button)"
          @click="button.toggle()"
        >
          <component :is="button.icon" class="size-5" aria-hidden="true" />
        </button>
      </div>
    </template>
  </nav>
</template>

<style scoped>
@reference "@/assets/main.css";

/* Mark of a pressed button, against the bar's edge, two steps away. */
.activity-button[aria-pressed='true']::before {
  content: '';
  position: absolute;
  inset-inline-start: calc(--spacing(2) * -1);
  inset-block: --spacing(2);
  border-inline-start: var(--marker-width) solid var(--color-primary);
}
</style>
