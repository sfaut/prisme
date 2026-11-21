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
  At the bottom of the bar, a button resets the display of the editor (its
  panels, its navigator, the grids and the sections of the Pivoteur of its
  result tabs: `editorDisplays.reset`), and the last one shares the room
  again: the open panels of the view take equal heights, the side panels
  and the console their default size.
  Each button has its shortcut, Ctrl-K then a key (`panelChord.ts`), given
  in its tooltip; the bar of the active editor alone follows them.

  The panels are those of the display of the editor (`editorDisplays`
  store), kept even while it has no connection.
-->
<script setup lang="ts">
import {
  Braces,
  Code,
  LayoutDashboard,
  ListTree,
  RotateCcw,
  SquareTerminal,
  Table,
  TableProperties,
} from '@lucide/vue'
import { type Component, computed, watchEffect } from 'vue'

import type { Panels } from '@/api/generated/Panels'

import { useEditorDisplaysStore } from '@/stores/editorDisplays'
import { useEditorsStore } from '@/stores/editors'
import { type Tooltip, vTooltip } from '@/components/ui/tooltip'

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

const displays = useEditorDisplaysStore()
const editors = useEditorsStore()

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
  const panels = displays.displayOf(props.editorId).panels
  const toggle = (name: keyof Panels) => () => (panels[name].open = !panels[name].open)
  return [
    [
      {
        label: 'Navigateur d’objets',
        icon: ListTree,
        key: 'n',
        open: panels.navigator.open,
        toggle: toggle('navigator'),
      },
    ],
    [
      {
        label: 'Éditeur',
        icon: Code,
        key: 'e',
        open: panels.editor.open,
        toggle: toggle('editor'),
      },
      {
        label: 'Résultat',
        icon: Table,
        key: 'r',
        open: panels.results.open,
        toggle: toggle('results'),
      },
      {
        label: 'Pivoteur',
        icon: TableProperties,
        key: 'p',
        open: panels.pivot.open,
        toggle: toggle('pivot'),
      },
    ],
    [
      {
        label: 'Explorateur de données',
        icon: Braces,
        key: 'x',
        open: panels.explorer.open,
        toggle: toggle('explorer'),
      },
      {
        label: 'Console',
        icon: SquareTerminal,
        key: 'c',
        open: panels.console.open,
        toggle: toggle('console'),
      },
    ],
  ]
})

/**
 * Tooltip of a button: the name of its panel, and its shortcut.
 *
 * @param button Button of a panel.
 */
function tooltip(button: PanelButton): Tooltip {
  return { title: button.label, shortcut: chordLabel(button.key) }
}

/**
 * Shares the room again: the results and the Pivoteur share the height
 * with the panel that fills, in equal parts (a height `null`), and the side
 * panels and the console take their default size.
 */
function distribute(): void {
  const panels = displays.displayOf(props.editorId).panels
  for (const name of ['results', 'pivot', 'console', 'navigator', 'explorer'] as const) {
    panels[name].size = null
  }
}

/**
 * Second key of the shortcut that shares the room.
 */
const DISTRIBUTE_KEY = '='

/**
 * Brings the display of the editor back to the defaults.
 */
function reset(): void {
  const editor = editors.editors.find((candidate) => candidate.id === props.editorId)
  displays.reset(props.editorId, editor?.connectionId ?? null)
}

/**
 * Second key of the shortcut that resets the display.
 */
const RESET_KEY = '0'

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
  if (key === RESET_KEY) {
    reset()
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
    <div class="flex flex-col gap-1">
      <button
        type="button"
        :class="BUTTON"
        aria-label="Réinitialiser l’affichage"
        v-tooltip:right="{
          title: 'Réinitialiser l’affichage',
          description:
            'Volets, navigateur, grilles et Pivoteur de cet éditeur reviennent à leur état par défaut',
          shortcut: chordLabel(RESET_KEY),
        }"
        @click="reset"
      >
        <RotateCcw class="size-5" aria-hidden="true" />
      </button>
      <button
        type="button"
        :class="BUTTON"
        aria-label="Répartir les volets"
        v-tooltip:right="{
          title: 'Répartir les volets',
          description:
            'Hauteurs égales pour les volets ouverts, taille par défaut pour les côtés et la console',
          shortcut: chordLabel(DISTRIBUTE_KEY),
        }"
        @click="distribute"
      >
        <LayoutDashboard class="size-5" aria-hidden="true" />
      </button>
    </div>
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
