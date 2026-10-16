<!--
  Vertical handle placed on the right edge of the left panel: dragging it
  changes the width of the panel, a double click resets it to its default
  value. A grip, in the middle, shows where to grab it. The width is remembered from one visit to the next (`siderWidth.ts`).
-->
<script setup lang="ts">
import { ref } from 'vue'

import { DEFAULT_SIDER_WIDTH, clampSiderWidth, saveSiderWidth } from './siderWidth'

/**
 * Width of the panel, in pixels.
 */
const width = defineModel<number>({ required: true })

/**
 * Drag in progress: starting position of the pointer and starting width.
 */
const drag = ref<{ x: number; width: number } | null>(null)

/**
 * Starts the drag; the pointer stays captured even outside the handle.
 *
 * @param event Press on the handle.
 */
function start(event: PointerEvent): void {
  ;(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId)
  drag.value = { x: event.clientX, width: width.value }
}

/**
 * Follows the pointer during the drag.
 *
 * @param event Pointer move.
 */
function move(event: PointerEvent): void {
  if (drag.value !== null) {
    width.value = clampSiderWidth(
      drag.value.width + event.clientX - drag.value.x,
      window.innerWidth,
    )
  }
}

/**
 * Ends the drag and remembers the width.
 */
function stop(): void {
  if (drag.value !== null) {
    drag.value = null
    saveSiderWidth(width.value)
  }
}

/**
 * Resets the default width.
 */
function reset(): void {
  width.value = DEFAULT_SIDER_WIDTH
  saveSiderWidth(width.value)
}
</script>

<template>
  <div
    class="sider-resizer"
    :class="{ dragging: drag !== null }"
    :style="{ insetInlineStart: `calc(${width}px - var(--resize-handle-width) / 2)` }"
    title="Glisser pour élargir, double clic pour revenir à la position initiale"
    @pointerdown.prevent="start"
    @pointermove="move"
    @pointerup="stop"
    @pointercancel="stop"
    @dblclick="reset"
  />
</template>

<style scoped>
.sider-resizer {
  position: absolute;
  inset-block: 0;
  z-index: 10;
  inline-size: var(--resize-handle-width);
  cursor: col-resize;
  touch-action: none;

  /* Central grip. */
  &::after {
    position: absolute;
    inset-block-start: calc(50% - var(--grip-length) / 2);
    inset-inline-start: calc(50% - var(--grip-thickness) / 2);
    inline-size: var(--grip-thickness);
    block-size: var(--grip-length);
    border-radius: var(--grip-thickness);
    background: var(--grip-color);
    content: '';
  }

  &:hover,
  &.dragging {
    background: var(--resize-hover);

    &::after {
      background: var(--accent);
    }
  }
}
</style>
