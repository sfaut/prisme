<!--
  Header of a panel of the layout (object explorer, data explorer,
  console): its title, or what replaces it (default slot), a line under the
  title (`subtitle` slot), the panel's actions (`actions` slot), then, last,
  the button that folds the panel towards its edge. The folded panel leaves
  a rail on this edge (`PanelRail.vue`), with the matching icon.
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
    <div class="flex min-w-0 flex-1 flex-col gap-0.5">
      <slot>
        <h2 class="text-title leading-none font-semibold text-fg-strong">{{ title }}</h2>
      </slot>
      <slot name="subtitle" />
    </div>
    <div class="flex items-center">
      <slot name="actions" />
      <ui-button
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
