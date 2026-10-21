<!--
  Handle placed on the inner edge of a panel of the layout: the right edge
  of a panel on the left, the left edge of a panel on the right, the top
  edge of the console, at the bottom. Dragging it changes the size of the
  panel, its width or its height; a double click resets it to its default
  value. A grip, in the middle, shows where to grab it. The size is
  remembered from one visit to the next (`panelSize.ts`).
-->
<script setup lang="ts">
import { computed, ref } from 'vue'

import { type PanelSize, clampPanelSize, savePanelSize } from './panelSize'
import { vTooltip } from './ui/tooltip'

const props = defineProps<{
  /**
   * Bounds and memory of the panel's size.
   */
  panel: PanelSize
  /**
   * Edge of the window the panel stands against: `start` (left), `end`
   * (right) or `bottom`.
   */
  side: 'start' | 'end' | 'bottom'
}>()

/**
 * Size of the panel, in pixels: its width, or its height at the bottom.
 */
const size = defineModel<number>({ required: true })

/**
 * Drag in progress: starting position of the pointer and starting size.
 */
const drag = ref<{ position: number; size: number } | null>(null)

/**
 * Is the handle horizontal, on top of the console?
 */
const horizontal = computed(() => props.side === 'bottom')

/**
 * Position of the pointer along the direction of the size.
 *
 * @param event Pointer event.
 */
function position(event: PointerEvent): number {
  return horizontal.value ? event.clientY : event.clientX
}

/**
 * Starts the drag; the pointer stays captured even outside the handle.
 *
 * @param event Press on the handle.
 */
function start(event: PointerEvent): void {
  ;(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId)
  drag.value = { position: position(event), size: size.value }
}

/**
 * Follows the pointer during the drag: a panel on the left widens when the
 * pointer goes right, one on the right when it goes left, the console grows
 * when it goes up.
 *
 * @param event Pointer move.
 */
function move(event: PointerEvent): void {
  if (drag.value !== null) {
    const shift = position(event) - drag.value.position
    size.value = clampPanelSize(
      props.panel,
      drag.value.size + (props.side === 'start' ? shift : -shift),
      horizontal.value ? window.innerHeight : window.innerWidth,
    )
  }
}

/**
 * Ends the drag and remembers the size.
 */
function stop(): void {
  if (drag.value !== null) {
    drag.value = null
    savePanelSize(props.panel, size.value)
  }
}

/**
 * Resets the default size.
 */
function reset(): void {
  size.value = props.panel.defaultSize
  savePanelSize(props.panel, size.value)
}

/**
 * Inset of the handle in its container, along the edge of the panel.
 */
const inset = computed(() => {
  const at = `calc(${size.value}px - var(--resize-handle-width) / 2)`
  switch (props.side) {
    case 'start':
      return { insetInlineStart: at }
    case 'end':
      return { insetInlineEnd: at }
    case 'bottom':
      return { insetBlockEnd: at }
  }
  return {}
})
</script>

<template>
  <div
    class="panel-resizer"
    :class="{ dragging: drag !== null, horizontal }"
    :style="inset"
    v-tooltip="'Glisser pour redimensionner, double clic pour revenir à la taille initiale'"
    @pointerdown.prevent="start"
    @pointermove="move"
    @pointerup="stop"
    @pointercancel="stop"
    @dblclick="reset"
  />
</template>

<style scoped>
.panel-resizer {
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

  /* On top of the console: across its width. */
  &.horizontal {
    inset-block: auto;
    inset-inline: 0;
    inline-size: auto;
    block-size: var(--resize-handle-width);
    cursor: row-resize;

    &::after {
      inset-block-start: calc(50% - var(--grip-thickness) / 2);
      inset-inline-start: calc(50% - var(--grip-length) / 2);
      inline-size: var(--grip-length);
      block-size: var(--grip-thickness);
    }
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
