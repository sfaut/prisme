<!--
  Bubble of the tooltips, a single one for the whole page (mounted by
  `App.vue`): it shows the text that the `v-tooltip` directive gave to the
  element under the pointer, after a short hover, or to the element that
  takes the keyboard focus. A short text in a small font on a dark
  background, above its element, so that the pointer does not hide it;
  under it when there is no room above.

  Built on the browser's own tools: the `popover` attribute (`hint`: shows in
  the top layer, over the windows, and closes with Escape) and the anchor
  positioning of CSS, which places it against its element without measuring
  anything in script. Colors from the theme (`--tooltip-*`, `main.css`).
-->
<script setup lang="ts">
import { onBeforeUnmount, onMounted, useTemplateRef, watch } from 'vue'

import { TOOLTIP_ATTRIBUTE, shownTooltip } from './tooltip'

/**
 * Delay of the hover before the tooltip shows, in milliseconds.
 */
const SHOW_DELAY = 400

/**
 * Anchor name given to the element of the shown tooltip.
 */
const ANCHOR = '--tooltip'

/**
 * Element of the bubble, a popover.
 */
const bubble = useTemplateRef<HTMLElement>('bubble')

/**
 * Timer of the hover delay, `undefined` when none is running.
 */
let timer: ReturnType<typeof setTimeout> | undefined

/**
 * Element with a tooltip that contains the target of an event, if any.
 *
 * @param target Target of the event.
 */
function elementOf(target: EventTarget | null): HTMLElement | null {
  return target instanceof Element ? target.closest<HTMLElement>(`[${TOOLTIP_ATTRIBUTE}]`) : null
}

/**
 * Shows the tooltip of an element after `delay`, or hides the shown one.
 * Passing from a shown tooltip to another element shows the next one at once.
 *
 * @param element Element whose tooltip to show, `null` to hide.
 * @param delay Delay before it shows, in milliseconds.
 */
function schedule(element: HTMLElement | null, delay: number): void {
  clearTimeout(timer)
  if (element === shownTooltip.element) {
    return
  }
  if (element === null) {
    shownTooltip.element = null
    return
  }
  timer = setTimeout(
    () => {
      shownTooltip.text = element.getAttribute(TOOLTIP_ATTRIBUTE) ?? ''
      shownTooltip.element = element
    },
    shownTooltip.element ? 0 : delay,
  )
}

/**
 * The pointer enters an element: its tooltip after the hover delay, or none.
 *
 * @param event Event of the pointer.
 */
function onPointerOver(event: PointerEvent): void {
  schedule(elementOf(event.target), SHOW_DELAY)
}

/**
 * The pointer leaves the page.
 *
 * @param event Event of the pointer.
 */
function onPointerOut(event: PointerEvent): void {
  if (event.relatedTarget === null) {
    schedule(null, 0)
  }
}

/**
 * A press hides the tooltip, until the pointer enters another element.
 */
function onPointerDown(): void {
  clearTimeout(timer)
  shownTooltip.element = null
}

/**
 * The keyboard focus enters an element: its tooltip at once. Not for a text
 * field, where the bubble would cover what is being typed.
 *
 * @param event Event of the focus.
 */
function onFocusIn(event: FocusEvent): void {
  const target = event.target
  if (
    target instanceof HTMLElement &&
    target.matches(':focus-visible') &&
    !target.matches('input, textarea, [contenteditable]')
  ) {
    schedule(elementOf(target), 0)
  }
}

/**
 * The focus leaves the element of the shown tooltip.
 *
 * @param event Event of the focus.
 */
function onFocusOut(event: FocusEvent): void {
  if (shownTooltip.element !== null && elementOf(event.target) === shownTooltip.element) {
    schedule(null, 0)
  }
}

/**
 * The bubble closes by itself (Escape, another popover): the state follows.
 *
 * @param event Toggle event of the popover.
 */
function onToggle(event: ToggleEvent): void {
  // The event comes after the fact: the bubble may be open again, for another element.
  if (event.newState === 'closed' && !bubble.value?.matches(':popover-open')) {
    shownTooltip.element = null
  }
}

// The bubble follows the shown element: anchored to it, open while there is one.
watch(
  () => shownTooltip.element,
  (element, previous) => {
    previous?.style.removeProperty('anchor-name')
    if (bubble.value === null) {
      return
    }
    if (element === null) {
      if (bubble.value.matches(':popover-open')) {
        bubble.value.hidePopover()
      }
      return
    }
    // Reopened for each element: an open bubble keeps the position it fell
    // back to for the previous element (under it, near the top of the
    // window), even where the next one leaves room above.
    if (bubble.value.matches(':popover-open')) {
      bubble.value.hidePopover()
    }
    element.style.setProperty('anchor-name', ANCHOR)
    bubble.value.showPopover()
  },
  { flush: 'post' },
)

/**
 * Events listened to on the whole document, by type.
 */
const LISTENERS = {
  pointerover: onPointerOver,
  pointerout: onPointerOut,
  pointerdown: onPointerDown,
  focusin: onFocusIn,
  focusout: onFocusOut,
} as const

onMounted(() => {
  for (const [type, listener] of Object.entries(LISTENERS)) {
    document.addEventListener(type, listener as EventListener, true)
  }
})

onBeforeUnmount(() => {
  clearTimeout(timer)
  for (const [type, listener] of Object.entries(LISTENERS)) {
    document.removeEventListener(type, listener as EventListener, true)
  }
})
</script>

<template>
  <div ref="bubble" popover="hint" role="tooltip" class="ui-tooltip" @toggle="onToggle">
    {{ shownTooltip.text }}
  </div>
</template>

<style scoped>
@reference "@/assets/main.css";

/* Above its element, centered on it: the pointer, which covers the bottom of
   the element, does not hide the text. Near an edge of the window, it lines up
   with the element's side instead; under it when there is no room above. The
   gap stands on both sides, whichever side the bubble falls back to. */
.ui-tooltip {
  position-anchor: --tooltip;
  inset: auto;
  position-area: top;
  position-try-fallbacks:
    top span-right,
    top span-left,
    flip-block,
    bottom span-right,
    bottom span-left;
  margin: 0;
  margin-block: var(--popup-offset);
  max-inline-size: --spacing(72);
  padding: --spacing(1) --spacing(2);
  overflow: hidden;
  border: none;
  border-radius: var(--control-radius);
  background: var(--tooltip-bg);
  color: var(--tooltip-fg);
  font-size: var(--font-size-small);
  line-height: 1.4;
  white-space: pre-line;
  overflow-wrap: anywhere;
  /* A long value (a cell) shows its first lines only. */
  -webkit-box-orient: vertical;
  -webkit-line-clamp: 12;
  line-clamp: 12;
  /* The pointer passes through: the bubble never hides what it explains. */
  pointer-events: none;

  &:popover-open {
    display: -webkit-box;
  }
}
</style>
