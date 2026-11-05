<!--
  Handle between two panels of a splitter (`SplitterGroup` of Reka UI): the
  line that separates them, which the pointer grabs a few pixels around,
  with a grip in its middle. Reka UI moves it with the pointer and the
  arrow keys, within the bounds of the panels; a double click emits
  `toggle`, which folds or unfolds the panel it serves.

  The zone grabbed around the line is the handle's own: Reka UI finds the
  handle under the pointer by its margins (`hitAreaMargins`), and the
  handle covers the same zone with a transparent box (`split-hit`). Without
  the box, the pointer near the line stayed over the neighbouring panel for
  the browser: the rail of a folded panel darkened and showed its tooltip
  while the handle showed its hover, the two at once.
-->
<script setup lang="ts">
import { SplitterResizeHandle } from 'reka-ui'

import { cssPixels } from '@/cssLength'
import { vTooltip } from './ui/tooltip'

/**
 * Margins of the zone grabbed on each side of the line, in pixels, for a
 * mouse (`fine`) and a touch screen (`coarse`): Reka UI takes only pixels
 * (`docs/DEVELOPER.md`, section "Lengths in pixels"). The box `split-hit`
 * covers the same zone with the same theme variables.
 */
const HIT_AREA_MARGINS = {
  fine: cssPixels('--split-hit-margin', 6),
  coarse: cssPixels('--split-hit-margin-coarse', 15),
}

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
  <splitter-resize-handle
    class="split-handle"
    :hit-area-margins="HIT_AREA_MARGINS"
    v-tooltip="tooltip"
    @dblclick="emit('toggle')"
  >
    <span class="split-hit" aria-hidden="true" />
  </splitter-resize-handle>
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

  /* Box of the zone grabbed: transparent, above the neighbours as the
     handle is, it takes the pointer from them over the whole zone where
     Reka UI takes it for the handle, the margin on each side of the line
     (`HIT_AREA_MARGINS`): the panel underneath shows no hover nor tooltip
     of its own there, and the tooltip shown is the handle's.

     Its length is that of the line. Across it, the box is the margin
     before the line, a transparent border as wide as the line, then the
     margin after it (`--hit-margin-after`), its far edge as far beyond the
     handle's inner edge. The border is rounded to whole pixels of the
     screen as the line is, so the box follows the line at any scale.
     Reka UI reads the mouse in whole CSS pixels (`main.css`, at
     `--split-hit-rounding`): after the line, the margin of the box is one
     pixel longer, so that the box holds the whole zone of Reka UI, which
     never marks the handle hovered with the pointer over the panel. At
     the edges of the zone, a strip thinner than a CSS pixel belongs to the
     box without the hover of the handle, where nothing shows a hover.

     The handle clips only along its line: the box overflows across it. */
  .split-hit {
    --hit-margin: var(--split-hit-margin);
    --hit-margin-after: calc(var(--hit-margin) + var(--split-hit-rounding));

    position: absolute;
    border: 0 solid transparent;

    /* Main pointer of touch: Reka UI takes its margin `coarse` on the same
       media query. */
    @media (pointer: coarse) {
      --hit-margin: var(--split-hit-margin-coarse);
    }
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

    /* The zone grabbed, left and right of the line. */
    .split-hit {
      inset-block: 0;
      inset-inline-end: calc(-1 * var(--hit-margin-after));
      border-inline-start-width: var(--line-width);
      padding-inline: var(--hit-margin) var(--hit-margin-after);
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

    /* The zone grabbed, above and below the line. */
    .split-hit {
      inset-inline: 0;
      inset-block-end: calc(-1 * var(--hit-margin-after));
      border-block-start-width: var(--line-width);
      padding-block: var(--hit-margin) var(--hit-margin-after);
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
