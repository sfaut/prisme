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
@reference "@/assets/main.css";

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

  /* Band of the hover and grip: each one a box that covers exactly the line
     (a border as wide as it, in a box of no width set against the handle's
     inner edge), whose outline draws the band or the grip on both sides. The
     browser rounds the width of an outline to whole pixels of the screen, as
     that of a border, and draws it around the pixels of the line: the band
     and the grip stay centered on the line wherever the handle falls. Boxes
     placed around the line by a calc() had their edges rounded on their own,
     and slid by a pixel to a side at some positions. */
  &::before,
  &::after {
    position: absolute;
    border: 0 solid transparent;
    content: '';
  }

  /* One step on each side of the line: a band of two steps. */
  &::before {
    outline: --spacing(1) solid transparent;
  }

  /* Half the grip on each side of the line, and at each of its ends. */
  &::after {
    border-color: var(--grip-color);
    border-radius: var(--grip-thickness);
    outline: calc(var(--grip-thickness) / 2) solid var(--grip-color);
  }

  /* Between panels side by side: a vertical line. The band runs along the
     whole line, its outline cut at the ends of the handle. */
  &[data-orientation='horizontal'] {
    border-inline-start-width: var(--line-width);
    overflow-block: clip;

    &::before,
    &::after {
      inset-inline-end: 0;
      inline-size: 0;
      border-inline-start-width: var(--line-width);
    }

    &::before {
      inset-block: 0;
    }

    &::after {
      inset-block-start: calc(50% - (var(--grip-length) - var(--grip-thickness)) / 2);
      block-size: calc(var(--grip-length) - var(--grip-thickness));
    }
  }

  /* Between panels one above the other: a horizontal line. */
  &[data-orientation='vertical'] {
    border-block-start-width: var(--line-width);
    overflow-inline: clip;

    &::before,
    &::after {
      inset-block-end: 0;
      block-size: 0;
      border-block-start-width: var(--line-width);
    }

    &::before {
      inset-inline: 0;
    }

    &::after {
      inset-inline-start: calc(50% - (var(--grip-length) - var(--grip-thickness)) / 2);
      inline-size: calc(var(--grip-length) - var(--grip-thickness));
    }
  }

  &[data-state='hover'],
  &[data-state='drag'],
  &[data-resize-handle-active='keyboard'] {
    &::before {
      border-color: var(--resize-hover);
      outline-color: var(--resize-hover);
    }

    &::after {
      border-color: var(--color-primary);
      outline-color: var(--color-primary);
    }
  }
}
</style>
