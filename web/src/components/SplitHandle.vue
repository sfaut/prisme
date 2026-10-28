<!--
  Handle between two panels of a splitter (`SplitterGroup` of Reka UI): the
  line that separates them, which the pointer grabs a few pixels around
  (`hitAreaMargins` of Reka UI), with a grip in its middle. Reka UI moves it
  with the pointer and the arrow keys, within the bounds of the panels; a
  double click emits `toggle`, which folds or unfolds the panel it serves.
-->
<script setup lang="ts">
import { SplitterResizeHandle } from 'reka-ui'

import { vTooltip } from './ui/tooltip'

defineProps<{
  /**
   * Tooltip: what dragging and the double click do.
   */
  tooltip: string
}>()

const emit = defineEmits<{
  /**
   * Double click: fold or unfold the panel the handle serves.
   */
  toggle: []
}>()
</script>

<template>
  <splitter-resize-handle class="split-handle" v-tooltip="tooltip" @dblclick="emit('toggle')" />
</template>

<style scoped>
/* Above its neighbours: the band of the hover overlaps them. The line is
   the handle's border, never its background: the browser rounds the width
   of a border to whole pixels of the screen, where a box of one CSS pixel
   would cover one and a half at a scale of 150% and be painted one or two
   pixels thick, according to its position. */
.split-handle {
  position: relative;
  z-index: 1;
  flex: none;
  border: 0 solid var(--color-line);
  outline: none;

  /* Band of the hover, wider than the line, and the grip, centered on it:
     the border fills the handle, whose inner box is empty, so the center
     of the line lies half a line before it. */
  &::before,
  &::after {
    position: absolute;
    content: '';
  }

  &::after {
    border-radius: var(--grip-thickness);
    background: var(--grip-color);
  }

  /* Between panels side by side: a vertical line. */
  &[data-orientation='horizontal'] {
    border-inline-start-width: var(--line-width);

    &::before {
      inset-block: 0;
      inset-inline-start: calc((var(--line-width) + var(--resize-handle-width)) / -2);
      inline-size: var(--resize-handle-width);
    }

    &::after {
      inset-block-start: calc(50% - var(--grip-length) / 2);
      inset-inline-start: calc((var(--line-width) + var(--grip-thickness)) / -2);
      inline-size: var(--grip-thickness);
      block-size: var(--grip-length);
    }
  }

  /* Between panels one above the other: a horizontal line. */
  &[data-orientation='vertical'] {
    border-block-start-width: var(--line-width);

    &::before {
      inset-inline: 0;
      inset-block-start: calc((var(--line-width) + var(--resize-handle-width)) / -2);
      block-size: var(--resize-handle-width);
    }

    &::after {
      inset-block-start: calc((var(--line-width) + var(--grip-thickness)) / -2);
      inset-inline-start: calc(50% - var(--grip-length) / 2);
      inline-size: var(--grip-length);
      block-size: var(--grip-thickness);
    }
  }

  &[data-state='hover'],
  &[data-state='drag'],
  &[data-resize-handle-active='keyboard'] {
    &::before {
      background: var(--resize-hover);
    }

    &::after {
      background: var(--color-primary);
    }
  }
}
</style>
