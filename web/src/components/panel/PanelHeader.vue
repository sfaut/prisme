<!--
  Header of a panel of the layout (object navigator, data explorer, editor,
  Pivoteur, console): its title, or what replaces it (default slot), what
  follows the title on its line (`inline` slot: the buttons of the editor,
  the tabs of the console), a line under the title (`subtitle` slot), the
  panel's actions (`actions` slot), and the button that folds the panel
  towards its edge. The button stands on the
  outer edge of a side panel, where the folded panel's rail shows its icon,
  so that the panel opens and folds without moving the pointer: first, left
  of the title, for the panel on the left; at the right end, after the
  actions, for the panel on the right, as for a panel at the top or at the
  bottom. A folded panel leaves a rail on its edge (`PanelRail.vue`), with
  the matching icon.
  The button, the title, what follows it and the actions share a first
  line, at least of a control's height, centred on it, at the top of the
  panel, whatever comes below (subtitle): the fold buttons of the panels
  and the icons of the rails stand at the same height, and tabs taller
  than a control (the console's) keep the buttons centred on them.
-->
<script setup lang="ts">
import { PanelBottomClose, PanelLeftClose, PanelRightClose, PanelTopClose } from '@lucide/vue'

import UiButton from '@/components/ui/UiButton.vue'
import { vTooltip } from '@/components/ui/tooltip'

const props = defineProps<{
  /**
   * Title of the panel.
   */
  title: string
  /**
   * Edge the panel folds towards: left (`start`), right (`end`), top (the
   * panels of an editor's view, which fold up into the rail at their top)
   * or bottom (the console).
   */
  side: 'start' | 'end' | 'top' | 'bottom'
}>()

const emit = defineEmits<{
  /**
   * The user folds the panel.
   */
  collapse: []
}>()

/**
 * Does the fold button come first, left of the title? On the left panel,
 * whose outer edge is on the left.
 */
const first = props.side === 'start'

/**
 * Icon of the fold button, by edge: its chevron points the way the panel
 * folds (up for the panels of an editor's view, down for the console).
 */
const ICONS = {
  start: PanelLeftClose,
  end: PanelRightClose,
  top: PanelTopClose,
  bottom: PanelBottomClose,
}

/**
 * Icon of the button.
 */
const icon = ICONS[props.side]

/**
 * Label and tooltip of the button.
 */
const buttonLabel = `Replier : ${props.title}`
</script>

<template>
  <div class="flex flex-col gap-1">
    <div class="flex min-h-8 min-w-0 items-center gap-3">
      <ui-button
        v-if="first"
        :icon="icon"
        variant="ghost"
        small
        :aria-label="buttonLabel"
        v-tooltip="buttonLabel"
        @click="emit('collapse')"
      />
      <div class="flex min-w-0 flex-1 items-center gap-3 self-stretch">
        <slot>
          <h2 class="shrink-0 text-lg leading-none font-semibold text-fg-strong">{{ title }}</h2>
        </slot>
        <slot name="inline" />
      </div>
      <div class="flex items-center self-stretch">
        <slot name="actions" />
        <ui-button
          v-if="!first"
          :icon="icon"
          variant="ghost"
          small
          :aria-label="buttonLabel"
          v-tooltip="buttonLabel"
          @click="emit('collapse')"
        />
      </div>
    </div>
    <slot name="subtitle" />
  </div>
</template>
