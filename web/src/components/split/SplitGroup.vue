<!--
  Group of a splitter of Prisme (`splitLayout.ts`): panels (`SplitPanel`)
  side by side or one above the other, with lines between them
  (`SplitHandle`). The browser lays it out as a flexbox, mounted shown or
  hidden alike; the group only gives its panels and lines what they share,
  and measures CSS lengths for a drag with a hidden box of its own.

  A group whose panels are all folded, or that has no panel to fill it,
  leaves room free: an element marked `data-split-room` among the panels
  takes it where it stands (`grow`), and a line takes its room from it. Its
  growth follows the transition of the panels (`SplitPanel.vue`), so that
  the room and the last panel share the height continuously while it folds.
  During the drag of a line, the group is marked `data-resizing`: the
  panels and the room change size without a transition.
-->
<script setup lang="ts">
import { provide, ref, useTemplateRef } from 'vue'

import { SPLIT_GROUP, type SplitDirection, type SplitPanelEntry } from './splitLayout'

const props = defineProps<{
  /**
   * Panels side by side (`horizontal`) or one above the other (`vertical`).
   */
  direction: SplitDirection
}>()

/**
 * Element of the group.
 */
const root = useTemplateRef<HTMLElement>('root')

/**
 * Box that measures a CSS length: given as its width, read back in pixels.
 */
const probe = useTemplateRef<HTMLElement>('probe')

/**
 * Is a line of the group being dragged?
 */
const dragging = ref(false)

/**
 * Panels of the group, in the order they were mounted; sorted in the order of
 * the page when read.
 */
const entries: SplitPanelEntry[] = []

/**
 * Length of an element along the group, in pixels.
 *
 * @param element Element.
 */
function length(element: Element): number {
  const box = element.getBoundingClientRect()
  return props.direction === 'horizontal' ? box.width : box.height
}

provide(SPLIT_GROUP, {
  direction: () => props.direction,
  panels: () =>
    entries
      .filter((entry) => entry.element() !== null)
      .sort((a, b) =>
        a.element()!.compareDocumentPosition(b.element()!) & Node.DOCUMENT_POSITION_FOLLOWING
          ? -1
          : 1,
      ),
  register: (panel) => entries.push(panel),
  unregister: (panel) => {
    const index = entries.indexOf(panel)
    if (index >= 0) {
      entries.splice(index, 1)
    }
  },
  measure: (css) => {
    const element = probe.value
    if (element === null) {
      return 0
    }
    element.style.inlineSize = css
    return element.getBoundingClientRect().width
  },
  freeRoom: () => {
    const element = root.value
    if (element === null) {
      return 0
    }
    let used = 0
    for (const child of element.children) {
      if (child !== probe.value && !(child as HTMLElement).hasAttribute('data-split-room')) {
        used += length(child)
      }
    }
    return Math.max(0, length(element) - used)
  },
  resizing: (value) => {
    dragging.value = value
  },
})
</script>

<template>
  <div ref="root" class="split-group" :class="direction" :data-resizing="dragging || undefined">
    <slot />
    <div ref="probe" class="split-probe" aria-hidden="true" />
  </div>
</template>

<style scoped>
.split-group {
  position: relative;
  display: flex;
  overflow: hidden;

  &.horizontal {
    flex-direction: row;
  }

  &.vertical {
    flex-direction: column;
  }
}

/* The free room grows and shrinks with the panels (`SplitPanel.vue`). */
:slotted([data-split-room]) {
  transition-property: flex-grow;
  transition-duration: var(--default-transition-duration);
  transition-timing-function: var(--default-transition-timing-function);
}

[data-resizing] > :slotted([data-split-room]) {
  transition: none;
}

@media (prefers-reduced-motion: reduce) {
  :slotted([data-split-room]) {
    transition: none;
  }
}

.split-probe {
  position: absolute;
  visibility: hidden;
  pointer-events: none;
  block-size: 0;
}
</style>
