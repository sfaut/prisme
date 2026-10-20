<!--
  Header of a panel of the layout (object explorer, data explorer,
  console): its title, or what replaces it (default slot), a line under the
  title (`subtitle` slot), the panel's actions (`actions` slot), and the
  button that folds the panel towards its edge. The button stands where
  the folded panel's rail shows its icon, or on the side of the main area:
  first, left of the title, for the panel on the right and the console at
  the bottom; last for the panel on the left. The folded panel leaves a
  rail on its edge (`PanelRail.vue`), with the matching icon.
-->
<script setup lang="ts">
import { PanelBottomClose, PanelLeftClose, PanelRightClose } from 'lucide-vue-next'

import UiButton from './ui/UiButton.vue'

const props = defineProps<{
  /**
   * Title of the panel.
   */
  title: string
  /**
   * Edge the panel folds towards: left (`start`), right (`end`) or bottom.
   */
  side: 'start' | 'end' | 'bottom'
}>()

const emit = defineEmits<{
  /**
   * The user folds the panel.
   */
  collapse: []
}>()

/**
 * Does the fold button come first, left of the title?
 */
const first = props.side !== 'start'

/**
 * Icon of the fold button, by edge.
 */
const ICONS = {
  start: PanelLeftClose,
  end: PanelRightClose,
  bottom: PanelBottomClose,
}
</script>

<template>
  <div class="flex min-h-control items-center gap-gap">
    <ui-button
      v-if="first"
      :icon="ICONS[props.side]"
      variant="ghost"
      small
      :aria-label="`Replier : ${title}`"
      :title="`Replier : ${title}`"
      @click="emit('collapse')"
    />
    <div class="flex min-w-0 flex-1 flex-col gap-0.5">
      <slot>
        <h2 class="text-title leading-none font-semibold text-fg-strong">{{ title }}</h2>
      </slot>
      <slot name="subtitle" />
    </div>
    <div class="flex items-center">
      <slot name="actions" />
      <ui-button
        v-if="!first"
        :icon="ICONS[props.side]"
        variant="ghost"
        small
        :aria-label="`Replier : ${title}`"
        :title="`Replier : ${title}`"
        @click="emit('collapse')"
      />
    </div>
  </div>
</template>
