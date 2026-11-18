<!--
  Tab of a document bar: the editor bar (`EditorTabBar.vue`) and the result
  bar of each editor (`EditorView.vue`). Both bars look and behave the same:

  - Look: the active tab is filled (`surface-raised`), its text darker;
    the line under the bar is drawn by the bar itself. The bottom edge
    of a tab stays free for the progress bar of the result tabs.
  - Height: its padding around its small buttons, taller than its text.
  - Focus: no outline. The tabs activate as they get the focus (arrow keys),
    so the focused tab is the active, filled one.
  - Progress, when the tab follows a work (`progress`: the query of a
    result tab, the last batch of an editor): a dot before the label,
    which pulses while the work runs and stays once it has ended, and a bar
    on the bottom edge of the tab: indeterminate (a segment that goes back
    and forth) when the completed share is unknown (`fraction`), green once
    the work has ended, orange once stopped by the user, red on failure,
    none while it waits for its turn.
    The bar grows by a transform, which the compositor of the browser
    animates alone: a transition of its width, started again by each batch
    of rows, made the page compute its layout and paint at every frame of a
    reception. Measured on 2026-10-31 for 1 million rows: received in 17 to
    20 s with the transform, 22 to 23 s with the width
    (`docs/PERFORMANCES.md`, chapter 12).
  - Content: the dot, what the bar puts in the default slot (label), then
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
import { computed } from 'vue'

import type { TabProgress } from '@/api/querySession'

import type { TabDrag } from './tabDrag'
import UiButton from '@/components/ui/UiButton.vue'
import UiProgressDot from '@/components/ui/UiProgressDot.vue'
import { vTooltip } from '@/components/ui/tooltip'

const props = defineProps<{
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
  /**
   * Progress of the work the tab follows; absent, neither dot nor bar.
   */
  progress?: TabProgress
  /**
   * Completed share of the work, from 0 to 1, `null` or absent when
   * unknown.
   */
  fraction?: number | null
}>()

const emit = defineEmits<{
  /**
   * The user closes the tab.
   */
  close: []
}>()

/**
 * Color of the bar, by progress: that of the action while the work runs.
 */
const TONES = {
  waiting: null,
  executing: 'primary',
  loading: 'primary',
  done: 'success',
  stopped: 'warning',
  error: 'error',
} as const satisfies Record<TabProgress, string | null>

/**
 * Color of the bar: none without progress, nor while the work waits for its
 * turn.
 */
const barTone = computed(() => (props.progress === undefined ? null : TONES[props.progress]))

/**
 * Completed share shown by the bar, `null` when unknown.
 */
const shownFraction = computed(() => props.fraction ?? null)
</script>

<template>
  <tabs-trigger
    :value="value"
    as="div"
    class="group relative flex max-w-107 min-w-0 shrink-0 cursor-pointer items-center gap-3 rounded-t-lg py-1 ps-3 pe-1 text-base font-medium text-fg-secondary select-none hover:text-fg-strong data-[state=active]:bg-surface-raised data-[state=active]:text-fg-strong"
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
    <ui-progress-dot v-if="progress" :progress="progress" />
    <slot />
    <span v-if="barTone" class="tab-progress" aria-hidden="true">
      <span
        class="tab-progress-bar"
        :class="[barTone, { indeterminate: shownFraction === null }]"
        :style="shownFraction === null ? undefined : { transform: `scaleX(${shownFraction})` }"
      />
    </span>
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
  box-shadow: inset var(--marker-width) 0 0 var(--color-primary);
}

.drop-after {
  box-shadow: inset calc(-1 * var(--marker-width)) 0 0 var(--color-primary);
}

/* Progress bar, over the whole width of the tab's bottom edge. */
.tab-progress {
  position: absolute;
  inset-inline: 0;
  inset-block-end: 0;
  block-size: var(--tab-progress-height);
  overflow: hidden;
}

/* The completed share scales the bar from its left edge (`scaleX`). A
   transition of its width would make the page draw at every frame of a
   reception (see the comment at the top). */
.tab-progress-bar {
  display: block;
  block-size: 100%;
  transform-origin: left;
  transition: transform 0.3s ease-out;

  &.primary {
    background: var(--color-primary);
  }

  &.success {
    background: var(--color-success);
  }

  &.warning {
    background: var(--color-warning);
  }

  &.error {
    background: var(--color-error);
  }

  /* Unknown share: a segment goes back and forth from one edge to the other. */
  &.indeterminate {
    inline-size: 30%;
    animation: sweep 1.2s ease-in-out infinite alternate;
  }
}

/* The segment takes 30 % of the tab: shifted by 70 / 30 of its width,
   it touches the right edge. */
@keyframes sweep {
  from {
    transform: translateX(0);
  }

  to {
    transform: translateX(calc(70% / 0.3));
  }
}
</style>
