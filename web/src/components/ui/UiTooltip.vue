<!--
  Tooltip: a short text in a small dark bubble, shown under its trigger (the
  default slot) after a short hover, or as soon as the trigger takes the
  focus; it goes above when there is no room below.

  Built on the browser's own tools: the `popover` attribute (`hint`: shows in
  the top layer, over the windows, and closes with Escape) and the anchor
  positioning of CSS, which places it against its trigger without measuring
  anything in script. Colors from the theme (`--tooltip-*`, `main.css`).
-->
<script setup lang="ts">
import { onBeforeUnmount, useId, useTemplateRef } from 'vue'

defineProps<{
  /**
   * Text of the tooltip.
   */
  text: string
}>()

/**
 * Delay of the hover before the tooltip shows, in milliseconds.
 */
const SHOW_DELAY = 400

/**
 * Identifier of the tooltip.
 */
const id = useId()

/**
 * Name of the anchor of the trigger, a CSS dashed ident: the identifier with
 * any character other than a letter, a digit or a dash replaced.
 */
const anchor = `--tip-${id.replace(/[^\w-]/g, '-')}`

/**
 * Element of the tooltip, a popover.
 */
const tip = useTemplateRef<HTMLElement>('tip')

/**
 * Timer of the hover delay, `undefined` when none is running.
 */
let timer: ReturnType<typeof setTimeout> | undefined

/**
 * Shows the tooltip, after `delay` milliseconds.
 *
 * @param delay Delay before it shows: that of the hover, none for the focus.
 */
function show(delay: number): void {
  clearTimeout(timer)
  timer = setTimeout(() => {
    if (tip.value && !tip.value.matches(':popover-open')) {
      tip.value.showPopover()
    }
  }, delay)
}

/**
 * Hides the tooltip, or cancels its coming display.
 */
function hide(): void {
  clearTimeout(timer)
  if (tip.value?.matches(':popover-open')) {
    tip.value.hidePopover()
  }
}

onBeforeUnmount(hide)
</script>

<template>
  <span
    class="inline-flex"
    :style="{ anchorName: anchor }"
    @pointerenter="show(SHOW_DELAY)"
    @pointerleave="hide"
    @pointerdown="hide"
    @focusin="show(0)"
    @focusout="hide"
  >
    <slot />
    <span
      :id="id"
      ref="tip"
      popover="hint"
      role="tooltip"
      class="ui-tooltip"
      :style="{ positionAnchor: anchor }"
    >
      {{ text }}
    </span>
  </span>
</template>

<style scoped>
@reference "@/assets/main.css";

/* Under the trigger, centered on it. Near an edge of the window, it lines up
   with the trigger's side instead; above it when there is no room below. */
.ui-tooltip {
  inset: auto;
  position-area: bottom;
  position-try-fallbacks:
    bottom span-right,
    bottom span-left,
    flip-block,
    top span-right,
    top span-left;
  margin: 0;
  margin-block-start: var(--popup-offset);
  max-inline-size: --spacing(72);
  padding: --spacing(1) --spacing(2);
  overflow: visible;
  border: none;
  border-radius: var(--control-radius);
  background: var(--tooltip-bg);
  color: var(--tooltip-fg);
  font-size: var(--font-size-small);
  line-height: 1.4;
}
</style>
