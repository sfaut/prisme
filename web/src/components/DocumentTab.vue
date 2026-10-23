<!--
  Tab of a document bar: the editor bar (`EditorTabBar.vue`) and the result
  bar of each editor (`QueryCell.vue`). Both bars look and behave the same:

  - Look: the active tab is filled (`surface-elevated`), its text darker;
    the line under the bar is drawn by the bar itself. The bottom edge of a
    tab stays free for the progress bar of the result tabs.
  - Focus: no outline. The tabs activate as they get the focus (arrow keys),
    so the focused tab is the active, filled one.
  - Content: what the bar puts in the default slot (label, progress), then
    the tab's buttons side by side: those of the `actions` slot, then the
    cross. The buttons show on hovering the tab or on the active tab.
  - Cross (or middle click): emits `close`.
  - Dragging the tab moves it (`tabDrag.ts`), with an insertion marker as for
    the grid's columns.

  To place in the Reka UI `TabsList` of the bar.
-->
<script setup lang="ts">
import { X } from '@lucide/vue'
import { TabsTrigger } from 'reka-ui'

import type { TabDrag } from './tabDrag'
import UiButton from './ui/UiButton.vue'
import { vTooltip } from './ui/tooltip'

defineProps<{
  /**
   * Value of the tab in its `TabsRoot`.
   */
  value: number
  /**
   * Position of the tab in its bar.
   */
  index: number
  /**
   * Number of tabs in the bar: the marker after the last one.
   */
  count: number
  /**
   * Dragging of the bar's tabs.
   */
  drag: TabDrag
  /**
   * Accessible label of the cross.
   */
  closeLabel: string
  /**
   * Tooltip of the cross.
   */
  closeTooltip: string
}>()

const emit = defineEmits<{
  /**
   * The user closes the tab.
   */
  close: []
}>()
</script>

<template>
  <tabs-trigger
    :value="value"
    as="div"
    class="group relative flex h-control max-w-80 min-w-0 shrink-0 cursor-pointer items-center gap-2 rounded-t-control ps-2 pe-1 text-ui font-medium text-fg-muted outline-none select-none hover:text-fg-strong data-[state=active]:bg-surface-elevated data-[state=active]:text-fg-strong"
    :class="{
      'drop-before': drag.dropAt.value === index,
      'drop-after': drag.dropAt.value === count && index === count - 1,
    }"
    @pointerdown="drag.onDown($event, index)"
    @pointermove="drag.onMove"
    @pointerup="drag.onDrop"
    @pointercancel="drag.onDrop"
    @auxclick.middle.prevent="emit('close')"
  >
    <slot />
    <!-- The buttons side by side, without the gap that sets the label apart. -->
    <span class="flex items-center">
      <slot name="actions" />
      <ui-button
        :icon="X"
        variant="ghost"
        small
        :aria-label="closeLabel"
        v-tooltip="closeTooltip"
        class="invisible group-hover:visible group-data-[state=active]:visible"
        @mousedown.stop
        @click.stop="emit('close')"
      />
    </span>
  </tabs-trigger>
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
