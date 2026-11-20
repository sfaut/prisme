<!--
  Activity bar of an editor's tab: a dark column on its left edge, under the
  editors' tabs, whose buttons open and close the panels of the tab. First
  the object navigator; then the panels of the view, from top to bottom
  (editor, results, Pivoteur); then the data explorer and the console. A
  closed panel leaves no trace in the tab: its button, released, is the way
  back. A pressed button is lit, with a mark of the primary color on the
  bar's edge. The tooltip, on the right of the button, names the panel,
  then its shortcut, smaller: the state of the button says what the click
  does.
  At the bottom of the bar, a button shares the room again: the open panels
  of the view take equal heights, the side panels and the console their
  default size.
  Each button has its shortcut, Ctrl-K then a key (`panelChord.ts`), given
  in its tooltip; the bar of the active editor alone follows them.

  The layouts come from the stores of the panels: `navigator`,
  `editorLayouts` (the view's panels and the console, kept for this editor
  even while it has no connection) and `explorer`.
-->
<script setup lang="ts">
import {
  Braces,
  Code,
  LayoutDashboard,
  ListTree,
  SquareTerminal,
  Table,
  TableProperties,
} from '@lucide/vue'
import { type Component, computed, watchEffect } from 'vue'

import { useEditorLayoutsStore } from '@/stores/editorLayouts'
import { useExplorerStore } from '@/stores/explorer'
import { useNavigatorStore } from '@/stores/navigator'
import { vTooltip } from '@/components/ui/tooltip'

import { chordLabel, followChords } from './panelChord'

const props = defineProps<{
  /**
   * Id of the editor of the tab.
   */
  editorId: number
  /**
   * Is the editor the active one? Its bar follows the shortcuts.
   */
  active: boolean
}>()

const navigator = useNavigatorStore()
const layouts = useEditorLayoutsStore()
const explorer = useExplorerStore()

/**
 * Button of a panel.
 */
interface PanelButton {
  /**
   * Name of the panel, its accessible label and its tooltip.
   */
  label: string
  /**
   * Icon of the panel.
   */
  icon: Component
  /**
   * Second key of its shortcut, after Ctrl-K.
   */
  key: string
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
        icon: ListTree,
        key: 'n',
        open: objects.open,
        toggle: () => (objects.open = !objects.open),
      },
    ],
    [
      {
        label: 'Éditeur',
        icon: Code,
        key: 'e',
        open: !view.editorFolded,
        toggle: () => (view.editorFolded = !view.editorFolded),
      },
      {
        label: 'Résultat',
        icon: Table,
        key: 'r',
        open: !view.resultsFolded,
        toggle: () => (view.resultsFolded = !view.resultsFolded),
      },
      {
        label: 'Pivoteur',
        icon: TableProperties,
        key: 'p',
        open: !view.pivotFolded,
        toggle: () => (view.pivotFolded = !view.pivotFolded),
      },
    ],
    [
      {
        label: 'Explorateur de données',
        icon: Braces,
        key: 'x',
        open: data.open,
        toggle: () => (data.open = !data.open),
      },
      {
        label: 'Console',
        icon: SquareTerminal,
        key: 'c',
        open: !view.consoleFolded,
        toggle: () => (view.consoleFolded = !view.consoleFolded),
      },
    ],
  ]
})

/**
 * Tooltip of a button: the name of its panel, and its shortcut.
 *
 * @param button Button of a panel.
 */
function tooltip(button: PanelButton): { text: string; shortcut: string } {
  return { text: button.label, shortcut: chordLabel(button.key) }
}

/**
 * Shares the room again: the results and the Pivoteur share the height
 * with the panel that fills, in equal parts (a height `null`), and the side
 * panels and the console take their default size.
 */
function distribute(): void {
  const view = layouts.layoutOf(props.editorId)
  view.results = null
  view.pivot = null
  view.console = null
  navigator.layoutOf(props.editorId).width = null
  explorer.layoutOf(props.editorId).width = null
}

/**
 * Second key of the shortcut that shares the room.
 */
const DISTRIBUTE_KEY = '='

/**
 * Runs the action of the second key of a shortcut.
 *
 * @param key Second key, lowercased.
 * @returns Is the key one of the bar's?
 */
function runChord(key: string): boolean {
  if (key === DISTRIBUTE_KEY) {
    distribute()
    return true
  }
  const button = groups.value.flat().find((candidate) => candidate.key === key)
  button?.toggle()
  return button !== undefined
}

// The bar of the active editor follows the shortcuts, while it is active.
watchEffect((onCleanup) => {
  if (props.active) {
    onCleanup(followChords(runChord))
  }
})

/**
 * Classes of a button of the bar.
 */
const BUTTON =
  'activity-button relative inline-flex size-8 cursor-pointer items-center justify-center rounded-lg text-on-activity hover:text-on-activity-strong aria-pressed:bg-on-activity-strong/10 aria-pressed:text-on-activity-strong'
</script>

<template>
  <nav
    class="flex w-12 shrink-0 flex-col items-center justify-between bg-activity py-2"
    aria-label="Volets"
  >
    <div class="flex flex-col items-center gap-2">
      <template v-for="(group, index) in groups" :key="index">
        <span v-if="index > 0" class="w-5 border-t border-activity-line" aria-hidden="true" />
        <div class="flex flex-col gap-1">
          <button
            v-for="button in group"
            :key="button.label"
            type="button"
            :class="BUTTON"
            :aria-label="button.label"
            :aria-pressed="button.open"
            v-tooltip:right="tooltip(button)"
            @click="button.toggle()"
          >
            <component :is="button.icon" class="size-5" aria-hidden="true" />
          </button>
        </div>
      </template>
    </div>
    <button
      type="button"
      :class="BUTTON"
      aria-label="Répartir les volets"
      v-tooltip:right="{ text: 'Répartir les volets', shortcut: chordLabel(DISTRIBUTE_KEY) }"
      @click="distribute"
    >
      <LayoutDashboard class="size-5" aria-hidden="true" />
    </button>
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
